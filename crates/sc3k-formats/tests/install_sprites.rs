//! Decodes every sprite in the install and reads SYS.PAK. Skips when SC3K_DATA is unset.

use sc3k_formats::ini::{parse_u32_list, Ini};
use sc3k_formats::ixf::{is_ixf, Archive};
use sc3k_formats::pak::Pak;
use sc3k_formats::sprite::{Sprite, TYPE_DATA, TYPE_INFO};
use std::path::{Path, PathBuf};

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            files(&p, out);
        } else {
            out.push(p);
        }
    }
}

#[test]
fn every_sprite_decodes() {
    let Some(root) = sc3k_formats::data_dir() else { return };
    let mut paths = Vec::new();
    files(&root.join("Apps/Res/Sprites"), &mut paths);
    let (mut spans, mut masks) = (0, 0);
    for p in &paths {
        let data = std::fs::read(p).unwrap();
        if !is_ixf(&data) || !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dat")) {
            continue;
        }
        let a = Archive::from_bytes(data).unwrap();
        for e in a.entries() {
            match e.tgi.type_id {
                TYPE_DATA => match Sprite::parse(a.data(e)) {
                    Ok(Sprite::Span(_)) => spans += 1,
                    Ok(Sprite::Alpha(_)) => masks += 1,
                    Err(err) => panic!("{}: {}: {err}", p.display(), e.tgi),
                },
                TYPE_INFO => assert_eq!(a.data(e).len(), 8, "{}: {}", p.display(), e.tgi),
                // BuildingSets/*.dat also carry the sets' building records.
                _ => {}
            }
        }
    }
    assert!(spans > 60_000 && masks > 1_000, "{spans} span sprites, {masks} masks");
}

#[test]
fn sys_pak_has_city_schemes() {
    let Some(root) = sc3k_formats::data_dir() else { return };
    let pak = Pak::parse(std::fs::read(root.join("Apps/Sys/SYS.PAK")).unwrap()).unwrap();
    assert_eq!(pak.names().count(), 51);
    let ini = Ini::parse(&pak.lines("sc3cityscheme.ini").unwrap().unwrap());
    let landscapes = ini.section("LandScapes").unwrap();
    assert_eq!(landscapes.len(), 5);
    assert_eq!(landscapes[0].0, "0");
    assert_eq!(parse_u32_list(&landscapes[0].1).unwrap()[..3], [0x8553_5958, 0x1003, 0x455A_72B1]);
}
