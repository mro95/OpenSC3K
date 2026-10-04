//! Occupant records (`TKB1`, type [`TYPE_OCCUPANT`]): a flat list of typed properties.
//! See `docs/formats/occupant.md`.
//!
//! The flora occupants (group [`GROUP_OCCUPANT`]) live in `Res/Occupant/OccupantAttribs.IXF`,
//! and each flora set's archive overrides them. Property [`PROP_SPRITE_ATTRIB`] is the key of
//! the occupant's sprite attributes (`csattrib.rs`).

use crate::ixf::Tgi;
use std::fmt;

pub const TYPE_OCCUPANT: u32 = 0xFFD3_0C03;
pub const GROUP_OCCUPANT: u32 = 0x80F4_8961;
const MAGIC: &[u8; 4] = b"TKB1";

/// The name, a string.
pub const PROP_NAME: u32 = 0x64;
/// The resource key of the sprite attributes.
pub const PROP_SPRITE_ATTRIB: u32 = 0x67;

#[derive(Debug)]
pub enum Error {
    BadMagic,
    Truncated,
    UnknownType { id: u32, kind: u16 },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadMagic => write!(f, "not a TKB1 record"),
            Error::Truncated => write!(f, "TKB1 record truncated"),
            Error::UnknownType { id, kind } => write!(f, "TKB1 property {id:#X} has unknown type {kind}"),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// Type 1.
    U8(Vec<u8>),
    /// Type 3.
    U32(Vec<u32>),
    /// Type 7: each string is a `u32` length and the bytes.
    Str(Vec<Vec<u8>>),
    /// Type 8: resource keys, stored type, group, instance.
    Key(Vec<Tgi>),
}

#[derive(Clone, Debug)]
pub struct Occupant {
    /// (property id, value), in file order.
    pub props: Vec<(u32, Value)>,
}

impl Occupant {
    /// Each property is `u32 id, u16 type, u16 count`, then `count` values of the type.
    pub fn parse(data: &[u8]) -> Result<Occupant, Error> {
        if data.get(..4) != Some(MAGIC) {
            return Err(Error::BadMagic);
        }
        let mut p = 4;
        let take = |p: &mut usize, n: usize| -> Result<&[u8], Error> {
            let s = data.get(*p..*p + n).ok_or(Error::Truncated)?;
            *p += n;
            Ok(s)
        };
        let u32_at = |s: &[u8]| u32::from_le_bytes(s.try_into().unwrap());
        let mut props = Vec::new();
        while p < data.len() {
            let head = take(&mut p, 8)?;
            let id = u32_at(&head[..4]);
            let kind = u16::from_le_bytes([head[4], head[5]]);
            let count = u16::from_le_bytes([head[6], head[7]]) as usize;
            let value = match kind {
                1 => Value::U8(take(&mut p, count)?.to_vec()),
                3 => Value::U32(take(&mut p, count * 4)?.chunks(4).map(u32_at).collect()),
                7 => {
                    let mut v = Vec::with_capacity(count);
                    for _ in 0..count {
                        let len = u32_at(take(&mut p, 4)?) as usize;
                        v.push(take(&mut p, len)?.to_vec());
                    }
                    Value::Str(v)
                }
                8 => Value::Key(
                    take(&mut p, count * 12)?
                        .chunks(12)
                        .map(|k| Tgi { type_id: u32_at(&k[..4]), group_id: u32_at(&k[4..8]), instance_id: u32_at(&k[8..]) })
                        .collect(),
                ),
                _ => return Err(Error::UnknownType { id, kind }),
            };
            props.push((id, value));
        }
        Ok(Occupant { props })
    }

    pub fn get(&self, id: u32) -> Option<&Value> {
        self.props.iter().find(|(i, _)| *i == id).map(|(_, v)| v)
    }

    /// The first key of property `id`.
    pub fn key(&self, id: u32) -> Option<Tgi> {
        match self.get(id)? {
            Value::Key(k) => k.first().copied(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_properties() {
        let mut d = b"TKB1".to_vec();
        let prop = |d: &mut Vec<u8>, id: u32, kind: u16, count: u16| {
            d.extend_from_slice(&id.to_le_bytes());
            d.extend_from_slice(&kind.to_le_bytes());
            d.extend_from_slice(&count.to_le_bytes());
        };
        prop(&mut d, PROP_NAME, 7, 1);
        d.extend_from_slice(&3u32.to_le_bytes());
        d.extend_from_slice(b"Elm");
        prop(&mut d, 0x65, 3, 1);
        d.extend_from_slice(&300u32.to_le_bytes());
        prop(&mut d, PROP_SPRITE_ATTRIB, 8, 1);
        for v in [0x6300u32, 0x6400, 6] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        prop(&mut d, 0x6E, 1, 1);
        d.push(0x8C);
        let o = Occupant::parse(&d).unwrap();
        assert_eq!(o.get(PROP_NAME), Some(&Value::Str(vec![b"Elm".to_vec()])));
        assert_eq!(o.key(PROP_SPRITE_ATTRIB), Some(Tgi { type_id: 0x6300, group_id: 0x6400, instance_id: 6 }));
        assert_eq!(o.get(0x6E), Some(&Value::U8(vec![0x8C])));
        assert!(Occupant::parse(&d[..d.len() - 1]).is_err());
    }
}
