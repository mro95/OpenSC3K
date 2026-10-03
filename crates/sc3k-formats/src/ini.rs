//! The INI dialect of the game's configuration files (`SYS.PAK`, `AUDIO.INI`, ...):
//! `[Section]` headers and `key=value` lines, kept in file order because some consumers
//! (e.g. `cSC3CitySchemeMgr`) list entries in the order they appear.

use crate::text::decode_cp1252;

#[derive(Clone, Debug, Default)]
pub struct Ini {
    sections: Vec<(String, Vec<(String, String)>)>,
}

impl Ini {
    pub fn parse<L: AsRef<[u8]>>(lines: &[L]) -> Ini {
        let mut ini = Ini::default();
        for line in lines {
            let line = decode_cp1252(line.as_ref());
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                ini.sections.push((name.trim().to_string(), Vec::new()));
            } else if let (Some((key, value)), Some((_, entries))) = (line.split_once('='), ini.sections.last_mut()) {
                entries.push((key.trim().to_string(), value.trim().to_string()));
            }
        }
        ini
    }

    /// Entries of `section` (name matched case-insensitively) in file order.
    pub fn section(&self, name: &str) -> Option<&[(String, String)]> {
        self.sections.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, e)| e.as_slice())
    }
}

/// `ConvertToUint32`: decimal, or hexadecimal with a `0x` prefix.
pub fn parse_u32(s: &str) -> Option<u32> {
    let s = s.trim();
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

/// A comma-separated list of numbers, as in `0=0x85535958,0x00001003,...`.
pub fn parse_u32_list(s: &str) -> Option<Vec<u32>> {
    s.split(',').map(parse_u32).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_file_order() {
        let ini = Ini::parse(&["[A]", "0xc=1, 2", "; note", "0=0x10", "", "[b]", "k = v"]);
        let a = ini.section("a").unwrap();
        assert_eq!(a[0], ("0xc".to_string(), "1, 2".to_string()));
        assert_eq!(a[1].0, "0");
        assert_eq!(ini.section("B").unwrap()[0].1, "v");
        assert_eq!(parse_u32_list("0x85535958,7").unwrap(), [0x8553_5958, 7]);
        assert_eq!(parse_u32("nope"), None);
    }
}
