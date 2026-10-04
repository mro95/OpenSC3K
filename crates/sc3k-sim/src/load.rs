//! Reading a saved city's layers: the terrain (`cSC3DirtBag`) and the trees
//! (`cSC3FloraLayer`). See `docs/formats/save.md`.
//!
//! The same records make up a `.sct` terrain and the ground of a `.sc3` save. Each layer
//! reads its record from the city's DB segment (`sc3k_formats::segment`) under a fixed key.

use crate::cellmap::CellMap;
use crate::dirt::Terrain;
use sc3k_formats::ixf::Tgi;
use sc3k_formats::serial::{self, Reader, Version};
use std::fmt;

/// `cSC3DirtBag`'s record.
pub const KEY_DIRT_BAG: Tgi = Tgi { type_id: 0x206C_6E7C, group_id: 0x2173_7DE5, instance_id: 0 };
/// `cSC3FloraLayer`'s record.
pub const KEY_FLORA_LAYER: Tgi = Tgi { type_id: 0x406B_1196, group_id: 0x80AB_8AB0, instance_id: 0 };

/// The largest city the reader accepts, in cells per side.
const MAX_SIZE: u32 = 1024;

#[derive(Debug)]
pub enum Error {
    Serial(serial::Error),
    Segment(sc3k_formats::segment::Error),
    Missing(Tgi),
    /// The record's length fits no square city.
    UnknownSize { len: usize },
    BadMarker { want: &'static str },
    SizeMismatch { layer: &'static str, size: (u32, u32), city: u32 },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Serial(e) => e.fmt(f),
            Error::Segment(e) => e.fmt(f),
            Error::Missing(key) => write!(f, "the city has no record {key}"),
            Error::UnknownSize { len } => write!(f, "dirt bag record of {len} bytes fits no city size"),
            Error::BadMarker { want } => write!(f, "expected the marker {want:?}"),
            Error::SizeMismatch { layer, size, city } => {
                write!(f, "{layer} is {}x{} cells, the city {city}x{city}", size.0, size.1)
            }
        }
    }
}

impl std::error::Error for Error {}

impl From<serial::Error> for Error {
    fn from(e: serial::Error) -> Error {
        Error::Serial(e)
    }
}

/// The ground of a saved city or terrain: what the city view draws before any building.
pub struct Ground {
    pub terrain: Terrain,
    /// Flora type and density per cell (`read_flora_layer`); all 0 without a flora layer.
    pub flora_layer: CellMap<u8>,
}

/// Read the terrain and the flora layer from a `.sct`/`.sc3` container.
pub fn read_ground(archive: &sc3k_formats::ixf::Archive) -> Result<Ground, Error> {
    let seg = sc3k_formats::segment::Segment::open(archive).map_err(Error::Segment)?;
    let terrain = read_dirt_bag(seg.get(KEY_DIRT_BAG).ok_or(Error::Missing(KEY_DIRT_BAG))?)?;
    let flora_layer = match seg.get(KEY_FLORA_LAYER) {
        Some(rec) => read_flora_layer(rec, terrain.size)?,
        None => CellMap::new(terrain.size, terrain.size, 0),
    };
    Ok(Ground { terrain, flora_layer })
}

/// `cSC3DirtBag::Init(cISC3City*, cIGZDBSegment*)`
/// (libSimDirt Ghidra 0x33564, SIMDIRT.DLL 0x10004A00), checked against the original by
/// `tools/diffcheck/run.py ground`:
///
/// ```text
/// header         version 3 (serial::Version)
/// string         "DirtBag Start"            only with markers
/// f32            altitude scale (3.266), into the static mkAltScale
/// u8[X+1][Z+1]   vertex altitude, one column per x, from x = X down to 0
/// u8             sea level
/// u8[X+1][Z+1]   water level per vertex, the same way
/// u32[X·Z/32]    blocked cells, bit (x + z·X) (`BlockCell`, Ghidra 0x4204C)
/// string         "DirtBag End"              only with markers
/// ```
///
/// X and Z are the city's cell counts, which the record does not hold; the reader finds
/// the square size that fits the record's length. Salt water is not saved: the original
/// recomputes it (`RecalcSaltWater`), which is not ported, so every vertex reads fresh. The
/// generator's flora map is not saved either; the trees are in the flora layer.
pub fn read_dirt_bag(record: &[u8]) -> Result<Terrain, Error> {
    let mut r = Reader::new(record);
    let v = Version::read(&mut r);
    let mut fixed = 4 + 1;
    if v.markers {
        let start = r.string()?;
        if start != b"DirtBag Start" {
            return Err(Error::BadMarker { want: "DirtBag Start" });
        }
        fixed += 4 + "DirtBag End".len();
    }
    let body = record.len() - r.pos();
    let size = (1..=MAX_SIZE)
        .find(|&n| {
            let vertices = (n as usize + 1).pow(2);
            fixed + 2 * vertices + 4 * ((n * n) >> 5) as usize == body
        })
        .ok_or(Error::UnknownSize { len: record.len() })?;
    let _alt_scale = r.f32()?;
    let altitude = read_columns(&mut r, size + 1, size + 1)?;
    let sea_level = r.u8()?;
    let water = read_columns(&mut r, size + 1, size + 1)?;
    let _blocked = r.bytes(4 * ((size * size) >> 5) as usize)?;
    if v.markers && r.string()? != b"DirtBag End" {
        return Err(Error::BadMarker { want: "DirtBag End" });
    }
    let v = size + 1;
    Ok(Terrain {
        size,
        sea_level,
        altitude,
        water,
        salt: CellMap::new(v, v, false),
        flora: CellMap::new(v, v, 0),
    })
}

/// `cSC3FloraLayer::Init(cISC3City*, cIGZDBSegment*)` (libSimGeom Ghidra 0x5BA84): one
/// byte per cell, the flora type in the high nibble and the density in the low one, 0 for
/// none. Versions up to 2 hold no trees.
///
/// ```text
/// header         (serial::Version)
/// string         "Flora Layer Start"        only with markers
/// u32 X, u32 Z   must equal the city's size
/// u8[X][Z]       one column per x, from x = X − 1 down to 0
/// string         "Flora Layer End"          only with markers
/// ```
pub fn read_flora_layer(record: &[u8], size: u32) -> Result<CellMap<u8>, Error> {
    let mut r = Reader::new(record);
    let v = Version::read(&mut r);
    if v.markers {
        r.string()?;
    }
    if v.version <= 2 {
        return Ok(CellMap::new(size, size, 0));
    }
    let (x, z) = (r.u32()?, r.u32()?);
    if (x, z) != (size, size) {
        return Err(Error::SizeMismatch { layer: "flora layer", size: (x, z), city: size });
    }
    Ok(read_columns(&mut r, x, z)?)
}

/// A `cRZCellMap<u8>` as the layers write it: column x = width − 1 first.
fn read_columns(r: &mut Reader, width: u32, height: u32) -> Result<CellMap<u8>, serial::Error> {
    let mut map = CellMap::new(width, height, 0);
    for x in (0..width).rev() {
        for (y, &b) in r.bytes(height as usize)?.iter().enumerate() {
            map.set(x, y as u32, b);
        }
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirt_record(size: u32, markers: bool) -> Vec<u8> {
        let v = size + 1;
        let mut d = vec![3, 0, 2 | markers as u8, 0, 0xEF, 0xBE, 0xAD, 0xDE];
        let string = |d: &mut Vec<u8>, s: &[u8]| {
            d.extend((s.len() as u32).to_le_bytes());
            d.extend(s);
        };
        if markers {
            string(&mut d, b"DirtBag Start");
        }
        d.extend(3.266f32.to_le_bytes());
        for x in (0..v).rev() {
            d.extend((0..v).map(|y| (x * 2 + y) as u8));
        }
        d.push(9);
        for _ in 0..v {
            d.extend(std::iter::repeat_n(9, v as usize));
        }
        d.extend(vec![0; 4 * ((size * size) >> 5) as usize]);
        if markers {
            string(&mut d, b"DirtBag End");
        }
        d
    }

    #[test]
    fn reads_dirt_bag() {
        for markers in [false, true] {
            let t = read_dirt_bag(&dirt_record(64, markers)).unwrap();
            assert_eq!((t.size, t.sea_level), (64, 9));
            assert_eq!(t.altitude.get(3, 5), 11);
            assert_eq!(t.water.get(64, 64), 9);
        }
    }

    #[test]
    fn reads_flora_layer() {
        let mut d = vec![3, 0, 0, 0xEF, 0xBE, 0xAD, 0xDE];
        d.extend(2u32.to_le_bytes());
        d.extend(2u32.to_le_bytes());
        d.extend([0x00, 0x00, 0x37, 0x00]);
        let m = read_flora_layer(&d, 2).unwrap();
        assert_eq!(m.get(0, 0), 0x37);
        assert!(read_flora_layer(&d, 4).is_err());
    }

    #[test]
    fn reads_terrain_files() {
        let Some(root) = sc3k_formats::data_dir() else {
            eprintln!("SC3K_DATA not set; skipping");
            return;
        };
        let dir = root.join("Cities/Terrains");
        let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
        files.push(root.join("Cities/Madison, WI.sc3"));
        for path in files {
            let archive = sc3k_formats::ixf::Archive::open(&path).unwrap();
            let g = read_ground(&archive).unwrap();
            let size = g.terrain.size;
            assert!((64..=256).contains(&size) && size.is_multiple_of(64), "{}: {size}", path.display());
        }
    }
}
