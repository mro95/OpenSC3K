//! OpenSC3K: the game. For now the startup flow, from the copyright splash to the main menu.

mod game;
mod window;

use game::{Event, Game, Scene};
use sc3k_assets::{Assets, DEFAULT_LANGUAGE};
use sc3k_audio::Audio;
use sc3k_formats::wav::Wav;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
usage: opensc3k [options]
  --res WxH            screen resolution (default 800x600; the original's UI art covers
                       640x480, 800x600 and 1024x768)
  --lang NAME          text language directory under Apps/Res/Text (default ENGLISH)
  --music-volume N     0 (off) to 1024 (default 1024)
  --screenshot FILE    render one frame to a PNG instead of opening a window
  --scene splash|menu|newcity
                       scene for --screenshot (default menu)
  --click X,Y          click (press and release) here before --screenshot; repeatable
  --type TEXT          type TEXT (after the clicks) before --screenshot
  --hover X,Y          pointer position for --screenshot
  --time MS            milliseconds of animation to simulate for --screenshot (default 0)

Reads the original game data from $SC3K_DATA.";

struct Options {
    width: i32,
    height: i32,
    lang: String,
    music_volume: u32,
    screenshot: Option<PathBuf>,
    scene: Scene,
    clicks: Vec<(i32, i32)>,
    typed: String,
    hover: Option<(i32, i32)>,
    time_ms: u64,
}

fn parse_args() -> Result<Options, String> {
    let mut o = Options {
        width: 800,
        height: 600,
        lang: DEFAULT_LANGUAGE.to_string(),
        music_volume: sc3k_audio::MAX_VOLUME,
        screenshot: None,
        scene: Scene::MainMenu,
        clicks: Vec::new(),
        typed: String::new(),
        hover: None,
        time_ms: 0,
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{a} needs a value\n\n{USAGE}"));
        match a.as_str() {
            "--res" => {
                let v = value()?;
                let (w, h) = v.split_once('x').ok_or_else(|| format!("bad resolution {v}"))?;
                o.width = w.parse().map_err(|_| format!("bad resolution {v}"))?;
                o.height = h.parse().map_err(|_| format!("bad resolution {v}"))?;
            }
            "--lang" => o.lang = value()?,
            "--music-volume" => o.music_volume = value()?.parse().map_err(|_| "bad --music-volume".to_string())?,
            "--screenshot" => o.screenshot = Some(PathBuf::from(value()?)),
            "--scene" => {
                o.scene = match value()?.as_str() {
                    "splash" => Scene::Splash { since_ms: 0 },
                    "menu" => Scene::MainMenu,
                    "newcity" => Scene::NewCity,
                    other => return Err(format!("unknown scene {other}")),
                }
            }
            "--hover" => o.hover = Some(parse_point(&value()?)?),
            "--click" => o.clicks.push(parse_point(&value()?)?),
            "--type" => o.typed = value()?,
            "--time" => o.time_ms = value()?.parse().map_err(|_| "bad --time".to_string())?,
            "-h" | "--help" => return Err(USAGE.to_string()),
            other => return Err(format!("unknown option {other}\n\n{USAGE}")),
        }
    }
    Ok(o)
}

fn parse_point(v: &str) -> Result<(i32, i32), String> {
    let bad = || format!("bad point {v}");
    let (x, y) = v.split_once(',').ok_or_else(bad)?;
    Ok((x.parse().map_err(|_| bad())?, y.parse().map_err(|_| bad())?))
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let o = parse_args()?;
    let assets = Assets::from_env(&o.lang).map_err(|e| e.to_string())?;
    let mut game = Game::new(&assets, o.width, o.height).map_err(|e| e.to_string())?;
    match &o.screenshot {
        Some(path) => screenshot(&mut game, &o, path),
        None => window::run(game, open_sound(&assets, o.music_volume)),
    }
}

/// Sound problems are not fatal: report them and carry on silently.
fn open_sound(assets: &Assets, music_volume: u32) -> window::Sound {
    let mut sound = window::Sound { audio: None, menu_loop: None, music_volume };
    if music_volume == 0 {
        return sound;
    }
    match Audio::open() {
        Ok(a) => sound.audio = Some(a),
        Err(e) => eprintln!("no sound: {e}"),
    }
    let path = format!("Res/Sound/{}", sc3k_audio::MENU_LOOP_PATH);
    match assets.file(&path).map_err(|e| e.to_string()).and_then(|d| Wav::parse(&d).map_err(|e| e.to_string())) {
        Ok(w) => sound.menu_loop = Some(w),
        Err(e) => eprintln!("{path}: {e}"),
    }
    sound
}

/// Render a frame headlessly, stepping animations in 1 ms increments.
fn screenshot(game: &mut Game, o: &Options, path: &Path) -> Result<(), String> {
    game.set_scene(o.scene);
    for &(x, y) in &o.clicks {
        game.handle(Event::MouseMove(x, y));
        game.handle(Event::MouseDown(x, y));
        game.handle(Event::MouseUp(x, y));
    }
    for c in o.typed.chars() {
        game.handle(Event::Text(c));
    }
    if let Some((x, y)) = o.hover {
        game.handle(Event::MouseMove(x, y));
    }
    for t in 0..=o.time_ms {
        if !matches!(o.scene, Scene::Splash { .. }) {
            game.update(t);
        }
    }
    game.draw();
    write_png(path, &game.screen).map_err(|e| format!("{}: {e}", path.display()))?;
    if let Some(id) = game.menu_mut().hovered() {
        println!("hovering window {id:08X}");
    }
    Ok(())
}

fn write_png(path: &Path, s: &sc3k_ui::Surface) -> Result<(), Box<dyn std::error::Error>> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut enc = png::Encoder::new(file, s.width as u32, s.height as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let rgb: Vec<u8> = s.pixels.iter().flat_map(|&p| [(p >> 16) as u8, (p >> 8) as u8, p as u8]).collect();
    enc.write_header()?.write_image_data(&rgb)?;
    Ok(())
}
