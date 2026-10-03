//! Per-vertex light, as `cSC3DirtBag::calculateAndSetVertexLight` (libSimDirt 0x4142C) stores
//! it in the dirt bag. See `docs/render/terrain.md`, "Vertex light".

use sc3k_sim::cellmap::CellMap;
use sc3k_sim::dirt::Terrain;

/// Horizontal size of a cell in world units (`cSC3City::CellSizeInWorldUnitsX/Z`).
const CELL_SIZE: f32 = 16.0;
/// World units per altitude step (`cSC3City::CellSizeInWorldUnitsY`, `AltitudeScale`).
const ALTITUDE_SCALE: f32 = 3.266;
/// Highest light index (`cSC3DirtClodColorLightTable::mkLightIndexMax`).
pub const MAX_LIGHT: u8 = 31;
/// The two light directions (libSimDirt rodata 0x61698, 0x616A4), as (x, up, y).
const LIGHTS: [[f32; 3]; 2] = [[1.0, -1.0, 0.0], [-1.0, -1.0, 0.0]];

type Vec3 = [f32; 3];

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[2] * b[2] + a[1] * b[1] + a[0] * b[0]
}

/// `vecUnit`: unchanged if the length is zero.
fn unit(v: Vec3) -> Vec3 {
    let len = dot(v, v).sqrt();
    if len > 0.0 {
        [v[0] / len, v[1] / len, v[2] / len]
    } else {
        v
    }
}

/// `getVertAltForLightCalc` (0x4156C): the vertex and its neighbours at (x, y+1), (x+1, y),
/// (x, y−1) and (x−1, y). A neighbour off the map is `2 · centre − opposite`, clamped.
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

/// `getVertexNormal` (0x363C4): the average of the cross products of the four edge vectors.
fn normal(a: [u8; 5]) -> Vec3 {
    let d = |i: usize| (a[i] as f32 - a[0] as f32) * ALTITUDE_SCALE;
    let v1 = [0.0, d(1), CELL_SIZE];
    let v2 = [CELL_SIZE, d(2), 0.0];
    let v3 = [0.0, d(3), -CELL_SIZE];
    let v4 = [-CELL_SIZE, d(4), 0.0];
    let mut n = [0.0; 3];
    for c in [cross(v1, v2), cross(v2, v3), cross(v3, v4), cross(v4, v1)] {
        for k in 0..3 {
            n[k] += c[k];
        }
    }
    n.map(|v| v / 4.0)
}

/// `getVertexLightIndex` (0x41800) and `GetLightIndex` (0x5ED08): the angle between the
/// normal and the light, in steps of (π/2)/31, truncated.
fn light_index(n: Vec3, light: Vec3) -> u8 {
    let l = unit(light);
    let d = dot(unit(n), [-l[0], -l[1], -l[2]]).max(0.0);
    let step = std::f64::consts::PI * 0.5 / MAX_LIGHT as f64;
    ((d.min(1.0) as f64).acos() / step) as u8
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
        assert_eq!(flat[0], 0.0);
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
