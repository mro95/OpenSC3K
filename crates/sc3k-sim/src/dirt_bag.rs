//! `cSC3DirtBag`'s queries on a loaded terrain (libSimDirt, SIMDIRT.DLL). The dirt bag's
//! vtable has the Loki slots in the Loki order on Windows too, so each query cites both.
//! `tools/diffcheck/run.py ground` asks every one of them on every saved terrain.
//!
//! The original answers 0 for the counts and 0.0 for the scale until `Init` has run; a
//! `Terrain` is always loaded.

use crate::dirt::Terrain;

/// `MinAltitudeAllowed` (libSimDirt 0x41230, SIMDIRT.DLL 0x10006EA0), also what `AltitudeMin`
/// (0x411F4, SIMDIRT.DLL 0x10006E80) returns: not the lowest vertex, the lowest allowed.
pub const MIN_ALTITUDE_ALLOWED: u8 = 0;

/// `MaxAltitudeAllowed` (libSimDirt 0x41224, SIMDIRT.DLL 0x10006E90), also what `AltitudeMax`
/// (0x411C0, SIMDIRT.DLL 0x10006E70) returns.
pub const MAX_ALTITUDE_ALLOWED: u8 = 0xFF;

/// `MaxAltitudeDeltaAllowed` (libSimDirt 0x4123C, SIMDIRT.DLL 0x10006EB0), also what
/// `MaxAltitudeDelta` (0x40870, SIMDIRT.DLL 0x1000F6E0) returns.
pub const MAX_ALTITUDE_DELTA_ALLOWED: u8 = 4;

/// `cSC3DirtClodFactory::GetMaxLightIndex` (SIMDIRT.DLL 0x1001684C), the highest light level.
pub const MAX_LIGHT_INDEX: u8 = 0x1F;

/// `GetPathGranularity` (libSimDirt 0x4203C, SIMDIRT.DLL 0x1000E460).
pub const PATH_GRANULARITY: u32 = 1;

/// `CellCountX` (libSimDirt 0x410CC, SIMDIRT.DLL 0x10006DF0).
pub fn cell_count_x(t: &Terrain) -> u32 {
    t.altitude.width() - 1
}

/// `CellCountZ` (libSimDirt 0x41104, SIMDIRT.DLL 0x10006E10).
pub fn cell_count_z(t: &Terrain) -> u32 {
    t.altitude.height() - 1
}

/// `VertexCountX` (libSimDirt 0x41140, SIMDIRT.DLL 0x10006E30).
pub fn vertex_count_x(t: &Terrain) -> u32 {
    t.altitude.width()
}

/// `VertexCountZ` (libSimDirt 0x41180, SIMDIRT.DLL 0x10006E50).
pub fn vertex_count_z(t: &Terrain) -> u32 {
    t.altitude.height()
}

/// `GetGlobalSeaLevel` (libSimDirt 0x40FAC, SIMDIRT.DLL 0x10006950).
pub fn global_sea_level(t: &Terrain) -> u8 {
    t.sea_level
}

/// `InCellBounds(x, z)` (libSimDirt 0x4127C, SIMDIRT.DLL 0x10006EE0).
pub fn in_cell_bounds(t: &Terrain, x: u32, z: u32) -> bool {
    x < cell_count_x(t) && z < cell_count_z(t)
}

/// `InCellBounds(x1, z1, x2, z2)` (libSimDirt 0x412D8, SIMDIRT.DLL 0x10006F10): an ordered
/// rectangle whose far corner is a cell. The near corner is not tested on its own.
pub fn in_cell_rect_bounds(t: &Terrain, x1: u32, z1: u32, x2: u32, z2: u32) -> bool {
    x1 <= x2 && z1 <= z2 && in_cell_bounds(t, x2, z2)
}

/// `InVertexBounds(x, z)` (libSimDirt 0x41340, SIMDIRT.DLL 0x10006F60).
pub fn in_vertex_bounds(t: &Terrain, x: u32, z: u32) -> bool {
    x < vertex_count_x(t) && z < vertex_count_z(t)
}

/// `InVertexBounds(x1, z1, x2, z2)` (libSimDirt 0x41374, SIMDIRT.DLL 0x10006F90): an ordered
/// rectangle with both corners vertices.
pub fn in_vertex_rect_bounds(t: &Terrain, x1: u32, z1: u32, x2: u32, z2: u32) -> bool {
    x1 <= x2 && z1 <= z2 && in_vertex_bounds(t, x1, z1) && in_vertex_bounds(t, x2, z2)
}

/// `IsValidVertexAltitude` (libSimDirt 0x413E0, SIMDIRT.DLL 0x10006FE0): every byte is.
pub fn is_valid_vertex_altitude(_altitude: u8) -> bool {
    true
}

/// `IsValidVertexLight` (libSimDirt 0x413F0, SIMDIRT.DLL 0x10006FF0).
pub fn is_valid_vertex_light(light: u8) -> bool {
    light <= MAX_LIGHT_INDEX
}

/// `GetVertexAltitudeDirt` (libSimDirt 0x409F4, SIMDIRT.DLL 0x10005AB0).
pub fn vertex_altitude_dirt(t: &Terrain, x: u32, z: u32) -> u8 {
    t.altitude.get(x, z)
}

/// `GetVertexAltitudeWater` (libSimDirt 0x40A44, SIMDIRT.DLL 0x10005AD0).
pub fn vertex_altitude_water(t: &Terrain, x: u32, z: u32) -> u8 {
    t.water.get(x, z)
}

/// `GetVertexLight` (libSimDirt 0x40A94, SIMDIRT.DLL 0x10005AF0). Both builds read the
/// altitude map here, not the light map: the vertex's dirt altitude comes back.
pub fn vertex_light(t: &Terrain, x: u32, z: u32) -> u8 {
    t.altitude.get(x, z)
}

/// `GetVertexAltitude` (libSimDirt 0x4099C, SIMDIRT.DLL 0x10005A70): the higher of dirt and
/// water.
pub fn vertex_altitude(t: &Terrain, x: u32, z: u32) -> u8 {
    t.altitude.get(x, z).max(t.water.get(x, z))
}

fn corners(x: u32, z: u32) -> [(u32, u32); 4] {
    [(x, z), (x + 1, z), (x + 1, z + 1), (x, z + 1)]
}

/// `IsWater` (libSimDirt 0x359A0, SIMDIRT.DLL 0x100065F0): some corner's water reaches its
/// dirt.
pub fn is_water(t: &Terrain, x: u32, z: u32) -> bool {
    corners(x, z).iter().any(|&(cx, cz)| t.water.get(cx, cz) >= t.altitude.get(cx, cz))
}

/// `IsRealWater` (libSimDirt 0x35B10, SIMDIRT.DLL 0x10006710): some corner's water is above
/// its dirt.
pub fn is_real_water(t: &Terrain, x: u32, z: u32) -> bool {
    corners(x, z).iter().any(|&(cx, cz)| t.water.get(cx, cz) > t.altitude.get(cx, cz))
}

/// `CellWaterVertCount` (libSimDirt 0x35C80, SIMDIRT.DLL 0x10006830): the corners on the map
/// whose water reaches their dirt.
pub fn cell_water_vert_count(t: &Terrain, x: u32, z: u32) -> u8 {
    corners(x, z)
        .iter()
        .filter(|&&(cx, cz)| in_vertex_bounds(t, cx, cz) && t.water.get(cx, cz) >= t.altitude.get(cx, cz))
        .count() as u8
}

/// `GetAverageAltitude` (libSimDirt 0x3EE80, SIMDIRT.DLL 0x1000EAB0): the mean height above
/// the sea of the dry vertices (water below the dirt), truncated; 0 when none is dry. A dry
/// vertex under the sea counts negative, and the low byte of the mean is returned.
pub fn average_altitude(t: &Terrain) -> u8 {
    let (mut sum, mut n) = (0i32, 0i32);
    for x in 0..vertex_count_x(t) {
        for z in 0..vertex_count_z(t) {
            let (dirt, water) = (t.altitude.get(x, z), t.water.get(x, z));
            if water < dirt {
                sum += dirt as i32 - t.sea_level as i32;
                n += 1;
            }
        }
    }
    if n == 0 {
        0
    } else {
        (sum / n) as u8
    }
}
