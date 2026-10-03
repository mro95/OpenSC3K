//! Resource lookup over the original install: mounts the UI and text archives and resolves
//! resources the way the game addresses them, by (group, instance).
//!
//! The game's code names resources by group and instance and leaves the type to the loader
//! (e.g. `cSC3Buffer(0x22729921, 0x82B9B75C, …)` for the title background: instance, group).
//! The type is implied by what is being loaded (`62B9DA24` UI image, `2026960B` string).

use sc3k_formats::fbf::Font;
use sc3k_formats::image::Image;
use sc3k_formats::ini::Ini;
use sc3k_formats::ixf::Archive;
use sc3k_formats::pak::Pak;
use sc3k_formats::sprite::{Sprite, TYPE_DATA};
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum Error {
    NoDataDir,
    Missing { what: String },
    Io { path: PathBuf, err: std::io::Error },
    Ixf { path: PathBuf, err: sc3k_formats::ixf::Error },
    Image { group: u32, instance: u32, err: sc3k_formats::image::Error },
    Font { path: PathBuf, err: sc3k_formats::fbf::Error },
    Sprite { group: u32, instance: u32, err: sc3k_formats::sprite::Error },
    Pak { path: PathBuf, err: sc3k_formats::pak::Error },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NoDataDir => write!(
                f,
                "{} is not set; point it at your SimCity 3000 Unlimited install",
                sc3k_formats::DATA_ENV
            ),
            Error::Missing { what } => write!(f, "not found in the install: {what}"),
            Error::Io { path, err } => write!(f, "{}: {err}", path.display()),
            Error::Ixf { path, err } => write!(f, "{}: {err}", path.display()),
            Error::Image { group, instance, err } => write!(f, "image {group:08X}/{instance:08X}: {err}"),
            Error::Font { path, err } => write!(f, "{}: {err}", path.display()),
            Error::Sprite { group, instance, err } => write!(f, "sprite {group:08X}/{instance:08X}: {err}"),
            Error::Pak { path, err } => write!(f, "{}: {err}", path.display()),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// Default language directory under `Apps/Res/Text`.
pub const DEFAULT_LANGUAGE: &str = "ENGLISH";

/// The sprite archive holding the UI's sprites (New City previews, ...). The city's sprite
/// archives will be mounted alongside it once the renderer needs them.
const UI_SPRITES: &str = "GAME_UI.DAT";

pub struct Assets {
    apps: PathBuf,
    language: String,
    archives: Vec<Archive>,
    /// (group, instance) -> (archive, entry). Earlier archives win, as in a search path.
    index: HashMap<(u32, u32), (usize, usize)>,
    sprite_archives: Vec<Archive>,
    /// (group, instance) -> (sprite archive, entry) of each sprite's pixel record.
    sprite_index: HashMap<(u32, u32), (usize, usize)>,
    sys: Pak,
}

impl Assets {
    /// Mount from `$SC3K_DATA`.
    pub fn from_env(language: &str) -> Result<Assets> {
        let root = sc3k_formats::data_dir().ok_or(Error::NoDataDir)?;
        Assets::open(&root, language)
    }

    /// Mount the UI (`Apps/Res/UI/Shared`) and text (`Apps/Res/Text/<language>`) archives of
    /// the install at `root`.
    pub fn open(root: &Path, language: &str) -> Result<Assets> {
        let apps = find_ci(root, "Apps")?;
        let res = find_ci(&apps, "Res")?;
        let ui = find_ci(&find_ci(&res, "UI")?, "Shared")?;
        let text = find_ci(&find_ci(&res, "Text")?, language)?;
        let sys_path = find_ci(&find_ci(&apps, "Sys")?, "SYS.PAK")?;
        let sys_data = std::fs::read(&sys_path).map_err(|err| Error::Io { path: sys_path.clone(), err })?;
        let sys = Pak::parse(sys_data).map_err(|err| Error::Pak { path: sys_path, err })?;
        let mut assets = Assets {
            apps,
            language: language.to_string(),
            archives: Vec::new(),
            index: HashMap::new(),
            sprite_archives: Vec::new(),
            sprite_index: HashMap::new(),
            sys,
        };
        for dir in [&text, &ui] {
            for path in sorted_files(dir)? {
                let is_ixf = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("ixf"));
                if is_ixf {
                    assets.mount(&path)?;
                }
            }
        }
        assets.mount_sprites(&find_ci(&find_ci(&res, "Sprites")?, UI_SPRITES)?)?;
        Ok(assets)
    }

    fn mount_sprites(&mut self, path: &Path) -> Result<()> {
        let archive = Archive::open(path).map_err(|err| Error::Ixf { path: path.to_path_buf(), err })?;
        let n = self.sprite_archives.len();
        for (i, e) in archive.entries().iter().enumerate() {
            if e.tgi.type_id == TYPE_DATA {
                self.sprite_index.entry((e.tgi.group_id, e.tgi.instance_id)).or_insert((n, i));
            }
        }
        self.sprite_archives.push(archive);
        Ok(())
    }

    fn mount(&mut self, path: &Path) -> Result<()> {
        let archive = Archive::open(path).map_err(|err| Error::Ixf { path: path.to_path_buf(), err })?;
        let n = self.archives.len();
        for (i, e) in archive.entries().iter().enumerate() {
            self.index.entry((e.tgi.group_id, e.tgi.instance_id)).or_insert((n, i));
        }
        self.archives.push(archive);
        Ok(())
    }

    pub fn language(&self) -> &str {
        &self.language
    }

    /// Raw bytes of the resource (group, instance).
    pub fn raw(&self, group: u32, instance: u32) -> Option<&[u8]> {
        let &(a, e) = self.index.get(&(group, instance))?;
        let archive = &self.archives[a];
        Some(archive.data(&archive.entries()[e]))
    }

    pub fn image(&self, group: u32, instance: u32) -> Result<Image> {
        let data = self
            .raw(group, instance)
            .ok_or_else(|| Error::Missing { what: format!("image {group:08X}/{instance:08X}") })?;
        Image::parse(data).map_err(|err| Error::Image { group, instance, err })
    }

    /// Sprite (group, instance) from the mounted sprite archives.
    pub fn sprite(&self, group: u32, instance: u32) -> Result<Sprite> {
        let &(a, e) = self
            .sprite_index
            .get(&(group, instance))
            .ok_or_else(|| Error::Missing { what: format!("sprite {group:08X}/{instance:08X}") })?;
        let archive = &self.sprite_archives[a];
        Sprite::parse(archive.data(&archive.entries()[e])).map_err(|err| Error::Sprite { group, instance, err })
    }

    /// A configuration file from `Apps/Sys/SYS.PAK`, e.g. `SC3CityScheme.ini`.
    pub fn sys_ini(&self, name: &str) -> Result<Ini> {
        let missing = || Error::Missing { what: format!("SYS.PAK/{name}") };
        let lines = self.sys.lines(name).ok_or_else(missing)?;
        let lines = lines.map_err(|err| Error::Pak { path: PathBuf::from(name), err })?;
        Ok(Ini::parse(&lines))
    }

    /// String `id` of string table `table` (the table's group), as Windows-1252 bytes.
    pub fn string(&self, table: u32, id: u32) -> Result<Vec<u8>> {
        self.raw(table, id)
            .and_then(sc3k_formats::text::parse_string)
            .map(<[u8]>::to_vec)
            .ok_or_else(|| Error::Missing { what: format!("string {table:08X}/{id:X}") })
    }

    /// A loose file under `Apps`, e.g. `Res/Sound/Music/3kloop.wav`. Each path component is
    /// matched case-insensitively; `\` separators (as in the game's own paths) work too.
    pub fn file(&self, path: &str) -> Result<Vec<u8>> {
        let p = self.resolve(path)?;
        std::fs::read(&p).map_err(|err| Error::Io { path: p, err })
    }

    /// An IXF archive under `Apps` that is not mounted, e.g.
    /// `Res/Dirt/PALETTE/455A72B1_LandPalettes.IXF`. Paths work as in [`Assets::file`].
    pub fn archive(&self, path: &str) -> Result<Archive> {
        let p = self.resolve(path)?;
        Archive::open(&p).map_err(|err| Error::Ixf { path: p, err })
    }

    fn resolve(&self, path: &str) -> Result<PathBuf> {
        let mut p = self.apps.clone();
        for part in path.split(['/', '\\']).filter(|s| !s.is_empty()) {
            p = find_ci(&p, part)?;
        }
        Ok(p)
    }

    /// A bitmap font from `Apps/Res/Text/<language>/<file>` (name matched case-insensitively).
    pub fn font(&self, file: &str) -> Result<Font> {
        let dir = find_ci(&find_ci(&find_ci(&self.apps, "Res")?, "Text")?, &self.language)?;
        let path = find_ci(&dir, file)?;
        let data = std::fs::read(&path).map_err(|err| Error::Io { path: path.clone(), err })?;
        Font::parse(&data).map_err(|err| Error::Font { path, err })
    }
}

/// `dir/name`, matching `name` case-insensitively (the install mixes `.IXF`/`.ixf`, `UI`/`ui`).
fn find_ci(dir: &Path, name: &str) -> Result<PathBuf> {
    let exact = dir.join(name);
    if exact.exists() {
        return Ok(exact);
    }
    let read = std::fs::read_dir(dir).map_err(|err| Error::Io { path: dir.to_path_buf(), err })?;
    read.filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.file_name().is_some_and(|f| f.to_string_lossy().eq_ignore_ascii_case(name)))
        .ok_or_else(|| Error::Missing { what: exact.display().to_string() })
}

fn sorted_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let read = std::fs::read_dir(dir).map_err(|err| Error::Io { path: dir.to_path_buf(), err })?;
    let mut files: Vec<PathBuf> = read.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_file()).collect();
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_main_menu_resources() {
        let Ok(assets) = Assets::from_env(DEFAULT_LANGUAGE) else {
            eprintln!("SC3K_DATA not set or install unreadable; skipping");
            return;
        };
        let bg = assets.image(0x82B9_B75C, 0x2272_9921).unwrap();
        assert_eq!((bg.width, bg.height), (800, 600));
        assert_eq!(assets.string(0x03C0_9AFF, 6).unwrap(), b"Exit");
        let font = assets.font("serif17.fbf").unwrap();
        assert_eq!(font.line_height(), 29);
        assert!(assets.file("Res\\Sound\\Music\\3kloop.wav").unwrap().starts_with(b"RIFF"));
        assert_eq!(assets.sprite(0, 0x3FC4_0000).unwrap().width(), 191);
        assert!(assets.sys_ini("SC3CityScheme.ini").unwrap().section("FloraSets").is_some());
    }
}
