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
                                     (root defaults to $SC3K_DATA)
  sc3k-dump terrain <seed> <size> <out.png> [difficulty]
                                     generate new-city terrain and write a top-down map
                                     (size 64..256 cells, difficulty 1..3, default 1)
  sc3k-dump iso <seed> <size> <zoom> <out.png>
                                     the same terrain drawn isometrically, whole map, at
                                     zoom 0..4, rotation 0, landscape 0 (needs $SC3K_DATA)
  sc3k-dump diffref rng <script.txt> <out.txt>
  sc3k-dump diffref dirt <seed> <size> <difficulty> <hills> <water> <trees> <flags> <out.bin>
                                     reference output of the port for tools/diffcheck";

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
        ["iso", seed, size, zoom, out] => iso(seed, size, zoom, Path::new(out)),
        ["terrain", seed, size, out] => terrain(seed, size, "1", Path::new(out)),
        ["terrain", seed, size, out, difficulty] => terrain(seed, size, difficulty, Path::new(out)),
        ["diffref", "rng", script, out] => diffref_rng(Path::new(script), Path::new(out)),
        ["diffref", "dirt", seed, size, difficulty, hills, water, trees, flags, out] => {
            diffref_dirt([seed, size, difficulty, hills, water, trees, flags], Path::new(out))
        }
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

/// A decimal or `0x` hexadecimal number.
fn num(s: &str) -> Result<u32, String> {
    let r = match s.strip_prefix("0x") {
        Some(hex) => u32::from_str_radix(hex, 16),
        None => s.parse(),
    };
    r.map_err(|e| format!("{s}: {e}"))
}

fn terrain(seed: &str, size: &str, difficulty: &str, out: &Path) -> Result<(), String> {
    use sc3k_sim::dirt::{generate, Params};
    let (seed, size, difficulty) = (num(seed)?, num(size)?, num(difficulty)?);
    if !(1..=256).contains(&size) {
        return Err(format!("size {size} outside 1..=256"));
    }
    let t = generate(size, difficulty as i32, Params::new_city(seed));
    let v = t.vertices();
    let mut rgba = Vec::with_capacity((v * v * 4) as usize);
    let (mut water, mut trees) = (0, 0);
    // Row y, column x: y = 0 at the top.
    for y in 0..v {
        for x in 0..v {
            if t.is_water(x, y) {
                water += 1;
            } else if t.flora.get(x, y) > 0 {
                trees += 1;
            }
            let [r, g, b] = t.preview_rgb(x, y);
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
    }
    write_png(out, v, v, &rgba).map_err(|e| format!("{}: {e}", out.display()))?;
    let (lo, hi) = (0..v)
        .flat_map(|x| (0..v).map(move |y| (x, y)))
        .map(|(x, y)| t.altitude.get(x, y))
        .fold((255u8, 0u8), |(lo, hi), a| (lo.min(a), hi.max(a)));
    println!(
        "seed {seed:#010x}, {size}x{size} cells, sea level {}, altitude {lo}..={hi}, \
         {water} water and {trees} flora vertices of {}; wrote {}",
        t.sea_level,
        v * v,
        out.display()
    );
    Ok(())
}

fn iso(seed: &str, size: &str, zoom: &str, out: &Path) -> Result<(), String> {
    use sc3k_render::palette::{load_land_palettes, DirtPalettes};
    use sc3k_render::terrain::{TerrainScene, View, MAX_ZOOM};
    use sc3k_sim::dirt::{generate, Params};
    let num = |s: &str| s.parse::<u32>().map_err(|e| format!("{s}: {e}"));
    let (seed, size, zoom) = (num(seed)?, num(size)?, num(zoom)?);
    if !(1..=256).contains(&size) || zoom > MAX_ZOOM {
        return Err(format!("size {size} or zoom {zoom} out of range"));
    }
    let assets = sc3k_assets::Assets::from_env("ENGLISH").map_err(|e| e.to_string())?;
    let land = load_land_palettes(&assets)?;
    let land = land.get(&0).ok_or("no landscape 0")?;
    let dirt = DirtPalettes::load(&assets)?;
    let terrain = generate(size, 1, Params::new_city(seed));
    let mut view = View { zoom, rotation: 0, origin_x: 0, origin_y: 0 };
    // The map spans size cells either way from the top corner, plus room for the relief
    // above and the edge skirts below.
    let (w, h) = (size as i32 * view.cell_width(), size as i32 * view.cell_height() + 300 * view.altitude_step());
    view.origin_x = w / 2;
    view.origin_y = 256 * view.altitude_step();
    let scene = TerrainScene::new(terrain, land, &dirt, seed);
    let mut screen = sc3k_ui::Surface::new(w, h);
    screen.fill(0);
    scene.draw(&mut screen, &view);
    let rgba: Vec<u8> = screen.pixels.iter().flat_map(|&p| [(p >> 16) as u8, (p >> 8) as u8, p as u8, 255]).collect();
    write_png(out, w as u32, h as u32, &rgba).map_err(|e| format!("{}: {e}", out.display()))?;
    println!("{w}x{h}, wrote {}", out.display());
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

/// Replays a `cRZRandom` call script for `tools/diffcheck/run.py rng`. Each script line is
/// `seed <s>`, `next_u32`, `uniform <n>`, `range <lo> <hi>`, `gaussian_fast <lo> <hi>`,
/// `range_min_of_two <lo> <hi>`, `double` or `double_range <lo> <hi>`; integers are `u32` in
/// hex (two's complement for negative values), doubles are their bits in hex. Each output line
/// is the result (in the same encoding) of the matching script line.
fn diffref_rng(script: &Path, out: &Path) -> Result<(), String> {
    use sc3k_sim::rng::Random;
    let text = std::fs::read_to_string(script).map_err(|e| format!("{}: {e}", script.display()))?;
    let mut rng = Random::new(0);
    let mut lines = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let words: Vec<&str> = line.split_whitespace().collect();
        let hex = |i: usize| -> Result<u64, String> {
            let w = words.get(i).ok_or(format!("line {}: missing argument", n + 1))?;
            u64::from_str_radix(w, 16).map_err(|e| format!("line {}: {w}: {e}", n + 1))
        };
        let int = |i| hex(i).map(|v| v as u32 as i32);
        let real = |i| hex(i).map(f64::from_bits);
        let result: u64 = match words.first().copied() {
            None => continue,
            Some("seed") => {
                rng.seed(hex(1)? as u32);
                0
            }
            Some("next_u32") => rng.next_u32() as u64,
            Some("uniform") => rng.uniform(hex(1)? as u32) as u64,
            Some("range") => rng.range(int(1)?, int(2)?) as u32 as u64,
            Some("gaussian_fast") => rng.gaussian_fast(int(1)?, int(2)?) as u32 as u64,
            Some("range_min_of_two") => rng.range_min_of_two(int(1)?, int(2)?) as u32 as u64,
            Some("double") => rng.double().to_bits(),
            Some("double_range") => rng.double_range(real(1)?, real(2)?).to_bits(),
            Some(op) => return Err(format!("line {}: unknown call {op}", n + 1)),
        };
        lines.push(format!("{result:x}\n"));
    }
    std::fs::write(out, lines.concat()).map_err(|e| format!("{}: {e}", out.display()))
}

/// The port's terrain and `cRZRandom` call trace for `tools/diffcheck/run.py dirt`.
///
/// Layout, little-endian: `b"SC3KDREF"`, u32 version (1), u32 vertices X, u32 vertices Y,
/// u32 sea level; then the altitude, water, flora and salt (0 or 1) maps as one byte per
/// vertex, column by column (`x * Y + y`, like `cRZCellMap`); then u32 call count and per call
/// u32 op (`sc3k_sim::rng::Op`), u64 a, u64 b, u32 state, u32 double state.
fn diffref_dirt(args: [&&str; 7], out: &Path) -> Result<(), String> {
    use sc3k_sim::dirt::{generate_traced, Params};
    let [seed, size, difficulty, hills, water, trees, flags] = args.map(|s| num(s));
    let size = size?;
    if !(1..=256).contains(&size) {
        return Err(format!("size {size} outside 1..=256"));
    }
    let byte = |v: Result<u32, String>| v.map(|v| v as u8);
    let params = Params {
        seed: seed?,
        hills: byte(hills)?,
        water: byte(water)?,
        trees: byte(trees)?,
        flags: byte(flags)?,
    };
    let (t, calls) = generate_traced(size, difficulty? as i32, params);
    let v = t.vertices();
    let mut b = b"SC3KDREF".to_vec();
    for n in [1, v, v, t.sea_level as u32] {
        b.extend_from_slice(&n.to_le_bytes());
    }
    let cells = || (0..v).flat_map(|x| (0..v).map(move |y| (x, y)));
    b.extend(cells().map(|(x, y)| t.altitude.get(x, y)));
    b.extend(cells().map(|(x, y)| t.water.get(x, y)));
    b.extend(cells().map(|(x, y)| t.flora.get(x, y)));
    b.extend(cells().map(|(x, y)| t.salt.get(x, y) as u8));
    b.extend_from_slice(&(calls.len() as u32).to_le_bytes());
    for c in calls {
        b.extend_from_slice(&(c.op as u32).to_le_bytes());
        b.extend_from_slice(&c.a.to_le_bytes());
        b.extend_from_slice(&c.b.to_le_bytes());
        b.extend_from_slice(&c.state.to_le_bytes());
        b.extend_from_slice(&c.double_state.to_le_bytes());
    }
    std::fs::write(out, b).map_err(|e| format!("{}: {e}", out.display()))
}
