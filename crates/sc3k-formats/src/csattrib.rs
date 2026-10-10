//! City sprite attributes (`cSC3CitySpriteAttrib`, type [`TYPE_SPRITE_ATTRIB`]): which sprite
//! an occupant shows at each zoom and rotation. See `docs/formats/occupant.md`.
//!
//! Base records are in `Res/Sprites/CSATTRIB.IXF`; flora and building sets carry their own.
//! Only the binary form (`BIN\r`, `SerialReadBinary`, libSimSpr Ghidra 0x7B738) occurs.
//! Unchecked: no check runs the SIMSPR.DLL readers yet.

use std::fmt;

pub const TYPE_SPRITE_ATTRIB: u32 = 0x6300;
const MAGIC: &[u8; 4] = b"BIN\r";
const HEADER_LEN: usize = 24;
const ENTRY_LEN: usize = 15;

#[derive(Debug)]
pub enum Error {
    BadMagic,
    Truncated,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadMagic => write!(f, "sprite attributes are not binary (BIN)"),
            Error::Truncated => write!(f, "sprite attributes truncated"),
        }
    }
}

impl std::error::Error for Error {}

/// One sprite of the set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Sprite archive group (the `.DAT` file's prefix, e.g. 9 for `00000009_Landscape.DAT`).
    pub group: u32,
    pub instance: u32,
    /// 0: as is. Positive: drawn that many times larger; negative: that many times smaller
    /// (`SprAttDraw` scales the image info the same way).
    pub scale: i8,
    /// Offset used by `SprAttDrawRegFrame` only.
    pub dx: i16,
    pub dy: i16,
}

#[derive(Clone, Debug)]
pub struct SpriteAttrib {
    /// GZCOM class of the sprite instance (`+0x1C`).
    pub class: u32,
    /// Header bytes 12..18, stored at +0x24, +0x25, +0x26, +0x23, +0x22, +0x27. The first
    /// two are the zoom and rotation counts for every flora record (5, 4).
    pub dims: [u8; 6],
    /// `+0x28`.
    pub flags: u32,
    pub entries: Vec<Entry>,
}

impl SpriteAttrib {
    /// `BIN\r`, `u32 1`, `u32 class`, six bytes, `u32 flags`, `u16 count`, then `count`
    /// entries of `u16 index, u32 group, u32 instance, i8 scale, i16 dx, i16 dy`.
    pub fn parse(data: &[u8]) -> Result<SpriteAttrib, Error> {
        if data.get(..4) != Some(MAGIC) {
            return Err(Error::BadMagic);
        }
        if data.len() < HEADER_LEN {
            return Err(Error::Truncated);
        }
        let u16_at = |i: usize| u16::from_le_bytes([data[i], data[i + 1]]);
        let u32_at = |i: usize| u32::from_le_bytes(data[i..i + 4].try_into().unwrap());
        let count = u16_at(22) as usize;
        if data.len() < HEADER_LEN + count * ENTRY_LEN {
            return Err(Error::Truncated);
        }
        let entries = (0..count)
            .map(|k| {
                let e = HEADER_LEN + k * ENTRY_LEN;
                Entry {
                    group: u32_at(e + 2),
                    instance: u32_at(e + 6),
                    scale: data[e + 10] as i8,
                    dx: u16_at(e + 11) as i16,
                    dy: u16_at(e + 13) as i16,
                }
            })
            .collect();
        Ok(SpriteAttrib { class: u32_at(8), dims: data[12..18].try_into().unwrap(), flags: u32_at(18), entries })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_entries() {
        let mut d = b"BIN\r".to_vec();
        d.extend_from_slice(&1u32.to_le_bytes());
        d.extend_from_slice(&0x672u32.to_le_bytes());
        d.extend_from_slice(&[5, 4, 4, 0x14, 1, 0]);
        d.extend_from_slice(&0x1000_0040u32.to_le_bytes());
        d.extend_from_slice(&2u16.to_le_bytes());
        for k in 0..2u16 {
            d.extend_from_slice(&k.to_le_bytes());
            d.extend_from_slice(&9u32.to_le_bytes());
            d.extend_from_slice(&(0x0006_0000u32 + k as u32).to_le_bytes());
            d.push(if k == 1 { 2 } else { 0 });
            d.extend_from_slice(&(-3i16).to_le_bytes());
            d.extend_from_slice(&4i16.to_le_bytes());
        }
        let a = SpriteAttrib::parse(&d).unwrap();
        assert_eq!(a.class, 0x672);
        assert_eq!(a.dims[..2], [5, 4]);
        assert_eq!(a.entries.len(), 2);
        assert_eq!(a.entries[1], Entry { group: 9, instance: 0x0006_0001, scale: 2, dx: -3, dy: 4 });
        assert!(SpriteAttrib::parse(&d[..d.len() - 1]).is_err());
    }
}
