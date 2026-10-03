//! Decodes every UI image and font in the install. Skips when SC3K_DATA is unset.

use sc3k_formats::fbf::Font;
use sc3k_formats::image::Image;
use sc3k_formats::ixf::Archive;
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

fn ext_is(p: &Path, ext: &str) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

#[test]
fn every_ui_image_decodes() {
    let Some(root) = sc3k_formats::data_dir() else { return };
    let mut paths = Vec::new();
    files(&root.join("Apps/Res/UI"), &mut paths);
    let mut decoded = 0;
    for p in paths.iter().filter(|p| ext_is(p, "ixf")) {
        let a = Archive::open(p).unwrap();
        for e in a.entries() {
            let data = a.data(e);
            assert!(Image::sniff(data), "{}: {} is not an image", p.display(), e.tgi);
            let img = Image::parse(data).unwrap_or_else(|err| panic!("{}: {}: {err}", p.display(), e.tgi));
            assert_eq!(img.pixels.len(), (img.width * img.height) as usize);
            decoded += 1;
        }
    }
    assert!(decoded > 700, "only {decoded} images");
}

#[test]
fn every_font_parses() {
    let Some(root) = sc3k_formats::data_dir() else { return };
    let mut paths = Vec::new();
    files(&root.join("Apps/Res/Text"), &mut paths);
    let fonts: Vec<_> = paths.iter().filter(|p| ext_is(p, "fbf")).collect();
    assert!(!fonts.is_empty());
    for p in fonts {
        let f = Font::parse(&std::fs::read(p).unwrap()).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        assert!(f.line_height() > 0, "{}", p.display());
        for g in &f.glyphs {
            assert!(g.left >= 0 && g.top >= 0 && g.right <= f.width as i32 && g.bottom <= f.height as i32);
        }
    }
}
