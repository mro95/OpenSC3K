//! `cSC3DirtGenerator`: the random terrain behind a new city. See `docs/sim/terrain-gen.md`.
//!
//! Ported from the Windows SIMDIRT.DLL, whose `cRZRandom` differs from the Loki build's.
//! Function names come from the Loki `libSimDirt.so`. Every map is indexed by vertex: a map of
//! `size` cells has `size + 1` vertices per side.

use crate::cellmap::CellMap;
use crate::rng::Random;

/// Bits of the `flags` argument of `GenerateRandom`.
pub mod flags {
    /// Sea along the y = 0 edge.
    pub const SEA_Y0: u8 = 0x01;
    /// Sea along the x = max edge.
    pub const SEA_X1: u8 = 0x02;
    /// Sea along the y = max edge.
    pub const SEA_Y1: u8 = 0x04;
    /// Sea along the x = 0 edge.
    pub const SEA_X0: u8 = 0x08;
    /// A river from the x = 0 edge to the x = max edge.
    pub const RIVER_X: u8 = 0x10;
    /// A river from the y = 0 edge to the y = max edge.
    pub const RIVER_Y: u8 = 0x20;
    /// A low centre (a lake or bay) and more water.
    pub const LOW_CENTRE: u8 = 0x40;
    /// A raised centre (a mountain).
    pub const MOUNTAIN: u8 = 0x80;
}

/// The arguments of `GenerateRandom(seed, hills, water, trees, flags)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Params {
    pub seed: u32,
    /// How much of the land keeps its full height. Larger is hillier.
    pub hills: u8,
    /// How much of the map is below sea level; also river depth and the sea's reach.
    pub water: u8,
    /// Tree density.
    pub trees: u8,
    /// See [`flags`].
    pub flags: u8,
}

impl Params {
    /// What the New City dialog passes (`ReadValuesFromWindow`): one sea edge (y = max) and a
    /// river. The original seeds from the clock (`0xFFFFFFFF`).
    pub fn new_city(seed: u32) -> Params {
        Params {
            seed,
            hills: 0x40,
            water: 0x40,
            trees: 0x40,
            flags: flags::SEA_Y1 | flags::RIVER_Y,
        }
    }
}

/// Lowest and highest altitude the subdivision produces.
const MIN_ALTITUDE: i32 = 0x10;
const MAX_ALTITUDE: i32 = 0xF0;
/// Height of the 5 × 5 control points before the sea edges lower them.
const BASE_ALTITUDE: u32 = 0x60;
/// Points along the river curve (`Bezier2D4Controls`, 1024 `cRZPoint`s).
const RIVER_POINTS: u32 = 0x400;
/// Upper bound of the flora scatter angle (SIMDIRT 0x10020C28): 6.2832, not exactly 2π.
const ANGLE_RANGE: f64 = f64::from_bits(0x4019_21FF_2E48_E8A7);

/// Per-difficulty divisors of the three altitude bands in `ChangeTerrainProfile`, and whether
/// the heights are snapped to even steps afterwards (SIMDIRT 0x10024828, 16 bytes a row).
const PROFILES: [([i32; 3], bool); 3] = [([12, 6, 3], true), ([10, 5, 3], false), ([6, 3, 2], false)];

/// The 5 × 5 control-point masks per flag bit (SIMDIRT 0x10024858, 0x10024878, 0x10024898).
/// Bit `k` is the point at x = `k % 5`, y = `k / 5` quarters of the map.
const LOW_POINTS: [u32; 8] = [0x1F, 0x0108_4210, 0x01F0_0000, 0x0010_8421, 0, 0, 0x1000, 0];
/// Points whose x midpoint to the next point is set to the average.
const MID_X_POINTS: [u32; 8] = [0x0F, 0, 0x00F0_0000, 0, 0, 0, 0, 0];
/// Points whose y midpoint to the next point is set to the average.
const MID_Y_POINTS: [u32; 8] = [0, 0x0008_4210, 0, 0x8421, 0, 0, 0, 0];

/// The generator's output, what `ReflectMap` hands to the `cSC3DirtBag`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Terrain {
    /// Map size in cells.
    pub size: u32,
    /// `SetSeaLevel` (dirt bag vtable 0x64).
    pub sea_level: u8,
    /// Vertex altitudes (vtable 0x84).
    pub altitude: CellMap<u8>,
    /// Water surface per vertex (vtable 0x8C). The generator sets every vertex to the sea level.
    pub water: CellMap<u8>,
    /// Salt-water flag per vertex (vtable 0x8C): within reach of a sea edge.
    pub salt: CellMap<bool>,
    /// Flora per vertex (vtable 0x90), 0 for none.
    pub flora: CellMap<u8>,
}

impl Terrain {
    /// Vertices per side.
    pub fn vertices(&self) -> u32 {
        self.size + 1
    }

    /// Whether the vertex is under water.
    pub fn is_water(&self, x: u32, y: u32) -> bool {
        self.altitude.get(x, y) <= self.water.get(x, y)
    }

    /// A made-up colour for top-down previews, not the game's palette. Water is blue, darker
    /// with depth and where salt; land is green to tan with height, darker with flora; slopes
    /// facing the top-left are lighter.
    pub fn preview_rgb(&self, x: u32, y: u32) -> [u8; 3] {
        let sea = self.sea_level as i32;
        let a = self.altitude.get(x, y) as i32;
        if self.is_water(x, y) {
            let depth = (sea - a).clamp(0, 40);
            let fresh = if self.salt.get(x, y) { 0 } else { 30 };
            return [20, (90 + fresh - depth * 2) as u8, (200 - depth * 3) as u8];
        }
        let up = self.altitude.get(x.saturating_sub(1), y.saturating_sub(1)) as i32;
        let light = ((a - up) * 12).clamp(-60, 60);
        let h = (a - sea).clamp(0, 120);
        let mut c = [90 + h, 150 - h / 3, 60 + h / 2];
        let f = self.flora.get(x, y) as i32;
        if f > 0 {
            c = [c[0] / 2, c[1] - 30 - f / 8, c[2] / 2];
        }
        c.map(|v| (v + light).clamp(0, 255) as u8)
    }
}

/// `cSC3DirtGenerator` after `Init(size, size)` and `SetDifficulty`.
struct Generator {
    rng: Random,
    /// Vertex counts, fields `+4` and `+8`.
    vx: u32,
    vy: u32,
    /// `+0x10`.
    sea: u8,
    /// `+0x14`, `+0x18`, `+0x1C` and the bit map at `+0x20`.
    alt: CellMap<u8>,
    water: CellMap<u8>,
    flora: CellMap<u8>,
    salt: CellMap<bool>,
    /// `+0x28`, from `SetDifficulty`: 1 easy, 2 medium, 3 hard.
    difficulty: i32,
}

/// `Init(size, size)`, `SetDifficulty(difficulty)`, `GenerateRandom(params)` and `ReflectMap`.
pub fn generate(size: u32, difficulty: i32, params: Params) -> Terrain {
    let v = size + 1;
    let mut g = Generator {
        rng: Random::new(0),
        vx: v,
        vy: v,
        sea: 0,
        alt: CellMap::new(v, v, 0),
        water: CellMap::new(v, v, 0),
        flora: CellMap::new(v, v, 0),
        salt: CellMap::new(v, v, false),
        difficulty,
    };
    g.generate_random(params);
    Terrain {
        size,
        sea_level: g.sea,
        altitude: g.alt,
        water: g.water,
        salt: g.salt,
        flora: g.flora,
    }
}

/// Unnamed (0x10017A09). A value between two neighbours, spread by `jitter`, clamped to
/// `MIN_ALTITUDE..=MAX_ALTITUDE`.
fn blend(rng: &mut Random, values: &[u8], range: u32, jitter: i32) -> u8 {
    if range < 2 {
        return values[0];
    }
    let max = *values.iter().max().unwrap() as i32;
    let min = *values.iter().min().unwrap() as i32;
    let lo = max - jitter;
    let hi = min + jitter;
    let v = if lo < hi {
        rng.gaussian_fast(lo, hi + 1)
    } else {
        (lo + hi) / 2
    };
    v.clamp(MIN_ALTITUDE, MAX_ALTITUDE) as u8
}

impl Generator {
    /// `GenerateRandom` (SIMDIRT 0x10017C2D, Loki 0x5024C).
    fn generate_random(&mut self, p: Params) {
        self.rng.seed(p.seed);
        self.alt.fill(0);
        self.water.fill(0);
        self.salt.fill(false);
        self.flora.fill(0);

        self.control_points(p.flags);
        let (x1, y1) = (self.vx - 1, self.vy - 1);
        self.altitude_subdivision(0, 0, x1, y1, x1.min(y1), 8);

        let mut histogram = [0u32; 256];
        for x in 0..self.vx {
            for y in 0..self.vy {
                histogram[self.alt.get(x, y) as usize] += 1;
            }
        }
        self.sea = sea_level(&histogram, self.vx * self.vy, p.water, p.flags);
        self.water.fill(self.sea);
        let [l1, l2, l3] = band_levels(&histogram, self.sea, self.vx * self.vy, p.hills, p.flags);

        let even = self.change_terrain_profile(self.sea.wrapping_add(2), l1, l2, l3);
        self.fix_shores(p.flags);
        self.create_rivers(p.flags, p.water);
        self.find_salt_water(p.flags, p.water);
        self.create_flora(p.trees);
        if even {
            let base = self.sea as u32 + 2;
            for x in 0..self.vx {
                for y in 0..self.vy {
                    let a = self.alt.get(x, y) as u32;
                    if a > base {
                        self.alt.set(x, y, (((a - base) & 0xFE) + base) as u8);
                    }
                }
            }
        }
    }

    /// The start of `GenerateRandom`: 25 control points on a 5 × 5 grid of map quarters.
    /// Points on a sea edge start lower, `MOUNTAIN` raises the centre, and the midpoints along
    /// sea edges are averaged so the coast stays straight.
    fn control_points(&mut self, flags: u8) {
        let (mut low, mut mid_x, mut mid_y) = (0, 0, 0);
        for bit in 0..8 {
            if flags & (1 << bit) != 0 {
                low |= LOW_POINTS[bit];
                mid_x |= MID_X_POINTS[bit];
                mid_y |= MID_Y_POINTS[bit];
            }
        }
        let size = (self.vx - 1).min(self.vy - 1);
        let qx = (self.vx - 1) >> 2;
        let qy = (self.vy - 1) >> 2;
        let point = |k: u32| ((k % 5) * qx, (k / 5) * qy);
        for k in 0..25 {
            let base = if low & (1 << k) != 0 {
                BASE_ALTITUDE - (size >> 2)
            } else {
                BASE_ALTITUDE
            };
            let (x, y) = point(k);
            let a = blend(&mut self.rng, &[base as u8, base as u8], 0x10, 0x0F);
            self.alt.set(x, y, a);
        }
        if flags & flags::MOUNTAIN != 0 {
            let peak = ((size >> 2) * 2 + BASE_ALTITUDE) as u8;
            let a = blend(&mut self.rng, &[peak, peak], 0x10, 0x0F);
            self.alt.set(2 * qx, 2 * qy, a);
        }
        for k in 0..25 {
            let (x, y) = point(k);
            if mid_x & (1 << k) != 0 {
                let avg = (self.alt.get(x, y) as u32 + self.alt.get(x + qx, y) as u32) / 2;
                self.alt.set(x + (qx >> 1), y, avg as u8);
            }
            if mid_y & (1 << k) != 0 {
                let avg = (self.alt.get(x, y) as u32 + self.alt.get(x, y + qy) as u32) / 2;
                self.alt.set(x, y + (qy >> 1), avg as u8);
            }
        }
    }

    /// `AltitudeSubdivision(x0, y0, x1, y1, range, depth)` (SIMDIRT 0x100177B7, Loki 0x4FAAC).
    /// Midpoint displacement: fills the unset (0) edge midpoints and centre of the box, then
    /// recurses into its four quarters with half the range.
    fn altitude_subdivision(&mut self, x0: u32, y0: u32, x1: u32, y1: u32, range: u32, depth: u32) {
        let mut mx = (x0 + x1) >> 1;
        let mut my = (y0 + y1) >> 1;
        if x1.abs_diff(x0) & 1 != 0 && self.rng.uniform(2) != 0 {
            mx += 1;
        }
        if y1.abs_diff(y0) & 1 != 0 && self.rng.uniform(2) != 0 {
            my += 1;
        }
        let a = self.alt.get(x0, y0);
        let b = self.alt.get(x0, y1);
        let c = self.alt.get(x1, y0);
        let d = self.alt.get(x1, y1);
        let mut left = self.alt.get(x0, my);
        let mut right = self.alt.get(x1, my);
        let mut top = self.alt.get(mx, y0);
        let mut bottom = self.alt.get(mx, y1);
        let centre = self.alt.get(mx, my);
        let jitter = range as i32 - 1;
        if left == 0 {
            left = blend(&mut self.rng, &[a, b], range, jitter);
            self.alt.set(x0, my, left);
        }
        if right == 0 {
            right = blend(&mut self.rng, &[c, d], range, jitter);
            self.alt.set(x1, my, right);
        }
        if top == 0 {
            top = blend(&mut self.rng, &[a, c], range, jitter);
            self.alt.set(mx, y0, top);
        }
        if bottom == 0 {
            bottom = blend(&mut self.rng, &[b, d], range, jitter);
            self.alt.set(mx, y1, bottom);
        }
        if centre == 0 {
            let v = blend(&mut self.rng, &[left, right, top, bottom], range, jitter);
            self.alt.set(mx, my, v);
        }
        if depth == 0 {
            return;
        }
        let range = range >> 1;
        self.altitude_subdivision(x0, y0, mx, my, range, depth - 1);
        self.altitude_subdivision(x0, my, mx, y1, range, depth - 1);
        self.altitude_subdivision(mx, y0, x1, my, range, depth - 1);
        self.altitude_subdivision(mx, my, x1, y1, range, depth - 1);
    }

    /// `ChangeTerrainProfile(lo, l1, l2, l3)` (SIMDIRT 0x10017AF6, Loki 0x50070). Squashes the
    /// three bands above `lo` by the difficulty's divisors and shifts the rest down to match.
    /// Returns whether the heights are snapped to even steps at the end.
    fn change_terrain_profile(&mut self, lo: u8, l1: u8, l2: u8, l3: u8) -> bool {
        let row = match self.difficulty {
            2 => 1,
            3 => 2,
            _ => 0,
        };
        let ([d1, d2, d3], even) = PROFILES[row];
        let (lo, l1, l2, l3) = (lo as i32, l1 as i32, l2 as i32, l3 as i32);
        let top1 = ((l1 - lo) / d1 + lo) as u8 as i32;
        let top2 = ((l2 - l1) / d2 + top1) as u8 as i32;
        for x in 0..self.vx {
            for y in 0..self.vy {
                let a = self.alt.get(x, y) as i32;
                if a < lo {
                    continue;
                }
                let v = if a < l1 {
                    (a - lo) / d1 + lo
                } else if a < l2 {
                    (a - l1) / d2 + top1
                } else if a < l3 {
                    (a - l2) / d3 + top2
                } else {
                    a + ((l3 - l2) / d3 + top2 - l3)
                };
                self.alt.set(x, y, v as u8);
            }
        }
        even
    }

    /// `FixShores(flags)` (SIMDIRT 0x100185C8, Loki 0x51674). Along each sea edge, land
    /// vertices get a ramp down to the edge, 1 to 5 vertices deep, wandering by at most one
    /// per step.
    fn fix_shores(&mut self, flags: u8) {
        let (vx, vy) = (self.vx, self.vy);
        // (flag, length of the edge, edge vertex at i, step inwards).
        type Edge = (u8, u32, fn(u32, u32, u32) -> (u32, u32), (i32, i32));
        let edges: [Edge; 4] = [
            (flags::SEA_Y0, vx, |i, _, _| (i, 0), (0, 1)),
            (flags::SEA_X1, vy, |i, vx, _| (vx - 1, i), (-1, 0)),
            (flags::SEA_Y1, vx, |i, _, vy| (i, vy - 1), (0, -1)),
            (flags::SEA_X0, vy, |i, _, _| (0, i), (1, 0)),
        ];
        for (flag, len, at, (dx, dy)) in edges {
            if flags & flag == 0 {
                continue;
            }
            let mut run = 3;
            for i in 0..len {
                let (ex, ey) = at(i, vx, vy);
                if self.alt.get(ex, ey) <= self.sea {
                    continue;
                }
                let r = self.rng.gaussian_fast(0, 7);
                if run < r {
                    run += 1;
                } else if r < run {
                    run -= 1;
                }
                let (mut x, mut y) = (ex as i32, ey as i32);
                for depth in (1..=run).rev() {
                    let cap = self.sea.wrapping_sub(depth as u8).wrapping_add(3);
                    if cap < self.alt.get(x as u32, y as u32) {
                        self.alt.set(x as u32, y as u32, cap);
                    }
                    x += dx;
                    y += dy;
                }
            }
        }
    }

    /// `CreateRivers(flags, water)` (SIMDIRT 0x100187DA, Loki 0x51A7C). A cubic Bézier from
    /// one edge to the opposite one, carved with `traceDepth` in four directions at each of
    /// its points. The depth runs from one end to the other.
    fn create_rivers(&mut self, flags: u8, water: u8) {
        if flags & (flags::RIVER_X | flags::RIVER_Y) == 0 {
            return;
        }
        let (vx, vy) = (self.vx, self.vy);
        let sea = self.sea as i32;
        let mut depth = ((vx as f64).sqrt() * 0.5) as i32 * (water as i32 + 0x40) / 0x80;
        if depth >= sea {
            depth = sea - 1;
        }
        let (mut start, mut end) = (depth, depth);
        let mut ends = [(0i32, 0i32); 4];
        let range = |rng: &mut Random, n: u32, a: u32, b: u32| {
            rng.range((n * a / b) as i32, (n * (b - a) / b) as i32)
        };
        if flags & flags::RIVER_Y != 0 {
            let xa = range(&mut self.rng, vx, 1, 4);
            let xb = range(&mut self.rng, vx, 1, 4);
            ends[0] = (xa, 0);
            ends[3] = (xb, vy as i32 - 1);
            ends[1] = (range(&mut self.rng, vx, 1, 8), range(&mut self.rng, vy, 1, 8));
            ends[2] = (range(&mut self.rng, vx, 1, 8), range(&mut self.rng, vy, 1, 8));
            if flags & flags::SEA_Y0 != 0 {
                start = (sea - self.alt.get(xa as u32, 0) as i32).max(depth);
            }
            if flags & flags::SEA_Y1 != 0 {
                end = (sea - self.alt.get(xb as u32, vy - 1) as i32).max(depth);
            }
        }
        if flags & flags::RIVER_X != 0 {
            let ya = range(&mut self.rng, vy, 1, 4);
            let yb = range(&mut self.rng, vy, 1, 4);
            ends[0] = (0, ya);
            ends[3] = (vx as i32 - 1, yb);
            ends[1] = (range(&mut self.rng, vx, 1, 8), range(&mut self.rng, vy, 1, 8));
            ends[2] = (range(&mut self.rng, vx, 1, 8), range(&mut self.rng, vy, 1, 8));
            if flags & flags::SEA_X0 != 0 {
                start = (sea - self.alt.get(0, ya as u32) as i32).max(depth);
            }
            if flags & flags::SEA_X1 != 0 {
                end = (sea - self.alt.get(vx - 1, yb as u32) as i32).max(depth);
            }
        }
        let points = bezier(ends, RIVER_POINTS);
        for (i, &(x, y)) in points.iter().enumerate() {
            if i > 0 && points[i - 1] == (x, y) {
                continue;
            }
            // The original interpolates in unsigned arithmetic and keeps the low byte.
            let step = ((end - start) as u32).wrapping_mul(i as u32) >> 10;
            let d = (step as u8).wrapping_add(start as u8);
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let jitter = self.rng.range(0, 3) as u8;
                self.trace_depth(x, y, dx, dy, jitter.wrapping_add(d));
            }
        }
    }

    /// `traceDepth(x, y, dx, dy, depth)` (SIMDIRT 0x10018B57, Loki 0x52234). Cuts a slope
    /// starting `depth` below sea level: 2 up per step under water, 1 above, until it leaves
    /// the map or reaches 0xFC.
    fn trace_depth(&mut self, mut x: i32, mut y: i32, dx: i32, dy: i32, depth: u8) {
        let mut level = self.sea.wrapping_sub(depth);
        loop {
            if level < self.alt.get(x as u32, y as u32) {
                self.alt.set(x as u32, y as u32, level);
            }
            x += dx;
            y += dy;
            level = level.wrapping_add(if level < self.sea { 2 } else { 1 });
            let inside = x >= 0 && y >= 0 && (x as u32) < self.vx && (y as u32) < self.vy;
            if !inside || level >= 0xFC {
                break;
            }
        }
    }

    /// `FindSaltWater(flags, water)` (SIMDIRT 0x100181D6, Loki 0x5101C). From each sea edge,
    /// marks up to `water × width / 256` vertices inwards as salt, stopping after the fifth
    /// vertex above water.
    fn find_salt_water(&mut self, flags: u8, water: u8) {
        self.salt.fill(false);
        let (vx, vy) = (self.vx, self.vy);
        let reach = (water as u32 * vx) >> 8;
        // (flag, lines, 0 if the lines run along x, first vertex, step inwards).
        type Line = (u8, u32, u32, (i32, i32), (i32, i32));
        let lines: [Line; 4] = [
            (flags::SEA_Y0, vx, 0, (0, 0), (0, 1)),
            (flags::SEA_X1, vy, 1, (vx as i32 - 1, 0), (-1, 0)),
            (flags::SEA_Y1, vx, 0, (0, vy as i32 - 1), (0, -1)),
            (flags::SEA_X0, vy, 1, (0, 0), (1, 0)),
        ];
        for (flag, count, axis, (fx, fy), (dx, dy)) in lines {
            if flags & flag == 0 {
                continue;
            }
            for line in 0..count as i32 {
                let (mut x, mut y) = if axis == 0 { (line, fy) } else { (fx, line) };
                let mut left = reach;
                let mut land = 5;
                while left != 0 {
                    let (ux, uy) = (x as u32, y as u32);
                    if (self.water.get(ux, uy) as i32) < self.alt.get(ux, uy) as i32 - 1 {
                        land -= 1;
                    }
                    left -= 1;
                    self.salt.set(ux, uy, true);
                    x += dx;
                    y += dy;
                    if land == 0 {
                        break;
                    }
                }
            }
        }
    }

    /// `CreateFlora(trees)` (SIMDIRT 0x10018427, Loki 0x513FC). `trees × vertices / 65536`
    /// clumps, each a random number of points scattered in a circle. Points on land get a flora
    /// value, lower near salt water.
    fn create_flora(&mut self, trees: u8) {
        let clumps = (trees as u32 * self.vx * self.vy) >> 16;
        let max_radius = ((trees >> 2) as i32).max(2);
        for _ in 0..clumps {
            let cx = self.rng.uniform(self.vx) as i32;
            let cy = self.rng.uniform(self.vy) as i32;
            let radius = self.rng.gaussian_fast(0, max_radius) as u32;
            let count = if radius > 1 {
                self.rng.gaussian_fast(0, (radius * radius) as i32) as u32
            } else {
                1
            };
            for _ in 0..count {
                let d = if radius > 1 { self.rng.uniform(radius) } else { 0 } as f64;
                let angle = self.rng.double_range(0.0, ANGLE_RANGE);
                let x = (angle.cos() * d) as i32 + cx;
                let y = (angle.sin() * d) as i32 + cy;
                if x < 0 || y < 0 || x as u32 >= self.vx || y as u32 >= self.vy {
                    continue;
                }
                let (ux, uy) = (x as u32, y as u32);
                if self.alt.get(ux, uy) <= self.water.get(ux, uy) {
                    continue;
                }
                let top = if self.salt.get(ux, uy) { 0x80 } else { 0x100 };
                let v = self.rng.range_min_of_two(0, top);
                self.flora.set(ux, uy, v as u8);
            }
        }
    }
}

/// The sea level: the lowest altitude with at least `vertices × max(water, 4) × k / 1536`
/// vertices at or below it, where `k` is 1, plus 2 per sea edge, plus 3 for `LOW_CENTRE`.
/// With `water` above 0x7F the share is 90%.
fn sea_level(histogram: &[u32; 256], vertices: u32, water: u8, flags: u8) -> u8 {
    let mut k = 1 + 2 * (flags & 0x0F).count_ones();
    if flags & flags::LOW_CENTRE != 0 {
        k += 3;
    }
    let share = if water > 3 { water as u32 } else { 4 };
    let mut target = vertices * share * k / 0x600;
    if water > 0x7F {
        target = vertices * 9 / 10;
    }
    let target = target.max(1);
    let mut sum = 0;
    for (level, &n) in histogram.iter().enumerate() {
        sum += n;
        if sum >= target {
            return level as u8;
        }
    }
    0xFF
}

/// The three band tops for `ChangeTerrainProfile`. With `f = 1 − √(hills / 512)` (`/ 256`
/// with `MOUNTAIN`), each band holds `vertices × f`, `× f³` and `× f⁴` vertices, counted
/// upwards from the previous top. 0xFF if a band never fills.
fn band_levels(histogram: &[u32; 256], sea: u8, vertices: u32, hills: u8, flags: u8) -> [u8; 3] {
    let scale = if flags & flags::MOUNTAIN != 0 { 128.0 } else { 256.0 };
    let f = 1.0 - (hills as f64 / scale * 0.5).sqrt();
    let n = vertices as f64;
    let targets = [(n * f) as u32, (n * f * f * f) as u32, (n * f * f * f * f) as u32];
    let mut levels = [0xFF; 3];
    let mut from = sea as usize + 1;
    for (level, target) in levels.iter_mut().zip(targets) {
        let mut sum = 0;
        for (v, &count) in histogram.iter().enumerate().skip(from) {
            sum += count;
            if sum >= target {
                *level = v as u8;
                break;
            }
        }
        from = *level as usize + 1;
    }
    levels
}

/// `Bezier2D4Controls<cRZPoint>` (SIMDIRT 0x10018C30): `n` points of a cubic Bézier at
/// `t = 1/n, 2/n, …, 1`, truncated to integers. The original accumulates `t` and sums the
/// terms on the x87 stack, in the order kept here.
fn bezier(p: [(i32, i32); 4], n: u32) -> Vec<(i32, i32)> {
    let dt = 1.0 / n as f64;
    let mut t = dt;
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let t2 = t * t;
        let t3 = t2 * t;
        let b0 = 1.0 - 3.0 * t + 3.0 * t2 - t3;
        let b1 = 3.0 * t - t2 * 6.0 + 3.0 * t3;
        let b2 = 3.0 * t2 - 3.0 * t3;
        let f = |v: i32| v as f64;
        let x = f(p[0].0) * b0 + f(p[1].0) * b1 + f(p[2].0) * b2 + f(p[3].0) * t3;
        let y = f(p[2].1) * b2 + f(p[0].1) * b0 + f(p[3].1) * t3 + f(p[1].1) * b1;
        out.push((x as i32, y as i32));
        t += dt;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_terrain() {
        let a = generate(64, 1, Params::new_city(42));
        let b = generate(64, 1, Params::new_city(42));
        assert_eq!(a, b);
        let c = generate(64, 1, Params::new_city(43));
        assert_ne!(a.altitude, c.altitude);
    }

    #[test]
    fn altitudes_and_sea() {
        for (seed, size, difficulty) in [(1, 64, 1), (2, 128, 2), (3, 256, 3), (4, 256, 1)] {
            let t = generate(size, difficulty, Params::new_city(seed));
            assert_eq!(t.altitude.width(), size + 1);
            let v = t.vertices();
            let mut under = 0;
            for x in 0..v {
                for y in 0..v {
                    assert_eq!(t.water.get(x, y), t.sea_level);
                    if t.is_water(x, y) {
                        under += 1;
                        assert_eq!(t.flora.get(x, y), 0, "no trees in water");
                    }
                }
            }
            // The dialog's settings put about an eighth of the map below sea level before the
            // shores and river are cut.
            let share = under as f64 / (v * v) as f64;
            assert!((0.08..0.5).contains(&share), "seed {seed}: {share}");
            // The sea edge (y = max) is water along most of its length.
            let wet = (0..v).filter(|&x| t.is_water(x, v - 1)).count();
            assert!(wet as u32 > v * 3 / 4, "seed {seed}: {wet}/{v}");
            // The river reaches the y = 0 edge.
            assert!((0..v).any(|x| t.is_water(x, 0)), "seed {seed}");
            // Salt is only near the sea edge.
            assert!((0..v).all(|x| !t.salt.get(x, 0)));
        }
    }

    #[test]
    fn blend_clamps_and_short_range() {
        let mut r = Random::new(5);
        assert_eq!(blend(&mut r, &[7, 200], 1, 0), 7, "range < 2 keeps the first value");
        for _ in 0..1000 {
            let v = blend(&mut r, &[16, 16], 0x10, 0x0F);
            assert!((16..=31).contains(&v), "{v}");
        }
        // Without spread the result is the midpoint.
        assert_eq!(blend(&mut r, &[100, 120], 4, 3), 110);
    }

    #[test]
    fn bezier_ends_on_last_control() {
        let pts = bezier([(10, 0), (5, 20), (30, 40), (20, 64)], 0x400);
        assert_eq!(pts.len(), 0x400);
        let (x, y) = *pts.last().unwrap();
        assert!((19..=20).contains(&x) && (63..=64).contains(&y), "{x},{y}");
    }

    #[test]
    fn easy_snaps_to_even_steps() {
        let t = generate(64, 1, Params::new_city(9));
        let base = t.sea_level as u32 + 2;
        for x in 0..65 {
            for y in 0..65 {
                let a = t.altitude.get(x, y) as u32;
                if a > base {
                    assert_eq!((a - base) % 2, 0);
                }
            }
        }
    }
}
