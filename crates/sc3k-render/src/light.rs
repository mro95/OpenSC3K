//! Per-vertex light, as `cSC3DirtBag::calculateAndSetVertexLight` stores it in the dirt bag
//! (libSimDirt 0x4142C, SIMDIRT.DLL 0x10007010). See `docs/render/terrain.md`, "Vertex
//! light". Checked against the original by `tools/diffcheck/run.py ground`.
//!
//! The arithmetic follows the Windows build step by step: x87 at 53-bit precision, so every
//! expression is evaluated in `f64` and rounded to `f32` where the original stores a float.
//! That rounding decides the index of vertices at a step boundary.

use sc3k_sim::cellmap::CellMap;
use sc3k_sim::dirt::Terrain;

/// Horizontal size of a cell in world units (`cSC3City::CellSizeInWorldUnitsX/Z`).
const CELL_SIZE: f64 = 16.0;
/// World units per altitude step (`cSC3City::CellSizeInWorldUnitsY`, `AltitudeScale`).
const ALTITUDE_SCALE: f32 = 3.266;
/// Highest light index (`cSC3DirtClodColorLightTable::mkLightIndexMax`).
pub const MAX_LIGHT: u8 = 31;
/// The two light directions (libSimDirt rodata 0x61698, 0x616A4), as (x, up, y).
const LIGHTS: [[f32; 3]; 2] = [[1.0, -1.0, 0.0], [-1.0, -1.0, 0.0]];

/// π as the original keeps it, a float (SIMDIRT.DLL data 0x10020F38).
const PI: f64 = std::f32::consts::PI as f64;

type Vec3 = [f32; 3];

/// `vecDotProduct` (SIMDIRT.DLL 0x1000FD10): z, then y, then x.
fn dot(a: Vec3, b: Vec3) -> f32 {
    let [a, b] = [a, b].map(|v| v.map(f64::from));
    (a[2] * b[2] + a[1] * b[1] + a[0] * b[0]) as f32
}

/// `vecUnit` (SIMDIRT.DLL 0x1000FD40): the length stays in `f64`. Unchanged if it is zero
/// (the original leaves its output untouched).
fn unit(v: Vec3) -> Vec3 {
    let [x, y, z] = v.map(f64::from);
    let len = (x * x + y * y + z * z).sqrt();
    if len != 0.0 {
        [(x / len) as f32, (y / len) as f32, (z / len) as f32]
    } else {
        v
    }
}

/// `getVertAltForLightCalc` (libSimDirt 0x4156C): the vertex and its neighbours at (x, y+1), (x+1, y),
/// (x, y−1) and (x−1, y). A neighbour off the map is `2 · centre − opposite`, clamped.
/// Unchecked: no Windows address known yet (the ground check runs it only inside the vertex light).
fn neighbours(alt: &CellMap<u8>, x: u32, y: u32) -> [u8; 5] {
    let (vx, vy) = (alt.width(), alt.height());
    let mirror = |c: u8, o: u8| (2 * c as i32 - o as i32).clamp(0, 255) as u8;
    let c = alt.get(x, y);
    let (up, down) = if y > 0 && y < vy - 1 {
        (alt.get(x, y + 1), alt.get(x, y - 1))
    } else if y == 0 {
        let up = alt.get(x, y + 1);
        (up, mirror(c, up))
    } else {
        let down = alt.get(x, y - 1);
        (mirror(c, down), down)
    };
    let (right, left) = if x > 0 && x < vx - 1 {
        (alt.get(x + 1, y), alt.get(x - 1, y))
    } else if x == 0 {
        let right = alt.get(x + 1, y);
        (right, mirror(c, right))
    } else {
        let left = alt.get(x - 1, y);
        (mirror(c, left), left)
    };
    [c, up, right, down, left]
}

/// `getVertexNormal` (libSimDirt 0x363C4, SIMDIRT.DLL 0x100073B0): the average of the cross products of
/// the four edge vectors (0, d1, 16), (16, d2, 0), (0, d3, −16) and (−16, d4, 0), with
/// `di = (a[i] − a[0]) · scale`. Written out, x = 32·(d4 − d2) / 4 and z = 32·(d3 − d1) / 4
/// and y = 256. The original stores d1..d3 as floats but keeps d4 on the x87 stack, and sums
/// the eight terms in this order.
fn normal(a: [u8; 5]) -> Vec3 {
    let scale = ALTITUDE_SCALE as f64;
    let d = |i: usize| (a[i] as f64 - a[0] as f64) * scale;
    let [d1, d2, d3] = [d(1), d(2), d(3)].map(|v| v as f32 as f64);
    let d4 = d(4);
    let f = |v: f64| v as f32 as f64;
    let c = CELL_SIZE;
    let x = (f(-c * d2) + f(-c * d2) + f(c * d4) + c * d4) * 0.25;
    let z = (f(-c * d1) + f(c * d3) + f(c * d3) + f(-c * d1)) * 0.25;
    [x as f32, (c * c * 4.0 * 0.25) as f32, z as f32]
}

/// `getVertexLightIndex` (libSimDirt 0x41800, SIMDIRT.DLL 0x10016852) and `GetLightIndex`
/// (libSimDirt 0x5ED08):
/// the angle between the normal and the light, in steps of (π/2)/31 with the float π,
/// truncated. A dot product rounded above 1 gives NaN, which `_ftol` turns into 0.
fn light_index(n: Vec3, light: Vec3) -> u8 {
    let l = unit(light);
    let d = dot(unit(n), [-l[0], -l[1], -l[2]]).max(0.0);
    let step = PI * 0.5 / MAX_LIGHT as f64;
    let angle = (d as f64).acos() / step;
    if angle.is_nan() {
        0
    } else {
        angle as i32 as u8
    }
}

/// The light index of every vertex: the larger of the two lights' indices. Only the dirt
/// altitude counts, not the water.
pub fn vertex_light(t: &Terrain) -> CellMap<u8> {
    let alt = &t.altitude;
    let mut out = CellMap::new(alt.width(), alt.height(), 0);
    for x in 0..alt.width() {
        for y in 0..alt.height() {
            let n = normal(neighbours(alt, x, y));
            let v = LIGHTS.iter().map(|&l| light_index(n, l)).max().unwrap();
            out.set(x, y, v);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_and_slopes() {
        // Flat ground: both lights at 45°, trunc(45 / (90 / 31)) = 15.
        let flat = normal([10; 5]);
        assert_eq!(flat, [0.0, 256.0, 0.0]);
        assert_eq!(LIGHTS.map(|l| light_index(flat, l)), [15, 15]);
        // Rising towards +x, so facing −x: closer to the first light, further from the second.
        let n = normal([10, 10, 15, 10, 5]);
        let both = LIGHTS.map(|l| light_index(n, l));
        assert!(both[0] < 15 && both[1] > 15, "{both:?}");
    }

    #[test]
    fn edges_mirror() {
        let mut alt = CellMap::new(3, 3, 0u8);
        alt.set(0, 0, 10);
        alt.set(0, 1, 30);
        alt.set(1, 0, 4);
        // At (0, 0): up = 30, down mirrored = 2·10 − 30 → clamped to 0; right = 4, left = 16.
        assert_eq!(neighbours(&alt, 0, 0), [10, 30, 4, 0, 16]);
    }
}
