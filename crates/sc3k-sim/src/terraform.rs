//! Terraforming: `cSC3DirtBag`'s raise, lower and level operations and their cost estimates
//! (libSimDirt, SIMDIRT.DLL). Each one sets the vertices of a rectangle, then lets a
//! `cSC3ConvexContour` grow outwards ring by ring until no neighbouring vertices differ by more
//! than 4. `tools/diffcheck/run.py terraform` runs them on every saved terrain.
//!
//! The original recomputes the vertex light around the edit; here the renderer does that from
//! the altitudes. It also posts a message per changed cell, which the port does not.

use crate::cellmap::CellMap;
use crate::dirt::Terrain;

/// Most vertices a contour ring holds (`addPri`, `addSec`).
const RING_MAX: usize = 0x400;
/// `GrowAndFix` gives up once the new ring holds more than this.
const RING_GIVE_UP: usize = 0x3FC;
/// The steepest slope the contour allows. Written into the code, not `MaxAltitudeDelta`.
const SLOPE: i32 = 4;

/// A cell rectangle: `cSC3CityBounds` without its 8.8 fixed point and its y. The operations
/// edit the vertices from (x1, z1) to (x2, z2), both included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub x1: u32,
    pub z1: u32,
    pub x2: u32,
    pub z2: u32,
}

/// The simulator values the costs scale by (`cISC3City` vtable 0x15C on Windows, `GetValue`).
#[derive(Clone, Copy, Debug)]
pub struct Costs {
    /// `GetValue(8)`, per altitude step levelled.
    pub level: u32,
    /// `GetValue(9)`, per vertex raised.
    pub raise: u32,
    /// `GetValue(10)`, per vertex lowered.
    pub lower: u32,
}

/// What an operation reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub ok: bool,
    /// The cells the operation touched, grown by one; `None` when it failed before that.
    pub bounds: Option<Bounds>,
    /// In quarters of the simulator values, rounded down per step.
    pub cost: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Raise,
    Lower,
    Level(u8),
}

impl Kind {
    /// The edge vertex's new altitude.
    fn target(self, a: u8) -> u8 {
        match self {
            Kind::Raise => a.saturating_add(1),
            Kind::Lower => a.saturating_sub(1),
            Kind::Level(l) => l,
        }
    }
}

/// `cSC3ContourVertex`: a vertex and the altitude the contour gives it.
#[derive(Clone, Copy, Debug)]
struct Vertex {
    x: i16,
    z: i16,
    alt: u8,
    /// The contour leaves the vertex as it is; only the others are set.
    keep: bool,
}

impl Vertex {
    fn at(&self, x: i16, z: i16) -> bool {
        self.x == x && self.z == z
    }

    /// Steps between two vertices along the axes.
    fn steps(&self, o: &Vertex) -> i32 {
        (self.x as i32 - o.x as i32).abs() + (self.z as i32 - o.z as i32).abs()
    }

    /// The neighbour in direction `d`, 0 to 7 counter-clockwise from (−1, −1).
    fn step(&self, d: u32) -> Vertex {
        let (dx, dz) = [(-1, -1), (0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0)]
            [(d & 7) as usize];
        Vertex { x: self.x + dx, z: self.z + dz, alt: 0, keep: false }
    }
}

/// The direction from `a` to `cur`, as `GrowAndFix` numbers it. A step towards lower z and
/// lower x is 5, as a straight step towards lower z, not 4: both builds have it. The rings
/// of the check's operations never have that step.
fn direction(a: &Vertex, cur: &Vertex) -> u32 {
    if a.z < cur.z {
        if a.x < cur.x {
            0
        } else if cur.x < a.x {
            2
        } else {
            1
        }
    } else if cur.z < a.z {
        if a.x < cur.x {
            6
        } else {
            5
        }
    } else if a.x < cur.x {
        7
    } else {
        3
    }
}

/// The dirt bag's change bits and the contour's two rings, which the original keeps in statics.
pub struct Terraformer {
    /// One bit per cell: whether the last operation changed one of its corners
    /// (`cSC3DirtBag` +0x40).
    pub changed: CellMap<bool>,
    pri: Vec<Vertex>,
    sec: Vec<Vertex>,
}

impl Terraformer {
    pub fn new(size: u32) -> Terraformer {
        Terraformer { changed: CellMap::new(size, size, false), pri: Vec::new(), sec: Vec::new() }
    }

    /// `ResetChanges` (libSimDirt 0x41DDC, SIMDIRT.DLL 0x1000DED0).
    pub fn reset_changes(&mut self) {
        self.changed.fill(false);
    }

    /// `MarkCellChanged` (libSimDirt 0x41E28, SIMDIRT.DLL 0x1000DF10).
    fn mark_cell(&mut self, x: u32, z: u32) {
        self.changed.set(x, z, true);
    }

    /// `MarkVertexChanged` (libSimDirt 0x3E3B0, SIMDIRT.DLL 0x1000DF40): the cells around it.
    fn mark_vertex(&mut self, t: &Terrain, x: u32, z: u32) {
        if x != 0 {
            if z != 0 {
                self.mark_cell(x - 1, z - 1);
            }
            if z < t.size {
                self.mark_cell(x - 1, z);
            }
        }
        if x < t.size && z != 0 {
            self.mark_cell(x, z - 1);
        }
        if x < t.size && z < t.size {
            self.mark_cell(x, z);
        }
    }

    /// `SetVertexAltitude(x, z, a)` (libSimDirt 0x34EB0, SIMDIRT.DLL 0x10005DF0), with the
    /// light updates off.
    fn set_altitude(&mut self, t: &mut Terrain, x: u32, z: u32, a: u8) {
        if t.altitude.get(x, z) != a {
            self.mark_vertex(t, x, z);
            t.altitude.set(x, z, a);
        }
    }

    /// `SetWaterTable(x1, z1, x2, z2, w)` (libSimDirt 0x352D0, SIMDIRT.DLL 0x100060D0), with
    /// the light updates off.
    fn set_water(&mut self, t: &mut Terrain, x1: u32, z1: u32, x2: u32, z2: u32, w: u8) {
        let mut any = false;
        for x in x1..=x2 {
            for z in z1..=z2 {
                if t.water.get(x, z) != w {
                    self.mark_vertex(t, x, z);
                    any = true;
                }
            }
        }
        if any {
            t.water.set_rect(x1, z1, x2, z2, w);
        }
    }

    /// `cSC3ConvexContour::addPri` (libSimDirt 0x41C64, SIMDIRT.DLL 0x100097A0): a vertex of
    /// the edited rectangle's edge, kept as it is.
    fn add_pri(&mut self, t: &Terrain, x: u32, z: u32, alt: u8) -> bool {
        if t.altitude.get(x, z) != alt {
            self.mark_vertex(t, x, z);
        }
        if self.pri.len() >= RING_MAX {
            return false;
        }
        self.pri.push(Vertex { x: x as i16, z: z as i16, alt, keep: true });
        true
    }

    /// `cSC3ConvexContour::addSec` (libSimDirt 0x381F4, SIMDIRT.DLL 0x10009830): a vertex of
    /// the next ring. The same vertex twice in a row is stored once, kept if either was.
    fn add_sec(&mut self, t: &Terrain, v: Vertex) -> bool {
        if t.altitude.get(v.x as u32, v.z as u32) != v.alt {
            self.mark_vertex(t, v.x as u32, v.z as u32);
        }
        if self.sec.len() >= RING_MAX {
            return false;
        }
        match self.sec.last_mut() {
            Some(last) if last.at(v.x, v.z) => last.keep = last.keep || v.keep,
            _ => self.sec.push(v),
        }
        true
    }

    /// `cSC3ConvexContour::addConnection` (libSimDirt 0x3843C, SIMDIRT.DLL 0x10009AC0): before
    /// `v` joins the next ring, a ring vertex that keeps the ring connected, when `v` is not
    /// next to its end.
    fn add_connection(&mut self, t: &Terrain, i: usize, v: Vertex) {
        let Some(last) = self.sec.last().copied() else { return };
        if v.steps(&last) == 1 || v.at(last.x, last.z) {
            return;
        }
        let p = self.pri[i];
        let q = if p.steps(&last) == 1 {
            p
        } else {
            self.pri[if i == 0 { self.pri.len() - 1 } else { i - 1 }]
        };
        self.add_sec(t, q);
    }

    /// `cSC3ConvexContour::outOfBounds` (libSimDirt 0x41D1C).
    /// Unchecked: Windows inlines it into `GrowAndFix` (SIMDIRT.DLL 0x100076E0), which the
    /// ground check runs.
    fn out_of_bounds(t: &Terrain, v: &Vertex) -> bool {
        let n = t.vertices() as i32;
        !(v.x >= 0 && (v.x as i32) < n && v.z >= 0 && (v.z as i32) < n)
    }

    /// `cSC3ConvexContour::needsAdjustment` (libSimDirt 0x382EC, SIMDIRT.DLL 0x10009920):
    /// brings `v` within the slope of the ring's last vertex and the ring vertices near it.
    /// The neighbourhood is ring vertices i − 3 to i + 2, the index wrapped as unsigned.
    fn needs_adjustment(&self, i: usize, v: &mut Vertex, up: &mut u32, down: &mut u32) -> bool {
        let (mut hi, mut lo) = (self.pri[i].alt, self.pri[i].alt);
        if let Some(last) = self.sec.last() {
            hi = hi.max(last.alt);
            lo = lo.min(last.alt);
        }
        let n = self.pri.len() as u32;
        for off in -3..3 {
            let p = &self.pri[((i as i32 + off) as u32 % n) as usize];
            if (v.x as i32 - p.x as i32).abs() < 2 && (v.z as i32 - p.z as i32).abs() < 2 {
                hi = hi.max(p.alt);
                lo = lo.min(p.alt);
            }
        }
        let min = hi.saturating_sub(SLOPE as u8);
        let max = lo.saturating_add(SLOPE as u8);
        if v.alt < min {
            *up = up.wrapping_add((min - v.alt) as u32);
            v.alt = min;
        } else if v.alt > max {
            *down = down.wrapping_add((v.alt - max) as u32);
            v.alt = max;
        } else {
            return false;
        }
        true
    }

    /// `cSC3ConvexContour::GrowAndFix` (libSimDirt 0x3658C, SIMDIRT.DLL 0x100076E0): builds
    /// the next ring outside the current one, each new vertex brought within the slope, and
    /// makes it current. Returns false when the ring grew too long, and how many steps the
    /// new vertices went up and down.
    fn grow_and_fix(&mut self, t: &Terrain) -> (bool, u32, u32) {
        self.sec.clear();
        let (mut up, mut down) = (0, 0);
        let n = self.pri.len();
        for i in 0..n {
            let cur = self.pri[i];
            let into = direction(&self.pri[if i == 0 { n - 1 } else { i - 1 }], &cur);
            let out = direction(&self.pri[if i == n - 1 { 0 } else { i + 1 }], &cur);
            let turn = out.wrapping_sub(into) & 7;
            let steps = if n == 1 {
                8
            } else {
                match turn {
                    0 => 6,
                    4 => 2,
                    6 => 4,
                    _ => 0,
                }
            };
            if turn == 2 {
                self.add_connection(t, i, cur);
                self.add_sec(t, cur);
            } else {
                let mut extra = false;
                let mut k = 2;
                while k <= steps {
                    let mut c = cur.step(into + k);
                    let repeat = self.sec.last().is_some_and(|l| l.at(c.x, c.z));
                    if !repeat {
                        let fix = !Self::out_of_bounds(t, &c) && {
                            c.alt = t.altitude.get(c.x as u32, c.z as u32);
                            self.needs_adjustment(i, &mut c, &mut up, &mut down) || extra
                        };
                        if !fix {
                            self.add_connection(t, i, cur);
                            self.add_sec(t, cur);
                        } else if k != 3 {
                            self.add_connection(t, i, c);
                            self.add_sec(t, c);
                        } else {
                            let mut d = cur.step(into + 2);
                            d.alt = t.altitude.get(d.x as u32, d.z as u32);
                            self.add_connection(t, i, d);
                            self.add_sec(t, d);
                            self.add_sec(t, c);
                            extra = true;
                        }
                    }
                    k += if turn == 6 { 1 } else { 2 };
                }
            }
            if self.sec.len() > RING_GIVE_UP {
                return (false, up, down);
            }
        }
        if let Some(&first) = self.sec.first() {
            self.add_connection(t, 0, first);
        }
        if self.sec.len() > 1 {
            let (first, last) = (self.sec[0], self.sec[self.sec.len() - 1]);
            if first.at(last.x, last.z) {
                self.sec.pop();
            }
        }
        std::mem::swap(&mut self.pri, &mut self.sec);
        (true, up, down)
    }

    /// `cSC3ConvexContour::Bounds` (libSimDirt 0x36B20, SIMDIRT.DLL 0x10007C90): the cells
    /// the ring touches, grown by one towards 0 and clamped to the map.
    fn contour_bounds(&self, t: &Terrain) -> Bounds {
        if self.pri.is_empty() {
            return Bounds { x1: 0, z1: 0, x2: 0, z2: 0 };
        }
        // Compared as unsigned, as the original does.
        let wide = |v: i16| v as i32 as u32;
        let mut b = Bounds { x1: 0x100, z1: 0x100, x2: 0, z2: 0 };
        for v in &self.pri {
            b.x1 = b.x1.min(wide(v.x));
            b.x2 = b.x2.max(wide(v.x));
            b.z1 = b.z1.min(wide(v.z));
            b.z2 = b.z2.max(wide(v.z));
        }
        b.x1 = b.x1.saturating_sub(1);
        b.z1 = b.z1.saturating_sub(1);
        b.x2 = b.x2.min(t.size - 1);
        b.z2 = b.z2.min(t.size - 1);
        b
    }

    /// The rectangle's edge vertices, clockwise from (x1, z1), each once.
    fn edge(b: Bounds) -> Vec<(u32, u32)> {
        let mut e: Vec<(u32, u32)> = (b.x1..b.x2).map(|x| (x, b.z1)).collect();
        e.extend((b.z1..b.z2).map(|z| (b.x2, z)));
        e.extend((b.x1 + 1..=b.x2).rev().map(|x| (x, b.z2)));
        e.extend((b.z1 + 1..=b.z2).rev().map(|z| (b.x1, z)));
        e
    }

    /// The interior's cost: how many vertices change, or for levelling how many steps.
    fn inside(&mut self, t: &mut Terrain, b: Bounds, kind: Kind, apply: bool, c: Costs) -> u32 {
        let mut n = 0u32;
        for x in b.x1..=b.x2 {
            for z in b.z1..=b.z2 {
                let a = t.altitude.get(x, z);
                let to = kind.target(a);
                if to == a {
                    continue;
                }
                if apply {
                    self.set_altitude(t, x, z, to);
                } else {
                    self.mark_vertex(t, x, z);
                }
                n += match kind {
                    Kind::Level(_) => (a as i32 - to as i32).unsigned_abs(),
                    _ => 1,
                };
            }
        }
        let per = match kind {
            Kind::Raise => c.raise,
            Kind::Lower => c.lower,
            Kind::Level(_) => c.level,
        };
        per.wrapping_mul(n) >> 2
    }

    /// The shared body of the six operations; `apply` false estimates only.
    fn run(&mut self, t: &mut Terrain, b: Bounds, kind: Kind, apply: bool, c: Costs) -> Outcome {
        let mut cost = 0u32;
        self.reset_changes();
        self.pri.clear();
        self.sec.clear();
        for (x, z) in Self::edge(b) {
            let to = match kind {
                Kind::Level(l) => l,
                _ => kind.target(t.altitude.get(x, z)),
            };
            let mut ok = self.add_pri(t, x, z, to);
            if !apply && self.pri.len() > 1 {
                let (a, p) = (self.pri[self.pri.len() - 1], self.pri[self.pri.len() - 2]);
                if a.steps(&p) != 1 {
                    return Outcome { ok: false, bounds: None, cost };
                }
                ok &= (a.alt as i32 - p.alt as i32).abs() <= SLOPE;
            }
            if !apply && !ok {
                return Outcome { ok: false, bounds: None, cost };
            }
        }
        loop {
            let (grew, up, down) = self.grow_and_fix(t);
            if !grew || up.wrapping_add(down) == 0 {
                break;
            }
            let step = c.lower.wrapping_mul(down).wrapping_add(c.raise.wrapping_mul(up));
            cost = cost.wrapping_add(step >> 2);
            if apply {
                for v in self.pri.clone() {
                    if !v.keep {
                        self.set_altitude(t, v.x as u32, v.z as u32, v.alt);
                    }
                }
            }
        }
        let out = self.contour_bounds(t);
        cost = cost.wrapping_add(self.inside(t, b, kind, apply, c));
        if apply {
            let sea = t.sea_level;
            self.set_water(t, out.x1 + 1, out.z1 + 1, out.x2, out.z2, sea);
        }
        Outcome { ok: true, bounds: Some(out), cost }
    }

    /// `CanRaiseTerrain` (libSimDirt 0x3870C, SIMDIRT.DLL 0x10009CD0): the cost of raising
    /// `b` one step, false when the edge is not a connected run within the slope.
    pub fn can_raise(&mut self, t: &Terrain, b: Bounds, c: Costs) -> Outcome {
        self.run(&mut t.clone(), b, Kind::Raise, false, c)
    }

    /// `CanLowerTerrain` (libSimDirt 0x38D2C, SIMDIRT.DLL 0x1000A210).
    pub fn can_lower(&mut self, t: &Terrain, b: Bounds, c: Costs) -> Outcome {
        self.run(&mut t.clone(), b, Kind::Lower, false, c)
    }

    /// `CanLevelTerrain` (libSimDirt 0x3934C, SIMDIRT.DLL 0x1000A750).
    pub fn can_level(&mut self, t: &Terrain, b: Bounds, level: u8, c: Costs) -> Outcome {
        self.run(&mut t.clone(), b, Kind::Level(level), false, c)
    }

    /// `GetOptimalLevel` (libSimDirt 0x398DC, SIMDIRT.DLL 0x1000AC50): the rounded mean of
    /// the edge's altitudes, and `CanLevelTerrain` to it. `None` for an empty edge.
    pub fn optimal_level(&mut self, t: &Terrain, b: Bounds, c: Costs) -> Option<(u8, Outcome)> {
        let e = Self::edge(b);
        if e.is_empty() {
            return None;
        }
        let sum: u32 = e.iter().map(|&(x, z)| t.altitude.get(x, z) as u32).sum();
        let n = e.len() as u32;
        let level = ((n >> 1) + sum) / n;
        Some((level as u8, self.can_level(t, b, level as u8, c)))
    }

    /// `RaiseTerrain` (libSimDirt 0x3AB28, SIMDIRT.DLL 0x1000BC90): raises `b` one step, then
    /// the land around it as far as the slope needs, and resets the water there to the sea.
    pub fn raise(&mut self, t: &mut Terrain, b: Bounds, c: Costs) -> Outcome {
        self.run(t, b, Kind::Raise, true, c)
    }

    /// `LowerTerrain` (libSimDirt 0x3B3F8, SIMDIRT.DLL 0x1000C130).
    pub fn lower(&mut self, t: &mut Terrain, b: Bounds, c: Costs) -> Outcome {
        self.run(t, b, Kind::Lower, true, c)
    }

    /// `LevelTerrain` (libSimDirt 0x3BCC8, SIMDIRT.DLL 0x1000C5D0).
    pub fn level(&mut self, t: &mut Terrain, b: Bounds, level: u8, c: Costs) -> Outcome {
        self.run(t, b, Kind::Level(level), true, c)
    }
}
