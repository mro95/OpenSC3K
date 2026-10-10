//! Inspect and extract SimCity 3000 Unlimited data files.

use sc3k_formats::image::Image;
use sc3k_formats::ixf::{is_ixf, Archive, Tgi};
use sc3k_formats::sprite::{Sprite, TYPE_DATA};
use sc3k_render::roads::{RoadSprites, RoadTile};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use sc3k_render::terrain::{TerrainScene, View, MAX_ZOOM};
use sc3k_sim::transit::Networks;
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
  sc3k-dump iso <seed> <size> <zoom> <out.png> [flora set]
                                     the same terrain drawn isometrically, whole map, at
                                     zoom 0..4, rotation 0, landscape 0, flora set 0 or the
                                     given FloraSets key, e.g. 0xc55a6d84
                                     (needs $SC3K_DATA)
  sc3k-dump iso-file <file> <zoom> <out.png> [flora set]
                                     the ground of a saved city (.sc3) or terrain (.sct)
                                     drawn the same way, trees from its flora layer and
                                     roads, rail, highways and power lines from its
                                     surface network
  sc3k-dump road-tile <id> <zoom> <out.png>
                                     one network tile's sprites at zoom 0..4, the four
                                     rotations side by side, layer 0 only (needs $SC3K_DATA)
  sc3k-dump diffref rng <script.txt> <out.txt>
  sc3k-dump diffref dirt <seed> <size> <difficulty> <hills> <water> <trees> <flags> <out.bin>
  sc3k-dump diffref qfs <dir>
  sc3k-dump diffref ui <script.txt> <out.txt>
  sc3k-dump diffref ground <file.sct|file.sc3> <outdir>
  sc3k-dump diffref bump <seed> <out.bin>
  sc3k-dump diffref tiling <list.txt> <out.txt>
  sc3k-dump diffref qfs-samples <root> <outdir> <limit>
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
        ["iso", seed, size, zoom, out] => iso(seed, size, zoom, Path::new(out), "0"),
        ["iso", seed, size, zoom, out, set] => iso(seed, size, zoom, Path::new(out), set),
        ["iso-file", file, zoom, out] => iso_file(Path::new(file), zoom, Path::new(out), "0"),
        ["iso-file", file, zoom, out, set] => iso_file(Path::new(file), zoom, Path::new(out), set),
        ["road-tile", id, zoom, out] => road_tile(id, zoom, Path::new(out)),
        ["terrain", seed, size, out] => terrain(seed, size, "1", Path::new(out)),
        ["terrain", seed, size, out, difficulty] => terrain(seed, size, difficulty, Path::new(out)),
        ["diffref", "rng", script, out] => diffref_rng(Path::new(script), Path::new(out)),
        ["diffref", "dirt", seed, size, difficulty, hills, water, trees, flags, out] => {
            diffref_dirt([seed, size, difficulty, hills, water, trees, flags], Path::new(out))
        }
        ["diffref", "qfs", dir] => diffref_qfs(Path::new(dir)),
        ["diffref", "ui", script, out] => diffref_ui(Path::new(script), Path::new(out)),
        ["diffref", "ground", file, outdir] => diffref_ground(Path::new(file), Path::new(outdir)),
        ["diffref", "bump", seed, out] => diffref_bump(seed, Path::new(out)),
        ["diffref", "tiling", list, out] => diffref_tiling(Path::new(list), Path::new(out)),
        ["diffref", "qfs-samples", root, out, limit] => {
            diffref_qfs_samples(Path::new(root), Path::new(out), limit)
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

fn iso(seed: &str, size: &str, zoom: &str, out: &Path, set: &str) -> Result<(), String> {
    use sc3k_sim::dirt::{generate, Params};
    let num = |s: &str| s.parse::<u32>().map_err(|e| format!("{s}: {e}"));
    let (seed, size) = (num(seed)?, num(size)?);
    if !(1..=256).contains(&size) {
        return Err(format!("size {size} out of range"));
    }
    let terrain = generate(size, 1, Params::new_city(seed));
    let trees = sc3k_sim::flora::place(&terrain, seed);
    draw_iso(terrain, &trees, &Networks::new(size), seed, zoom, out, set)
}

/// A saved city or terrain drawn like `iso`; the trees come from its flora layer and the
/// network tiles from its surface network.
fn iso_file(file: &Path, zoom: &str, out: &Path, set: &str) -> Result<(), String> {
    let archive = Archive::open(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let ground = sc3k_sim::load::read_ground(&archive).map_err(|e| format!("{}: {e}", file.display()))?;
    let t = &ground.terrain;
    println!("{}x{} cells, sea level {}", t.size, t.size, t.sea_level);
    let trees = sc3k_sim::flora::from_layer(t, &ground.flora_layer, 1);
    draw_iso(ground.terrain, &trees, &ground.networks, 1, zoom, out, set)
}

fn road_tile(id: &str, zoom: &str, out: &Path) -> Result<(), String> {
    const MARGIN: i32 = 8;
    let zoom = zoom.parse::<u32>().map_err(|e| format!("{zoom}: {e}"))?;
    if zoom > MAX_ZOOM {
        return Err(format!("zoom {zoom} out of range"));
    }
    let id = id.parse::<u32>().map_err(|e| format!("{id}: {e}"))?;

    let assets = sc3k_assets::Assets::from_env("ENGLISH").map_err(|e| e.to_string())?;
    let tile = RoadTile::load(&assets, id)?;

    // One box per rotation, side by side, each as wide as its frame.
    let mut infos = Vec::with_capacity(4);
    for rotation in 0..4 {
        infos.push(
            tile.info(zoom, rotation)
                .ok_or_else(|| format!("tile {id}: no frame for zoom {zoom}, rotation {rotation}"))?,
        );
    }
    let w = MARGIN + infos.iter().map(|i| i.left as i32 + i.right as i32 + MARGIN).sum::<i32>();
    let h = 2 * MARGIN + infos.iter().map(|i| i.up as i32 + i.down as i32).max().unwrap_or(0);

    let mut screen = sc3k_ui::Surface::new(w, h);
    screen.fill(0xFF00FF);
    let mut x = MARGIN;
    for (rotation, info) in (0..4).zip(&infos) {
        tile.draw(&mut screen, (x + info.left as i32, MARGIN + info.up as i32), zoom, rotation);
        x += info.left as i32 + info.right as i32 + MARGIN;
    }

    let rgba: Vec<u8> = screen.pixels.iter().flat_map(|&p| [(p >> 16) as u8, (p >> 8) as u8, p as u8, 255]).collect();
    write_png(out, w as u32, h as u32, &rgba).map_err(|e| format!("{}: {e}", out.display()))?;
    println!("tile {id}: {} frames, {w}x{h}, wrote {}", tile.frame_count(), out.display());
    Ok(())
}

fn draw_iso(
    terrain: sc3k_sim::dirt::Terrain,
    trees: &sc3k_sim::cellmap::CellMap<Option<sc3k_sim::flora::Flora>>,
    networks: &Networks,
    seed: u32,
    zoom: &str,
    out: &Path,
    set: &str,
) -> Result<(), String> {
    use sc3k_render::palette::{load_land_palettes, DirtPalettes};
    let zoom = zoom.parse::<u32>().map_err(|e| format!("{zoom}: {e}"))?;
    if zoom > MAX_ZOOM {
        return Err(format!("zoom {zoom} out of range"));
    }
    let size = terrain.size;
    let assets = sc3k_assets::Assets::from_env("ENGLISH").map_err(|e| e.to_string())?;
    let land = load_land_palettes(&assets)?;
    let land = land.get(&0).ok_or("no landscape 0")?;
    let dirt = DirtPalettes::load(&assets)?;
    let mut view = View { zoom, rotation: 0, origin_x: 0, origin_y: 0 };
    // The map spans size cells either way from the top corner, plus room for the relief
    // above and the edge skirts below.
    let (w, h) = (size as i32 * view.cell_width(), size as i32 * view.cell_height() + 300 * view.altitude_step());
    view.origin_x = w / 2;
    view.origin_y = 256 * view.altitude_step();
    let set = sc3k_formats::ini::parse_u32(set).ok_or_else(|| format!("flora set {set}"))?;
    let sprites = sc3k_render::flora::FloraSprites::load(&assets, set)?;
    let scene = TerrainScene::new(terrain, land, &dirt, seed);
    let mut screen = sc3k_ui::Surface::new(w, h);
    screen.fill(0);
    // One id per network cell; `RoadSprites::load` skips the repeats.
    let road_ids: Vec<u16> = (0..size)
        .flat_map(|x| (0..size).map(move |y| (x, y)))
        .filter_map(|(x, y)| networks.surface.get(x, y))
        .map(|tile| tile.tile_id)
        .collect();
    let road_sprites = RoadSprites::load(&assets, &road_ids)?;
    let roads = sc3k_render::roads::draw_cell(&road_sprites, &networks.surface, &view);
    let flora = sc3k_render::flora::draw_cell(&sprites, trees, &view);
    scene.draw_with(&mut screen, &view, |screen, draw, map| {
        roads(screen, draw, map);
        flora(screen, draw, map);
    });
    let n = (0..size).flat_map(|x| (0..size).map(move |y| (x, y))).filter(|&(x, y)| trees.get(x, y).is_some()).count();
    println!("{n} trees, {} network tiles", road_ids.len());
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

/// The port's main UI layout for `tools/diffcheck/run.py ui`. Each script line is a query,
/// answered by one output line, all numbers decimal:
/// - `place W H` and (w h) of the panel, navigator, RCI meter and bar, `0 0` for a missing
///   one: `place_windows`'s areas as `x y w h` per window in that order, `- - - -` if none;
/// - `button ID H`: `get_menu_btn_info_main` for a screen `H` high, as `x y w h
///   hover_group hover hover_open submenu_group submenu_image submenu_x submenu_y` (0 where
///   absent), or `none`;
/// - `bar W`: `get_layout_info`'s art group for a screen `W` wide, or `none`.
fn diffref_ui(script: &Path, out: &Path) -> Result<(), String> {
    use sc3k_ui::main_ui::{bar_group, main_button_info, place_windows, Sizes, GROUP_800};
    use sc3k_ui::Rect;
    let text = std::fs::read_to_string(script).map_err(|e| format!("{}: {e}", script.display()))?;
    let mut lines = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let words: Vec<&str> = line.split_whitespace().collect();
        let int = |i: usize| -> Result<i32, String> {
            let w = words.get(i).ok_or(format!("line {}: missing argument", n + 1))?;
            w.parse().map_err(|e| format!("line {}: {w}: {e}", n + 1))
        };
        let size = |i: usize| -> Result<Option<(i32, i32)>, String> {
            let (w, h) = (int(i)?, int(i + 1)?);
            Ok(((w, h) != (0, 0)).then_some((w, h)))
        };
        let answer = match words.first().copied() {
            None => continue,
            Some("place") => {
                let sizes = Sizes { panel: size(3)?, nav: size(5)?, rci: size(7)?, bar: size(9)? };
                let l = place_windows(Rect::new(0, 0, int(1)?, int(2)?), &sizes);
                let rect = |r: Option<Rect>| r.map_or("- - - -".to_string(), |r| format!("{} {} {} {}", r.x, r.y, r.w, r.h));
                [l.panel, l.nav, l.rci, l.bar].map(rect).join(" ")
            }
            Some("button") => match main_button_info(int(1)? as u32, int(2)?) {
                None => "none".to_string(),
                Some(b) => {
                    let ((g, i), (dx, dy)) = b.submenu.unwrap_or(((0, 0), (0, 0)));
                    let a = b.area;
                    let open = b.hover_open.unwrap_or(0);
                    format!("{} {} {} {} {GROUP_800} {} {open} {g} {i} {dx} {dy}", a.x, a.y, a.w, a.h, b.hover)
                }
            },
            Some("bar") => bar_group(int(1)?).map_or("none".to_string(), |g| g.to_string()),
            Some(q) => return Err(format!("line {}: unknown query {q}", n + 1)),
        };
        lines.push(answer + "\n");
    }
    std::fs::write(out, lines.concat()).map_err(|e| format!("{}: {e}", out.display()))
}

/// The port's tile sets for `tools/diffcheck/run.py tiling`. `list` names one file per line;
/// each gets a line of its tile ids in file order, space-separated, or `-` for none.
fn diffref_tiling(list: &Path, out: &Path) -> Result<(), String> {
    use sc3k_formats::tiling::parse_tile_set;
    let text = std::fs::read_to_string(list).map_err(|e| format!("{}: {e}", list.display()))?;
    let mut lines = Vec::new();
    for name in text.lines().filter(|l| !l.is_empty()) {
        let buf = std::fs::read(name).map_err(|e| format!("{name}: {e}"))?;
        let ids: Vec<String> = parse_tile_set(&buf).iter().map(u32::to_string).collect();
        lines.push(if ids.is_empty() { "-".to_string() } else { ids.join(" ") } + "\n");
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

/// The saved terrain of a `.sct`/`.sc3` for `tools/diffcheck/run.py ground`: writes the dirt
/// bag's record as stored (`<outdir>/record.bin`, the original's input) and the port's reading
/// of it (`<outdir>/port.bin`): "SC3KGREF", u32 version 2, vertices X, vertices Y, sea level;
/// then the altitude, water, vertex light and `GetVertexAltitude` maps, one byte per vertex,
/// column by column (`x * Y + y`); then one byte per cell, the same way: bit 0 `IsWater`,
/// bit 1 `IsRealWater`.
fn diffref_ground(file: &Path, outdir: &Path) -> Result<(), String> {
    use sc3k_sim::flora;
    use sc3k_sim::load::{read_dirt_bag, KEY_DIRT_BAG};
    let at = |e: &dyn std::fmt::Display| format!("{}: {e}", file.display());
    let archive = Archive::open(file).map_err(|e| at(&e))?;
    let seg = sc3k_formats::segment::Segment::open(&archive).map_err(|e| at(&e))?;
    let record = seg.get(KEY_DIRT_BAG).ok_or_else(|| at(&"no dirt bag record"))?;
    std::fs::create_dir_all(outdir).map_err(|e| format!("{}: {e}", outdir.display()))?;
    let write = |name: &str, b: &[u8]| {
        let p = outdir.join(name);
        std::fs::write(&p, b).map_err(|e| format!("{}: {e}", p.display()))
    };
    write("record.bin", record)?;
    let t = read_dirt_bag(record).map_err(|e| at(&e))?;
    let light = sc3k_render::light::vertex_light(&t);
    let v = t.vertices();
    let mut b = b"SC3KGREF".to_vec();
    for n in [2, v, v, t.sea_level as u32] {
        b.extend_from_slice(&n.to_le_bytes());
    }
    let cells = || (0..v).flat_map(|x| (0..v).map(move |y| (x, y)));
    b.extend(cells().map(|(x, y)| t.altitude.get(x, y)));
    b.extend(cells().map(|(x, y)| t.water.get(x, y)));
    b.extend(cells().map(|(x, y)| light.get(x, y)));
    b.extend(cells().map(|(x, y)| flora::vertex_altitude(&t, x, y)));
    let c = t.size;
    b.extend((0..c).flat_map(|x| (0..c).map(move |y| (x, y))).map(|(x, y)| {
        flora::is_water(&t, x, y) as u8 | (flora::is_real_water(&t, x, y) as u8) << 1
    }));
    write("port.bin", &b)
}

/// The bump maps for `tools/diffcheck/run.py ground`, with `seed` in place of the clock:
/// "SC3KBUMP", u32 version 1, then the land and the water map.
fn diffref_bump(seed: &str, out: &Path) -> Result<(), String> {
    let (land, water) = sc3k_render::terrain::bump_maps(num(seed)?);
    let mut b = b"SC3KBUMP".to_vec();
    b.extend_from_slice(&1u32.to_le_bytes());
    b.extend_from_slice(&land);
    b.extend_from_slice(&water);
    std::fs::write(out, b).map_err(|e| format!("{}: {e}", out.display()))
}

/// Decompresses every `<name>.qfs` in `dir` for `tools/diffcheck/run.py qfs`: writes
/// `<name>.out` with the decompressed bytes, or `<name>.err` with the error.
fn diffref_qfs(dir: &Path) -> Result<(), String> {
    use sc3k_formats::qfs;
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for e in entries {
        let path = e.map_err(|e| e.to_string())?.path();
        if path.extension().and_then(|x| x.to_str()) != Some("qfs") {
            continue;
        }
        let data = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let (dest, bytes) = match qfs::decompress(&data) {
            Ok(out) => (path.with_extension("out"), out),
            Err(err) => (path.with_extension("err"), err.to_string().into_bytes()),
        };
        std::fs::write(&dest, bytes).map_err(|e| format!("{}: {e}", dest.display()))?;
    }
    Ok(())
}

/// Copies up to `limit` QFS streams from the containers under `root`, spread evenly over all
/// of them, to `<outdir>/<n>.qfs`, with `<outdir>/index.tsv` naming each one's record. A
/// stream is a whole record, the pixels of an image record or those of a sprite record.
fn diffref_qfs_samples(root: &Path, out: &Path, limit: &str) -> Result<(), String> {
    use sc3k_formats::qfs::is_qfs;
    let limit = num(limit)? as usize;
    let mut files = Vec::new();
    walk(root, &mut files).map_err(|e| format!("{}: {e}", root.display()))?;
    files.sort();
    let mut found = Vec::new();
    for path in &files {
        let Ok(data) = std::fs::read(path) else { continue };
        if !is_ixf(&data) {
            continue;
        }
        let Ok(a) = Archive::from_bytes(data) else { continue };
        let rel = path.strip_prefix(root).unwrap_or(path).display().to_string();
        for e in a.entries() {
            let d = a.data(e);
            // Whole record, image pixels (24-byte header), sprite pixels (16 + 4 bytes).
            let at = if is_qfs(d) {
                0
            } else if Image::sniff(d) {
                24
            } else if d.len() > 20 && is_qfs(&d[20..]) {
                20
            } else {
                continue;
            };
            found.push((rel.clone(), e.tgi, d[at..].to_vec()));
        }
    }
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let step = (found.len() as f64 / limit.max(1) as f64).max(1.0);
    let mut index = String::new();
    let mut n = 0;
    while n < limit && ((n as f64 * step) as usize) < found.len() {
        let (file, tgi, stream) = &found[(n as f64 * step) as usize];
        let dest = out.join(format!("{n:05}.qfs"));
        std::fs::write(&dest, stream).map_err(|e| format!("{}: {e}", dest.display()))?;
        index += &format!("{n:05}\t{file}\t{tgi}\n");
        n += 1;
    }
    std::fs::write(out.join("index.tsv"), index).map_err(|e| e.to_string())?;
    println!("{n} of {} QFS streams written to {}", found.len(), out.display());
    Ok(())
}
