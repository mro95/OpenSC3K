//! The city view's menus, from `Sys/MenuItem.INI` in SYS.PAK, read by `cSC3MenuMgr::Init`
//! (libSimUI Ghidra 0xC82A0). See `docs/ui/main-ui.md`.
//!
//! Four sections build the tree:
//! - `[SC3MenuItemInfo]`: what an item is called and the command it sends
//!   (`cSC3MenuItemInfo::init_version_1`);
//! - `[SC3MenuBtnDefs]`: a button, which shows an item and may open a submenu
//!   (`cSC3MenuBtnDef::InitFromRegistry`);
//! - `[SC3MenuDescs]`: a menu, which lists buttons (`cSC3MenuDesc::InitFromRegistry`);
//! - `[SC3MenuSets]` and `[SC3MSET_<id>]`: which menus a set uses (`cSC3MenuSet`).
//!
//! Every value is a comma-separated field list closed by `END`; the text after `END` is a
//! comment.
//! Unchecked: no Windows address known yet.

use std::collections::HashMap;
use std::fmt;

use sc3k_formats::ini::{parse_u32, Ini};

/// The root menu: the main panel's nine buttons.
pub const ROOT: u32 = 0xD100_1000;

#[derive(Debug)]
pub enum Error {
    MissingSection(&'static str),
    BadEntry { section: &'static str, key: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::MissingSection(s) => write!(f, "MenuItem.INI has no [{s}] section"),
            Error::BadEntry { section, key } => write!(f, "MenuItem.INI [{section}] {key}: bad entry"),
        }
    }
}

impl std::error::Error for Error {}

/// A string in a string table: (table group, string ID).
pub type StringRef = (u32, u32);

/// What choosing an item does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    /// `COMM` (a command ID with arguments) or `PMSG` (a message posted to the city).
    pub kind: String,
    pub data: [u32; 4],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub version: u32,
    pub name: StringRef,
    pub tip: StringRef,
    pub command: Command,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ButtonDef {
    /// The item the button shows and runs.
    pub item: u32,
    /// The menu it opens, if any.
    pub submenu: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Desc {
    /// The menu's title; (0, 0) for the root.
    pub name: StringRef,
    /// Two colours (`GetMenuDescColors`); zero in the root and the first-level menus.
    pub colors: (u32, u32),
    pub buttons: Vec<u32>,
}

#[derive(Clone, Debug, Default)]
pub struct Menus {
    pub items: HashMap<u32, Item>,
    pub buttons: HashMap<u32, ButtonDef>,
    pub descs: HashMap<u32, Desc>,
    /// Menu sets in file order: (set ID, menus it maps).
    pub sets: Vec<(u32, Vec<(u32, u32)>)>,
}

/// The fields of a value up to `END`, without the trailing comment.
fn fields(value: &str) -> Vec<&str> {
    let value = value.split(';').next().unwrap_or("");
    value.split(',').map(str::trim).take_while(|f| *f != "END").collect()
}

impl Menus {
    pub fn parse(ini: &Ini) -> Result<Menus, Error> {
        let section = |name: &'static str| ini.section(name).ok_or(Error::MissingSection(name));
        let mut menus = Menus::default();

        for (key, value) in section("SC3MenuItemInfo")? {
            let bad = || Error::BadEntry { section: "SC3MenuItemInfo", key: key.clone() };
            let f = fields(value);
            let n = |i: usize| f.get(i).and_then(|s| parse_u32(s)).ok_or_else(bad);
            let id = parse_u32(key).ok_or_else(bad)?;
            let item = Item {
                version: n(0)?,
                name: (n(1)?, n(2)?),
                tip: (n(3)?, n(4)?),
                command: Command {
                    kind: f.get(5).ok_or_else(bad)?.to_string(),
                    data: [n(6)?, n(7)?, n(8)?, n(9)?],
                },
            };
            menus.items.insert(id, item);
        }

        for (key, value) in section("SC3MenuBtnDefs")? {
            let bad = || Error::BadEntry { section: "SC3MenuBtnDefs", key: key.clone() };
            let f = fields(value);
            let n = |i: usize| f.get(i).and_then(|s| parse_u32(s)).ok_or_else(bad);
            let id = parse_u32(key).ok_or_else(bad)?;
            let submenu = Some(n(1)?).filter(|&d| d != 0);
            menus.buttons.insert(id, ButtonDef { item: n(0)?, submenu });
        }

        for (key, value) in section("SC3MenuDescs")? {
            let bad = || Error::BadEntry { section: "SC3MenuDescs", key: key.clone() };
            let f = fields(value);
            let n = |i: usize| f.get(i).and_then(|s| parse_u32(s)).ok_or_else(bad);
            let id = parse_u32(key).ok_or_else(bad)?;
            let count = n(4)? as usize;
            let buttons = (0..count).map(|i| n(5 + i)).collect::<Result<Vec<_>, _>>()?;
            menus.descs.insert(id, Desc { name: (n(0)?, n(1)?), colors: (n(2)?, n(3)?), buttons });
        }

        for (key, _) in section("SC3MenuSets")? {
            let bad = || Error::BadEntry { section: "SC3MenuSets", key: key.clone() };
            let id = key.strip_prefix("SC3MSET_").and_then(parse_u32).ok_or_else(bad)?;
            let mapping = ini
                .section(key)
                .unwrap_or_default()
                .iter()
                .map(|(k, v)| Some((parse_u32(k)?, parse_u32(fields(v).first()?)?)))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(bad)?;
            menus.sets.push((id, mapping));
        }
        Ok(menus)
    }

    /// The buttons of menu `desc`, in order.
    pub fn buttons_of(&self, desc: u32) -> &[u32] {
        self.descs.get(&desc).map_or(&[], |d| d.buttons.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[&str] = &[
        "[SC3MenuItemInfo]",
        "0x10002001=1,0x225872FE,0x00000016,0x225872FE,0x00000016,COMM,0x4253EC3F,0,0,0,END  ; Place Trees",
        "[SC3MenuBtnDefs]",
        "0x00001001=0x10001001,0xD1002000,END ; Root\\Landscaping",
        "0x00002001=0x10002001,0x00000000,END ; Root\\Landscaping\\Place Trees",
        "[SC3MenuDescs]",
        "0xD1002000=0x225872FE,0x00000001,0x00000000,0x00000000,1,0x00002001,END ; Landscape",
        "[SC3MenuSets]",
        "SC3MSET_0x00001000=0x00000000,0x00000000,Sys\\MenuItem.INI",
        "[SC3MSET_0x00001000]",
        "0xD1002000=0xD1002000 ; Sys: MenuBtnDefs for Root\\Landscaping",
    ];

    #[test]
    fn parses_each_section() {
        let m = Menus::parse(&Ini::parse(SAMPLE)).unwrap();
        let item = &m.items[&0x1000_2001];
        assert_eq!(item.name, (0x2258_72FE, 0x16));
        assert_eq!(item.command, Command { kind: "COMM".into(), data: [0x4253_EC3F, 0, 0, 0] });
        assert_eq!(m.buttons[&0x1001], ButtonDef { item: 0x1000_1001, submenu: Some(0xD100_2000) });
        assert_eq!(m.buttons[&0x2001].submenu, None);
        assert_eq!(m.buttons_of(0xD100_2000), &[0x2001]);
        assert_eq!(m.sets, vec![(0x1000, vec![(0xD100_2000, 0xD100_2000)])]);
    }

    #[test]
    fn install_menus() {
        let Ok(assets) = sc3k_assets::Assets::from_env("ENGLISH") else { return };
        let m = Menus::parse(&assets.sys_ini("MenuItem.INI").unwrap()).unwrap();
        let root = m.buttons_of(ROOT);
        assert_eq!(root, &[0x1001, 0x1002, 0x1003, 0x1004, 0x1005, 0x1006, 0x1007, 0x1008, 0x1009]);
        // Every button of every menu is defined, and every submenu is a menu.
        for d in m.descs.values() {
            for b in &d.buttons {
                let def = m.buttons.get(b).unwrap_or_else(|| panic!("button {b:#x}"));
                if let Some(s) = def.submenu {
                    assert!(m.descs.contains_key(&s), "submenu {s:#x}");
                }
            }
        }
        assert_eq!(m.items.len(), 90);
        assert_eq!(m.descs.len(), 21);
    }
}
