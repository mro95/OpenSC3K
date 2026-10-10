//! Trees on a new map (`cSC3FloraLayer`, libSimGeom). See `docs/sim/flora.md`.
//!
//! When the simulation begins, `cSC3DirtBag::SimulationBegin` (libSimDirt Ghidra 0x344F4,
//! SIMDIRT.DLL 0x10005230) walks the flora map the generator left in the dirt bag, cell x
//! outer, y inner. Each value of 0x20 or more becomes `SetFloraDensity(cell, value >> 5)` on
//! the flora layer, which picks a tree type from the height and places one flora occupant.
//! Unchecked: no check runs the flora layer yet.

use crate::cellmap::CellMap;
use crate::dirt::Terrain;
use crate::rng::Random;

/// Flora occupant IDs by type, density and variant (libSimGeom data 0x83F80, 100 `u16`,
/// indexed `type · 20 + density · 2 + variant`). Type 0 is unused. The IDs are instances of
/// occupant records (`sc3k_formats::occupant`).
pub const OCCUPANTS: [[u16; 20]; 5] = [
    [0; 20],
    // 1: wetland.
    [
        0x1B8E, 0x1B8E, 0x1B8E, 0x1B8F, 0x1B8E, 0x1B8F, 0x1B8E, 0x1B8F, 0x1B8E, 0x1B8F, 0x1B8E, 0x1B8F, 0x1B8E,
        0x1B8F, 0x1B8E, 0x1B8F, 0x1B91, 0x1B91, 0x1B91, 0x1B91,
    ],
    // 2: low ground, 1..4 above the sea.
    [
        0x1B80, 0x1B80, 0x1B81, 0x1B81, 0x1B82, 0x1B82, 0x1B83, 0x1B83, 0x1B84, 0x1B84, 0x1B85, 0x1B85, 0x1B86,
        0x1B8D, 0x1B86, 0x1B8D, 0x23F0, 0x23F0, 0x23F0, 0x23F0,
    ],
    // 3: the middle band.
    [
        0x0006, 0x0006, 0x0007, 0x0007, 0x0008, 0x0008, 0x0009, 0x0009, 0x000A, 0x000A, 0x000B, 0x000B, 0x000C,
        0x1B90, 0x000C, 0x1B90, 0x1B91, 0x1B91, 0x1B91, 0x1B91,
    ],
    // 4: high ground, 8 and more above the sea.
    [
        0x1B58, 0x1B58, 0x1B59, 0x1B59, 0x1B5A, 0x1B5A, 0x1B5B, 0x1B5B, 0x1B5C, 0x1B5C, 0x1B5D, 0x1B5D, 0x1B5E,
        0x1B8C, 0x1B5E, 0x1B8C, 0x23F1, 0x23F1, 0x23F1, 0x23F1,
    ],
];

/// One placed flora occupant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Flora {
    pub occupant: u16,
    /// Occupant z: `GetVertexAltitude` of the cell's (x, y) vertex, the higher of dirt and
    /// water.
    pub altitude: u8,
}

/// Place the trees of a new map. One entry per cell.
///
/// `cSC3FloraLayer::StaticInit` (libSimGeom 0x5B614) creates the layer's `cRZRandom` once
/// per run with seed `0xFFFFFFFF`, which means "from the clock"; a second city continues
/// the sequence. Here the caller passes the seed, and each city starts afresh.
/// Unchecked: no check runs the flora layer yet.
pub fn place(t: &Terrain, seed: u32) -> CellMap<Option<Flora>> {
    let mut rng = Random::new(seed);
    let mut out = CellMap::new(t.size, t.size, None);
    for x in 0..t.size {
        for y in 0..t.size {
            let v = t.flora.get(x, y);
            if v >= 0x20 {
                out.set(x, y, set_flora_density(t, &mut rng, x, y, v >> 5));
            }
        }
    }
    out
}

/// The trees of a loaded city, from its flora layer (`load::read_flora_layer`): each
/// non-zero cell becomes `CreateOccupant(value >> 4, value & 0xF)`, on water too
/// (`cSC3FloraLayer::Init(cISC3City*, cIGZDBSegment*)`, libSimGeom Ghidra 0x5BA84). The seed
/// stands in for the layer's clock-seeded `cRZRandom`, as in [`place`].
pub fn from_layer(t: &Terrain, layer: &CellMap<u8>, seed: u32) -> CellMap<Option<Flora>> {
    let mut rng = Random::new(seed);
    let mut out = CellMap::new(t.size, t.size, None);
    for x in 0..t.size {
        for y in 0..t.size {
            let v = layer.get(x, y);
            if v != 0 {
                out.set(x, y, create_occupant(t, &mut rng, x, y, v as usize >> 4, v as usize & 0xF));
            }
        }
    }
    out
}

/// `cSC3FloraLayer::SetFloraDensity` (libSimGeom 0x5C4B4) on an empty cell: nothing on
/// water, else `SelectFloraType` and `CreateOccupant(type, density)`.
/// Unchecked: no check runs the flora layer yet.
fn set_flora_density(t: &Terrain, rng: &mut Random, x: u32, y: u32, density: u8) -> Option<Flora> {
    if is_water(t, x, y) {
        return None;
    }
    let kind = select_flora_type(t, rng, x, y);
    create_occupant(t, rng, x, y, kind, density as usize)
}

/// `cSC3FloraLayer::CreateOccupant(type, density)` (0x5D5F4): the occupant of a random
/// variant, at the cell's vertex altitude. A type or density outside the table, which only
/// a damaged save holds, places nothing; the original would read past the table.
/// Unchecked: no check runs the flora layer yet.
fn create_occupant(t: &Terrain, rng: &mut Random, x: u32, y: u32, kind: usize, density: usize) -> Option<Flora> {
    let variant = (rng.uniform(2) != 0) as usize;
    let occupant = *OCCUPANTS.get(kind)?.get(density * 2 + variant)?;
    Some(Flora { occupant, altitude: vertex_altitude(t, x, y) })
}

/// `cSC3FloraLayer::SelectFloraType` (0x5C874), by the height h of the cell's (x, y) vertex
/// above the sea:
/// - wet cells (some corner level with the water, none under it): 1;
/// - h 1..=2: 2; h 3..=4: 2 or 3 at random;
/// - h 5..=7: 3; h 8..=126: 3 or 4 at random;
/// - anything else: 3.
/// Unchecked: no Windows address known yet.
fn select_flora_type(t: &Terrain, rng: &mut Random, x: u32, y: u32) -> usize {
    let h = vertex_altitude(t, x, y).wrapping_sub(t.sea_level);
    let wet = is_water(t, x, y) && !is_real_water(t, x, y);
    let mut low = h.wrapping_sub(1) < 4;
    let mut mid = h.wrapping_sub(3) < 0x7C;
    let mut high = h.wrapping_sub(8) < 0x77;
    if low && mid {
        if rng.uniform(2) != 0 {
            mid = false;
        } else {
            low = false;
        }
    }
    if !low && mid && high {
        if rng.uniform(2) == 0 {
            mid = false;
        } else {
            high = false;
        }
    }
    if wet {
        1
    } else if low {
        2
    } else if mid || !high {
        3
    } else {
        4
    }
}

/// `cSC3DirtBag::GetVertexAltitude` (libSimDirt 0x4099C, SIMDIRT.DLL 0x10005A70): the higher
/// of dirt and water.
pub fn vertex_altitude(t: &Terrain, x: u32, y: u32) -> u8 {
    t.altitude.get(x, y).max(t.water.get(x, y))
}

fn corners(x: u32, y: u32) -> [(u32, u32); 4] {
    [(x, y), (x + 1, y), (x + 1, y + 1), (x, y + 1)]
}

/// `cSC3DirtBag::IsWater` (libSimDirt 0x359A0, SIMDIRT.DLL 0x100065F0): some corner's water
/// reaches its dirt.
pub fn is_water(t: &Terrain, x: u32, y: u32) -> bool {
    corners(x, y).iter().any(|&(cx, cy)| t.water.get(cx, cy) >= t.altitude.get(cx, cy))
}

/// `cSC3DirtBag::IsRealWater` (libSimDirt 0x35B10, SIMDIRT.DLL 0x10006710): some corner's
/// water is above its dirt.
pub fn is_real_water(t: &Terrain, x: u32, y: u32) -> bool {
    corners(x, y).iter().any(|&(cx, cy)| t.water.get(cx, cy) > t.altitude.get(cx, cy))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terrain(alt: u8) -> Terrain {
        let v = 9;
        let mut flora = CellMap::new(v, v, 0);
        flora.set(2, 3, 0xE0);
        flora.set(4, 4, 0x1F);
        Terrain {
            size: 8,
            sea_level: 10,
            altitude: CellMap::new(v, v, alt),
            water: CellMap::new(v, v, 10),
            salt: CellMap::new(v, v, false),
            flora,
        }
    }

    fn place_test(t: &Terrain) -> CellMap<Option<Flora>> {
        place(t, 1)
    }

    #[test]
    fn places_by_height() {
        // h = 6: always type 3; density 7.
        let f = place_test(&terrain(16));
        let tree = f.get(2, 3).unwrap();
        assert!(tree.occupant == OCCUPANTS[3][14] || tree.occupant == OCCUPANTS[3][15]);
        assert_eq!(tree.altitude, 16);
        // Below 0x20 nothing grows.
        assert_eq!(f.get(4, 4), None);
        // h = 1: type 2.
        let tree = place_test(&terrain(11)).get(2, 3).unwrap();
        assert!(OCCUPANTS[2][14..16].contains(&tree.occupant));
        // h = 30: type 3 or 4.
        let tree = place_test(&terrain(40)).get(2, 3).unwrap();
        assert!(OCCUPANTS[3][14..16].contains(&tree.occupant) || OCCUPANTS[4][14..16].contains(&tree.occupant));
        // Under water: nothing.
        assert_eq!(place_test(&terrain(10)).get(2, 3), None);
    }
}
