//! Inspect and extract SimCity 3000 Unlimited data files.

use sc3k_formats::image::Image;
use sc3k_formats::ixf::{is_ixf, Archive, Tgi};
use sc3k_formats::sprite::{Sprite, TYPE_DATA};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
usage:
  sc3k-dump list <file>              print the TGI index of an IXF container
  sc3k-dump extract <file> <outdir>  write each entry to <outdir>/<T>-<G>-<I>.bin
  sc3k-dump images <file> <outdir>   decode each image entry to <outdir>/<T>-<G>-<I>.png
  sc3k-dump sprites <file> <outdir>  decode each sprite of a sprite .DAT to <outdir>/<G>-<I>.png
  sc3k-dump census [root]            group histogram over every container under root
                                     (root defaults to $SC3K_DATA)";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["list", file] => list(Path::new(file)),
        ["extract", file, out] => extract(Path::new(file), Path::new(out)),
        ["images", file, out] => images(Path::new(file), Path::new(out)),
        ["sprites", file, out] => sprites(Path::new(file), Path::new(out)),
        ["census"] => match sc3k_formats::data_dir() {
            Some(root) => census(&root),
            None => Err(format!("{} not set and no root given", sc3k_formats::DATA_ENV)),
        },
        ["census", root] => census(Path::new(root)),
        _ => Err(USAGE.to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn open(path: &Path) -> Result<Archive, String> {
    Archive::open(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn list(path: &Path) -> Result<(), String> {
    let a = open(path)?;
    // File order: group, instance, type.
    println!("{:<8} {:<8} {:<8} {:>10} {:>10}  head", "group", "instance", "type", "offset", "size");
    for e in a.entries() {
        let head: String = a.data(e).iter().take(8).map(|b| format!("{b:02x}")).collect();
        println!(
            "{:08X} {:08X} {:08X} {:>10} {:>10}  {head}",
            e.tgi.group_id, e.tgi.instance_id, e.tgi.type_id, e.offset, e.size
        );
    }
    println!("{} entries, terminator {:?}", a.entries().len(), a.terminator());
    Ok(())
}

fn extract(path: &Path, out: &Path) -> Result<(), String> {
    let a = open(path)?;
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    for e in a.entries() {
        let dest = out.join(format!("{}.bin", e.tgi));
        std::fs::write(&dest, a.data(e)).map_err(|err| format!("{}: {err}", dest.display()))?;
    }
    println!("extracted {} entries to {}", a.entries().len(), out.display());
    Ok(())
}

fn images(path: &Path, out: &Path) -> Result<(), String> {
    let a = open(path)?;
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let (mut written, mut skipped) = (0, 0);
    for e in a.entries() {
        let data = a.data(e);
        if !Image::sniff(data) {
            skipped += 1;
            continue;
        }
        let img = Image::parse(data).map_err(|err| format!("{}: {err}", e.tgi))?;
        let dest = out.join(format!("{}.png", e.tgi));
        write_png(&dest, img.width, img.height, &img.to_rgba8())
            .map_err(|err| format!("{}: {err}", dest.display()))?;
        written += 1;
    }
    println!("wrote {written} images to {} ({skipped} other entries)", out.display());
    Ok(())
}

fn sprites(path: &Path, out: &Path) -> Result<(), String> {
    let a = open(path)?;
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut written = 0;
    for e in a.entries().iter().filter(|e| e.tgi.type_id == TYPE_DATA) {
        let sprite = Sprite::parse(a.data(e)).map_err(|err| format!("{}: {err}", e.tgi))?;
        let dest = out.join(format!("{:08X}-{:08X}.png", e.tgi.group_id, e.tgi.instance_id));
        write_png(&dest, sprite.width(), sprite.height(), &sprite.to_rgba8())
            .map_err(|err| format!("{}: {err}", dest.display()))?;
        written += 1;
    }
    println!("wrote {written} sprites to {}", out.display());
    Ok(())
}

fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut enc = png::Encoder::new(file, width, height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(rgba)?;
    Ok(())
}

#[derive(Default)]
struct TypeStats {
    records: usize,
    bytes: u64,
    files: BTreeMap<String, usize>,
    example: Option<Tgi>,
}

fn census(root: &Path) -> Result<(), String> {
    let mut files = Vec::new();
    walk(root, &mut files).map_err(|e| format!("{}: {e}", root.display()))?;
    files.sort();

    let mut types: BTreeMap<u32, TypeStats> = BTreeMap::new();
    let mut archives = 0;
    for path in &files {
        let Ok(data) = std::fs::read(path) else { continue };
        if !is_ixf(&data) {
            continue;
        }
        let a = Archive::from_bytes(data).map_err(|e| format!("{}: {e}", path.display()))?;
        archives += 1;
        let rel = path.strip_prefix(root).unwrap_or(path).display().to_string();
        for e in a.entries() {
            let s = types.entry(e.tgi.group_id).or_default();
            s.records += 1;
            s.bytes += e.size as u64;
            *s.files.entry(rel.clone()).or_default() += 1;
            s.example.get_or_insert(e.tgi);
        }
    }

    let mut rows: Vec<_> = types.iter().collect();
    rows.sort_by(|a, b| b.1.records.cmp(&a.1.records));
    println!("{:<8} {:>7} {:>11} {:>6}  example file", "group", "records", "bytes", "files");
    for (t, s) in rows {
        let (file, _) = s.files.iter().max_by_key(|(_, n)| **n).unwrap();
        println!("{t:08X} {:>7} {:>11} {:>6}  {file}", s.records, s.bytes, s.files.len());
    }
    println!("{archives} archives, {} distinct groups", types.len());
    Ok(())
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        if p.is_dir() {
            walk(&p, out)?;
        } else {
            out.push(p);
        }
    }
    Ok(())
}
