//! Reading a saved city's layers: the terrain (`cSC3DirtBag`) and the trees
//! (`cSC3FloraLayer`). See `docs/formats/save.md`.
//!
//! The same records make up a `.sct` terrain and the ground of a `.sc3` save. Each layer
//! reads its record from the city's DB segment (`sc3k_formats::segment`) under a fixed key.

use crate::cellmap::CellMap;
use crate::dirt::Terrain;
use crate::transit::{NetworkTile, Networks};
use sc3k_formats::ixf::Tgi;
use sc3k_formats::serial::{self, Reader, Version};
use std::fmt;

/// `cSC3DirtBag`'s record.
pub const KEY_DIRT_BAG: Tgi = Tgi { type_id: 0x206C_6E7C, group_id: 0x2173_7DE5, instance_id: 0 };
/// `cSC3FloraLayer`'s record.
pub const KEY_FLORA_LAYER: Tgi = Tgi { type_id: 0x406B_1196, group_id: 0x80AB_8AB0, instance_id: 0 };

/// `cSTTransitLayer`'s header record; instances 1 to 3 hold the occupants of each manager.
pub const KEY_NETWORK_LAYER: Tgi = Tgi { type_id: 0x206C_6E7C, group_id: 0x2147_C2DD, instance_id: 0 };

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
    /// The network header's persist version is not 1.
    PersistVersion { found: u32 },
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
            Error::PersistVersion { found } => write!(f, "network persist version {found}, expected 1"),
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
    /// Network tiles per manager (`read_network_layer`); empty without a network layer.
    pub networks: Networks,
}

/// Read the terrain, the flora layer and the networks from a `.sct`/`.sc3` container.
pub fn read_ground(archive: &sc3k_formats::ixf::Archive) -> Result<Ground, Error> {
    let seg = sc3k_formats::segment::Segment::open(archive).map_err(Error::Segment)?;
    let terrain = read_dirt_bag(seg.get(KEY_DIRT_BAG).ok_or(Error::Missing(KEY_DIRT_BAG))?)?;
    let flora_layer = match seg.get(KEY_FLORA_LAYER) {
        Some(rec) => read_flora_layer(rec, terrain.size)?,
        None => CellMap::new(terrain.size, terrain.size, 0),
    };
    let networks = match seg.get(KEY_NETWORK_LAYER) {
        Some(_) => read_network_layer(|key| seg.get(key), terrain.size)?,
        None => Networks::new(terrain.size),
    };
    Ok(Ground { terrain, flora_layer, networks })
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
/// u32[X·Z/32]    blocked cells, bit (x + z·X) (`BlockCell`, Ghidra 0x4204C,
///                SIMDIRT.DLL 0x1000E470)
/// string         "DirtBag End"              only with markers
/// ```
///
/// X and Z are the city's cell counts, which the record does not hold; the reader finds
/// the square size that fits the record's length. Salt water is not saved: the original
/// recomputes it (`RecalcSaltWater`), which is not ported, so every vertex reads fresh. The
/// generator's flora map is not saved either; the trees are in the flora layer.
/// Unchecked: the reader skips the blocked cells, and no check calls `BlockCell`.
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
/// Unchecked: no Windows address known yet.
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

/// `cSTTransitLayer::Init(cISC3City*, cIGZDBSegment*)` (libSimNtwrk Ghidra 0x68980).
///
/// `get` looks up a record of the city's segment, as `Segment::get` does.
/// Unchecked: no check reads the network layer record yet.
pub fn read_network_layer<'a>(get: impl Fn(Tgi) -> Option<&'a [u8]>, size: u32) -> Result<Networks, Error> {
    let key = KEY_NETWORK_LAYER;
    let header = get(key).ok_or(Error::Missing(key))?;
    let mut r = Reader::new(header);
    let version = Version::read(&mut r);
    if version.version != 4 {
        return Ok(Networks::new(size));
    }
    let counts = [r.u32()?, r.u32()?, r.u32()?];
    let persist_version = r.u32()?;
    if persist_version != 1 {
        return Err(Error::PersistVersion { found: persist_version });
    }

    let mut networks = Networks::new(size);
    for i in 1..4usize {
        if counts[i - 1] == 0 {
            continue;
        }
        let key = Tgi { instance_id: i as u32, ..KEY_NETWORK_LAYER };
        let record = get(key).ok_or(Error::Missing(key))?;
        let mut r = Reader::new(record);
        Version::read(&mut r);
        let map = match i {
            1 => &mut networks.surface,
            2 => &mut networks.plumbing,
            3 => &mut networks.subway,
            _ => unreachable!(),
        };
        for _ in 0..counts[i - 1] {
            let w0 = r.u32()?;
            let w1 = r.u32()?;
            let x = w0 & 0x7FF;
            let y = w0 >> 11 & 0x7FF;
            let altitude = (w0 >> 22 & 0xFF) as u8;
            let rotation = (w0 >> 30) as u8;
            let tile_id = (w1 & 0xFFFF) as u16;
            if x >= size || y >= size {
                continue;
            }
            map.set(x, y, Some(NetworkTile { tile_id, rotation, altitude }));
        }
    }

    Ok(networks)
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

    /// A network record: the version header, then `body`.
    fn network_record(body: &[u32]) -> Vec<u8> {
        let mut d = vec![4, 0, 2, 0, 0xEF, 0xBE, 0xAD, 0xDE];
        d.extend(body.iter().flat_map(|w| w.to_le_bytes()));
        d
    }

    /// A tile's two words, packed as `cSC3TransitBlockPersistInfo` stores them.
    fn network_tile(x: u32, y: u32, altitude: u32, rotation: u32, tile: u32) -> [u32; 2] {
        [x | y << 11 | altitude << 22 | rotation << 30, 0x3000_0000 | tile]
    }

    fn network_key(instance_id: u32) -> Tgi {
        Tgi { instance_id, ..KEY_NETWORK_LAYER }
    }

    #[test]
    fn reads_network_layer() {
        let header = network_record(&[2, 0, 1, 1]);
        let surface =
            network_record(&[network_tile(3, 5, 31, 2, 29), network_tile(64, 0, 31, 0, 44)].concat());
        let subway = network_record(&network_tile(63, 63, 200, 3, 0x2D5D));
        let records = [(network_key(0), header), (network_key(1), surface), (network_key(3), subway)];
        let get = |key| records.iter().find(|(k, _)| *k == key).map(|(_, d)| d.as_slice());
        let n = read_network_layer(get, 64).unwrap();
        assert_eq!(n.surface.get(3, 5), Some(NetworkTile { tile_id: 29, rotation: 2, altitude: 31 }));
        assert_eq!(n.subway.get(63, 63), Some(NetworkTile { tile_id: 0x2D5D, rotation: 3, altitude: 200 }));
        // The tile at x = 64 lies off the map and is skipped; record 2 is not read.
        let tiles = |m: &CellMap<Option<NetworkTile>>| {
            (0..64).flat_map(|x| (0..64).map(move |y| (x, y))).filter(|&(x, y)| m.get(x, y).is_some()).count()
        };
        assert_eq!((tiles(&n.surface), tiles(&n.plumbing), tiles(&n.subway)), (1, 0, 1));
    }

    #[test]
    fn network_layer_errors() {
        let read = |header: &[u32], surface: Option<Vec<u8>>| {
            let header = network_record(header);
            read_network_layer(
                |key| match key.instance_id {
                    0 => Some(header.as_slice()),
                    1 => surface.as_deref(),
                    _ => None,
                },
                64,
            )
        };
        assert!(matches!(read(&[1, 0, 0, 2], None), Err(Error::PersistVersion { found: 2 })));
        assert!(matches!(read(&[1, 0, 0, 1], None), Err(Error::Missing(key)) if key == network_key(1)));
        assert!(matches!(
            read(&[2, 0, 0, 1], Some(network_record(&network_tile(0, 0, 0, 0, 29)))),
            Err(Error::Serial(_))
        ));
        assert!(
            matches!(read_network_layer(|_| None, 64), Err(Error::Missing(key)) if key == KEY_NETWORK_LAYER)
        );
        // Another version holds nothing the reader knows: no networks, and no error.
        let old = [3, 0, 0, 0, 0xEF, 0xBE, 0xAD, 0xDE, 1, 0, 0, 0];
        let n = read_network_layer(|key| (key == KEY_NETWORK_LAYER).then_some(&old[..]), 64).unwrap();
        assert_eq!(n.surface.get(0, 0), None);
    }

    #[test]
    fn reads_network_files() {
        let Some(root) = sc3k_formats::data_dir() else {
            eprintln!("SC3K_DATA not set; skipping");
            return;
        };
        let cities = root.join("Cities");
        let mut files: Vec<_> = std::fs::read_dir(&cities).unwrap().map(|e| e.unwrap().path()).collect();
        files.retain(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("sc3")));
        assert!(files.len() >= 14, "{} saves in {}", files.len(), cities.display());
        for path in files {
            let archive = sc3k_formats::ixf::Archive::open(&path).unwrap();
            let seg = sc3k_formats::segment::Segment::open(&archive).unwrap();
            let size = read_ground(&archive).unwrap().terrain.size;
            let n = read_network_layer(|key| seg.get(key), size).unwrap();
            // No save has two tiles on one cell, so every counted tile lands on its own cell.
            let mut header = Reader::new(seg.get(KEY_NETWORK_LAYER).unwrap());
            Version::read(&mut header);
            let counts = [header.u32().unwrap(), header.u32().unwrap(), header.u32().unwrap()];
            let tiles = |m: &CellMap<Option<NetworkTile>>| {
                (0..size)
                    .flat_map(|x| (0..size).map(move |y| (x, y)))
                    .filter(|&(x, y)| m.get(x, y).is_some())
                    .count() as u32
            };
            assert_eq!(
                [tiles(&n.surface), tiles(&n.plumbing), tiles(&n.subway)],
                counts,
                "{}",
                path.display()
            );
            if path.ends_with("Madison, WI.sc3") {
                assert_eq!(counts, [7026, 3516, 0]);
                assert_eq!(
                    n.surface.get(0, 87),
                    Some(NetworkTile { tile_id: 44, rotation: 0, altitude: 31 })
                );
            }
        }
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
