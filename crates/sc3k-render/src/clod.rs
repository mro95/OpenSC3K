//! Dirt clods (`cSC3DirtClodX` and its kinds): one map cell of ground, drawn the way
//! SIMDIRT.DLL draws it. See `docs/render/terrain.md`. `tools/diffcheck/run.py ground` draws
//! clods of every saved terrain with the original and compares the pixels.
//!
//! A clod works in its own coordinates: x from the left corner, y from the anchor corner of
//! its type. `area_from_pt` places it on the target.

use crate::palette::ColorTable;
use sc3k_formats::image::rgb565;
use sc3k_sim::dirt::Terrain;
use sc3k_ui::surface::{from_565, to_565};
use sc3k_ui::Surface;

/// Cell width at each zoom (`cSC3DirtClodFactory::mCellWidth`, SIMDIRT.DLL 0x100252BC).
pub fn cell_width(zoom: u32) -> i32 {
    8 << zoom
}

/// Cell height (`mCellHeight`, 0x10025B88).
pub fn cell_height(zoom: u32) -> i32 {
    4 << zoom
}

/// Screen pixels per altitude step (`mCellAltStep`, 0x10025B64).
pub fn alt_step(zoom: u32) -> i32 {
    1 << zoom
}

/// What a clod draws on: 16-bit pixels.
pub trait Target {
    fn get(&self, x: i32, y: i32) -> u16;
    fn set(&mut self, x: i32, y: i32, v: u16);
}

/// A plain RGB565 buffer.
pub struct Buffer16 {
    pub width: i32,
    pub height: i32,
    pub pixels: Vec<u16>,
}

impl Target for Buffer16 {
    fn get(&self, x: i32, y: i32) -> u16 {
        self.pixels[(y * self.width + x) as usize]
    }

    fn set(&mut self, x: i32, y: i32, v: u16) {
        self.pixels[(y * self.width + x) as usize] = v;
    }
}

impl Target for Surface {
    fn get(&self, x: i32, y: i32) -> u16 {
        to_565(self.pixels[(y * self.width + x) as usize])
    }

    fn set(&mut self, x: i32, y: i32, v: u16) {
        self.pixels[(y * self.width + x) as usize] = from_565(v);
    }
}

/// `cRZRect`: corners (x1, y1) and (x2, y2), the second excluded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
}

/// The 16-bit buffer's packer (`cIGZBuffer` vtable 0x1A0): the low byte of each channel,
/// truncated to 5, 6 and 5 bits.
pub fn pack(r: u32, g: u32, b: u32) -> u16 {
    ((r & 0xF8) << 8 | (g & 0xFC) << 3 | (b & 0xF8) >> 3) as u16
}

/// The buffer's unpacker (vtable 0x1A4).
pub fn unpack(p: u16) -> [u8; 3] {
    rgb565(p)
}

/// The clod types (SIMDIRT.DLL 0x10024258, 19 bytes each): the piece count; for each of up to
/// two pieces six corner slots, the left edge chain then the right one, top to bottom; the
/// chain lengths (left of pieces 0 and 1 at 13 and 14, right at 15 and 16); the anchor
/// corner, at y 0 (17); the lowest corner (18).
pub const TYPES: [[u8; 19]; 55] = [
    [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [2, 0, 3, 0, 1, 3, 0, 2, 1, 3, 2, 3, 0, 2, 3, 3, 2, 0, 3],
    [2, 0, 3, 0, 1, 3, 0, 1, 3, 2, 3, 0, 0, 2, 2, 3, 2, 0, 3],
    [1, 0, 3, 0, 1, 2, 3, 0, 0, 0, 0, 0, 0, 2, 0, 4, 0, 0, 3],
    [1, 0, 3, 0, 1, 2, 0, 0, 0, 0, 0, 0, 0, 2, 0, 3, 0, 0, 3],
    [1, 0, 3, 2, 0, 1, 2, 0, 0, 0, 0, 0, 0, 3, 0, 3, 0, 0, 2],
    [2, 0, 3, 1, 3, 0, 0, 2, 1, 3, 2, 3, 0, 2, 3, 2, 2, 2, 3],
    [2, 0, 3, 1, 3, 0, 0, 1, 3, 2, 3, 0, 0, 2, 2, 2, 2, 0, 3],
    [1, 0, 3, 1, 2, 3, 0, 0, 0, 0, 0, 0, 0, 2, 0, 3, 0, 0, 3],
    [1, 0, 3, 1, 2, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 2, 0, 0, 3],
    [1, 0, 3, 2, 1, 2, 0, 0, 0, 0, 0, 0, 0, 3, 0, 2, 0, 0, 2],
    [1, 2, 1, 0, 3, 2, 3, 0, 0, 0, 0, 0, 0, 4, 0, 2, 0, 2, 3],
    [1, 1, 0, 3, 2, 3, 0, 0, 0, 0, 0, 0, 0, 3, 0, 2, 0, 1, 3],
    [1, 1, 0, 3, 1, 2, 3, 0, 0, 0, 0, 0, 0, 3, 0, 3, 0, 1, 3],
    [1, 1, 0, 3, 1, 2, 0, 0, 0, 0, 0, 0, 0, 3, 0, 2, 0, 1, 3],
    [1, 1, 0, 3, 2, 1, 2, 0, 0, 0, 0, 0, 0, 4, 0, 2, 0, 1, 2],
    [1, 2, 1, 0, 2, 3, 0, 0, 0, 0, 0, 0, 0, 3, 0, 2, 0, 2, 0],
    [1, 1, 0, 2, 3, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 2, 0, 1, 0],
    [1, 1, 0, 1, 2, 3, 0, 0, 0, 0, 0, 0, 0, 2, 0, 3, 0, 1, 0],
    [2, 1, 0, 1, 3, 0, 0, 1, 3, 1, 2, 0, 0, 2, 2, 2, 2, 1, 0],
    [2, 1, 0, 1, 3, 0, 0, 1, 3, 2, 1, 2, 0, 2, 3, 2, 2, 1, 2],
    [1, 2, 1, 0, 2, 3, 0, 0, 0, 0, 0, 0, 0, 3, 0, 3, 0, 2, 0],
    [1, 1, 0, 2, 3, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 3, 0, 1, 0],
    [1, 1, 0, 1, 2, 3, 0, 0, 0, 0, 0, 0, 0, 2, 0, 4, 0, 1, 0],
    [2, 1, 0, 1, 3, 0, 0, 1, 3, 1, 2, 0, 0, 2, 2, 3, 2, 1, 0],
    [2, 1, 0, 1, 3, 0, 0, 1, 3, 2, 1, 2, 0, 2, 3, 3, 2, 1, 0],
    [2, 0, 3, 0, 1, 3, 0, 2, 1, 3, 2, 3, 0, 2, 3, 3, 2, 2, 3],
    [2, 1, 0, 1, 3, 0, 0, 1, 3, 2, 1, 2, 0, 2, 3, 3, 2, 1, 2],
    [2, 0, 3, 0, 1, 3, 0, 2, 1, 3, 2, 3, 0, 2, 3, 3, 2, 0, 3],
    [2, 0, 3, 0, 1, 3, 0, 1, 3, 2, 3, 0, 0, 2, 2, 3, 2, 0, 3],
    [2, 0, 3, 0, 1, 3, 0, 1, 3, 1, 2, 3, 0, 2, 2, 3, 3, 0, 3],
    [2, 0, 3, 0, 1, 3, 0, 1, 3, 1, 2, 0, 0, 2, 2, 3, 2, 0, 3],
    [2, 0, 3, 0, 1, 3, 0, 1, 3, 2, 1, 2, 0, 2, 3, 3, 2, 0, 2],
    [2, 0, 3, 1, 3, 0, 0, 2, 1, 3, 2, 3, 0, 2, 3, 2, 2, 2, 3],
    [2, 0, 3, 1, 3, 0, 0, 1, 3, 2, 3, 0, 0, 2, 2, 2, 2, 0, 3],
    [2, 0, 3, 1, 3, 0, 0, 1, 3, 1, 2, 3, 0, 2, 2, 2, 3, 0, 3],
    [2, 0, 3, 1, 3, 0, 0, 1, 3, 1, 2, 0, 0, 2, 2, 2, 2, 0, 3],
    [2, 0, 3, 1, 3, 0, 0, 1, 3, 2, 1, 2, 0, 2, 3, 2, 2, 0, 2],
    [2, 1, 0, 3, 1, 3, 0, 2, 1, 3, 2, 3, 0, 3, 3, 2, 2, 2, 3],
    [2, 1, 0, 3, 1, 3, 0, 1, 3, 2, 3, 0, 0, 3, 2, 2, 2, 1, 3],
    [2, 1, 0, 3, 1, 3, 0, 1, 3, 1, 2, 3, 0, 3, 2, 2, 3, 1, 3],
    [2, 1, 0, 3, 1, 3, 0, 1, 3, 1, 2, 0, 0, 3, 2, 2, 2, 1, 3],
    [2, 1, 0, 3, 1, 3, 0, 1, 3, 2, 1, 2, 0, 3, 3, 2, 2, 1, 2],
    [2, 1, 0, 1, 3, 0, 0, 2, 1, 3, 2, 3, 0, 2, 3, 2, 2, 2, 0],
    [2, 1, 0, 1, 3, 0, 0, 1, 3, 2, 3, 0, 0, 2, 2, 2, 2, 1, 0],
    [2, 1, 0, 1, 3, 0, 0, 1, 3, 1, 2, 3, 0, 2, 2, 2, 3, 1, 0],
    [2, 1, 0, 1, 3, 0, 0, 1, 3, 1, 2, 0, 0, 2, 2, 2, 2, 1, 0],
    [2, 1, 0, 1, 3, 0, 0, 1, 3, 2, 1, 2, 0, 2, 3, 2, 2, 1, 2],
    [2, 1, 0, 1, 3, 0, 0, 2, 1, 3, 2, 3, 0, 2, 3, 3, 2, 2, 0],
    [2, 1, 0, 1, 3, 0, 0, 1, 3, 2, 3, 0, 0, 2, 2, 3, 2, 1, 0],
    [2, 1, 0, 1, 3, 0, 0, 1, 3, 1, 2, 3, 0, 2, 2, 3, 3, 1, 0],
    [2, 1, 0, 1, 3, 0, 0, 1, 3, 1, 2, 0, 0, 2, 2, 3, 2, 1, 0],
    [2, 1, 0, 1, 3, 0, 0, 1, 3, 2, 1, 2, 0, 2, 3, 3, 2, 1, 0],
    [2, 0, 3, 0, 1, 3, 0, 2, 1, 3, 2, 3, 0, 2, 3, 3, 2, 2, 3],
    [2, 1, 0, 1, 3, 0, 0, 1, 3, 2, 1, 2, 0, 2, 3, 3, 2, 1, 2],
];

/// A clod's four corners in slot order (left, top, right, bottom), x and y.
pub type Points = [[i16; 2]; 4];

/// The classifier (SIMDIRT.DLL 0x10012F94): the type of the shape, and the corners moved so
/// that the type's anchor is at y 0. Type 0, nothing drawn, when the top corner is not
/// above the bottom one. `fold` adds 27.
pub fn classify(p: &mut Points, fold: bool) -> usize {
    let (top, bottom) = (p[1][1], p[3][1]);
    if bottom <= top {
        for c in p.iter_mut() {
            c[1] = 0;
        }
        return 0;
    }
    let class = |y: i16| {
        if y < top {
            0
        } else if y == top {
            1
        } else if y < bottom {
            2
        } else if y == bottom {
            3
        } else {
            4
        }
    };
    let (l, r) = (p[0][1], p[2][1]);
    let mut t = class(l) * 5 + 1 + class(r);
    if t == 1 && r < l {
        t = 26;
    } else if t == 25 && l < r {
        t = 27;
    }
    if fold {
        t += 27;
    }
    let shift = p[TYPES[t][17] as usize][1];
    for c in p.iter_mut() {
        c[1] -= shift;
    }
    t
}

/// The original's float to int: added to 2^52 + 2^31 as a double, whose low word less 2^31 is
/// the value to nearest, ties to even. Infinities and NaNs give `i32::MIN`.
fn round(v: f32) -> i32 {
    let d = v as f64 + 4_503_601_774_854_144.0;
    (d.to_bits() as u32).wrapping_sub(0x8000_0000) as i32
}

/// The darkening of the grid lines, applied to one channel.
fn grid_channel(v: u8, grid: i32) -> u32 {
    let v = v as i32 + grid;
    if grid < 0 {
        v.max(0) as u32
    } else {
        v.min(0xFF) as u32
    }
}

/// One edge chain's running state: x and the colour, and their steps per row.
#[derive(Clone, Copy, Default)]
struct Edge {
    x: f32,
    dx: f32,
    c: [f32; 3],
    dc: [f32; 3],
}

impl Edge {
    /// The edge from corner `a` to corner `b`. The reciprocal of the height stays a double;
    /// the steps are rounded to float.
    fn new(p: &Points, cols: &[[u8; 3]; 4], a: usize, b: usize) -> Edge {
        let inv = 1.0 / (p[b][1] as i32 - p[a][1] as i32) as f64;
        let step = |d: i32| (d as f64 * inv) as f32;
        Edge {
            x: p[a][0] as f32,
            dx: step(p[b][0] as i32 - p[a][0] as i32),
            c: cols[a].map(|v| v as f32),
            dc: [0, 1, 2].map(|k| step(cols[b][k] as i32 - cols[a][k] as i32)),
        }
    }

    fn advance(&mut self) {
        self.x += self.dx;
        for k in 0..3 {
            self.c[k] += self.dc[k];
        }
    }
}

/// How the rasterizer adds noise: none, one byte for all channels (water) or three (land).
#[derive(Clone, Copy)]
pub enum Bump<'a> {
    None,
    One(&'a [u8; 0x400], u32),
    Three(&'a [u8; 0x400], u32),
}

/// How the rasterizer turns a pixel's colour into the pixel.
#[derive(Clone, Copy)]
pub enum Shade<'a> {
    /// The colour, plus noise.
    Plain,
    /// The shore (libSimDirt 0x47088, SIMDIRT.DLL 0x100148D7), by the squared distance `d`
    /// of the rounded colour from the grey that marks wet corners:
    /// - `d ≥ 0x400`: the colour plus noise;
    /// - `0x100 < d < 0x400`: `(water · (0x400 − d) + colour · (d − 0x100)) · f32(1 / 0x300)`,
    ///   truncated, plus noise;
    /// - `d ≤ 0x100`: the water colour plus one byte of `water_bump` at the next index.
    Shore {
        grey: [u8; 3],
        water: [u8; 3],
        water_bump: &'a [u8; 0x400],
    },
}

/// The shore's choice for a rounded colour: `None` keeps the colour, `Some((v, true))` is the
/// water colour.
fn shore_mix(c: [u8; 3], grey: [u8; 3], water: [u8; 3]) -> Option<([u8; 3], bool)> {
    const LAND: i32 = 0x400;
    const WATER: i32 = 0x100;
    let inv = (1.0f64 / (LAND - WATER) as f64) as f32;
    let d: i32 = (0..3).map(|k| (c[k] as i32 - grey[k] as i32).pow(2)).sum();
    if d >= LAND {
        None
    } else if d > WATER {
        let mix = |k: usize| {
            let n = water[k] as i32 * (LAND - d) + (d - WATER) * c[k] as i32;
            (n as f64 * inv as f64).trunc() as i32 as u8
        };
        Some(([0, 1, 2].map(mix), false))
    } else {
        Some((water, true))
    }
}

/// The Gouraud rasterizer (libSimDirt 0x463D0, SIMDIRT.DLL 0x10013556). `area` places the
/// clod's origin; `clip` limits the pixels drawn (the area when none). Each piece of the type
/// is filled row by row between its left and right chains, from the rounded left edge to the
/// rounded right edge. `grid` darkens (or lightens) the first pixels of each row along the
/// left edges and the row through the top corner.
#[allow(clippy::too_many_arguments)]
pub fn rasterize(
    target: &mut impl Target,
    area: Area,
    clip: Option<Area>,
    ty: usize,
    p: &Points,
    cols: &[[u8; 3]; 4],
    bump: Bump,
    shade: Shade,
    grid: i32,
) {
    let (ox, oy) = (area.x1, area.y1);
    let c = clip.unwrap_or(area);
    let (cl, ct, cr, cb) = (c.x1 - ox, c.y1 - oy, c.x2 - ox, c.y2 - oy);
    let row = &TYPES[ty];
    let mut put = |x: i32, y: i32, v: u16| target.set(ox + x, oy + y, v);
    let (table, start, three) = match bump {
        Bump::None => (None, 0, false),
        Bump::One(t, s) => (Some(t), s, false),
        Bump::Three(t, s) => (Some(t), s, true),
    };
    for piece in 0..row[0] as usize {
        let chains = &row[1 + piece * 6..7 + piece * 6];
        let (nl, nr) = (row[13 + piece] as usize, row[15 + piece] as usize);
        let (left, right) = (&chains[..nl], &chains[nl..nl + nr]);
        let bottom = (p[left[nl - 1] as usize][1] as i32).min(cb);
        let top = p[left[0] as usize][1] as i32;
        let (mut lnext, mut rnext) = (top, top);
        let (mut li, mut ri) = (0, 0);
        let (mut le, mut re) = (Edge::default(), Edge::default());
        let (mut grid_on, mut grid_w, mut grid_prev) = (false, 0, 0);
        let mut bump_row = (top as u32).wrapping_mul(0x8F).wrapping_add(start);
        let mut y = top;
        while y < bottom {
            let mut b = (bump_row & 0x3FF) as usize;
            if y >= lnext && li + 1 < nl {
                let a = left[li] as usize;
                li += 1;
                let n = left[li] as usize;
                lnext = p[n][1] as i32;
                le = Edge::new(p, cols, a, n);
                if grid != 0 {
                    grid_on = piece == 0 || a != 1;
                    if grid_on {
                        grid_w = round(le.dx).wrapping_abs().wrapping_add(1);
                    }
                }
            }
            if y >= rnext && ri + 1 < nr {
                let a = right[ri] as usize;
                ri += 1;
                let n = right[ri] as usize;
                rnext = p[n][1] as i32;
                re = Edge::new(p, cols, a, n);
            }
            let w = (re.x as f64 - le.x as f64) + 1.0;
            if y >= ct && w > 0.0 {
                let end = round(re.x).min(cr);
                let inv = 1.0 / w;
                let mut col = le.c;
                let d = [0, 1, 2].map(|k| ((re.c[k] as f64 - le.c[k] as f64) * inv) as f32);
                let mut x = round(le.x);
                let step = |col: &mut [f32; 3]| {
                    for k in 0..3 {
                        col[k] += d[k];
                    }
                };
                if grid_on {
                    // The land rasterizer covers the wider of this row's and the last row's
                    // grid width, the shore one this row's only.
                    let n = match shade {
                        Shade::Plain => grid_prev.max(grid_w),
                        Shade::Shore { .. } => grid_w,
                    };
                    grid_prev = grid_w;
                    let mut count = 0;
                    while count < n && x < end {
                        let mut v = col.map(|v| round(v) as u8);
                        if let Shade::Shore { grey, water, .. } = shade {
                            v = shore_mix(v, grey, water).map_or(v, |(m, _)| m);
                        }
                        let v = v.map(|v| grid_channel(v, grid));
                        if x >= cl {
                            put(x, y, pack(v[0], v[1], v[2]));
                        }
                        step(&mut col);
                        if table.is_some() {
                            b = (b + if three { 3 } else { 1 }) & 0x3FF;
                        }
                        count += 1;
                        x += 1;
                    }
                }
                while x < end {
                    let n = match table {
                        None => [0; 3],
                        Some(t) if three => {
                            let n = [t[b], t[(b + 1) & 0x3FF], t[(b + 2) & 0x3FF]];
                            b = (b + 3) & 0x3FF;
                            n
                        }
                        Some(t) => {
                            let n = [t[b]; 3];
                            b = (b + 1) & 0x3FF;
                            n
                        }
                    };
                    let base = col.map(|v| round(v) as u8);
                    let v = match shade {
                        Shade::Shore {
                            grey,
                            water,
                            water_bump,
                        } => match shore_mix(base, grey, water) {
                            Some((w, true)) => {
                                let wb = if table.is_some() { water_bump[b] } else { 0 };
                                w.map(|v| v.wrapping_add(wb))
                            }
                            Some((m, false)) => [0, 1, 2].map(|k| m[k].wrapping_add(n[k])),
                            None => [0, 1, 2].map(|k| base[k].wrapping_add(n[k])),
                        },
                        Shade::Plain => [0, 1, 2].map(|k| base[k].wrapping_add(n[k])),
                    };
                    if x >= cl {
                        put(x, y, pack(v[0] as u32, v[1] as u32, v[2] as u32));
                    }
                    step(&mut col);
                    x += 1;
                }
            }
            le.advance();
            re.advance();
            bump_row = bump_row.wrapping_add(0x8F);
            y += 1;
        }
    }
    if grid != 0 {
        let y = p[1][1] as i32;
        if ct <= y && y < cb {
            let xs = if p[0][1] == p[1][1] { p[0][0] } else { p[1][0] } as i32;
            let xe = if p[2][1] == p[1][1] { p[2][0] } else { p[1][0] } as i32;
            for x in xs.max(cl)..xe.min(cr) {
                let v = unpack(target.get(ox + x, oy + y)).map(|v| grid_channel(v, grid));
                target.set(ox + x, oy + y, pack(v[0], v[1], v[2]));
            }
        }
    }
}

/// The haze below zoom 3 (SIMDIRT.DLL 0x1001318A, tables 0x10024670 and 0x1002467C): each
/// channel loses `round(c / 10)` per zoom step below 3 and gains a blue-grey offset, wrapping.
pub fn haze(c: [u8; 3], zoom: u32) -> [u8; 3] {
    const OFFSET: [[u8; 3]; 3] = [[0x31, 0x33, 0x35], [0x21, 0x22, 0x23], [0x10, 0x11, 0x12]];
    if zoom >= 3 {
        return c;
    }
    let steps = (3 - zoom) as u8;
    [0, 1, 2].map(|k| {
        let tenth = ((c[k] as u32 * 10 + 50) / 100) as u8;
        c[k].wrapping_sub(tenth.wrapping_mul(steps))
            .wrapping_add(OFFSET[zoom as usize][k])
    })
}

/// Which clod `cSC3DirtBag::GetDirtClod` makes for a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Land,
    Shore,
    Water,
}

/// `cSC3DirtBag::GetDirtClod` (libSimDirt 0x40AE4, SIMDIRT.DLL 0x10005B10): the clod kind by
/// how many corners are at or below the water (none land, all four water, else shore, and
/// land when forced), and the altitude the sprite cell map gets: the dirt at (x, z) for land,
/// the water for water, the higher of the two for shore.
pub fn dirt_clod(t: &Terrain, x: u32, z: u32, force_land: bool) -> (Kind, u8) {
    let (dirt, water) = (t.altitude.get(x, z), t.water.get(x, z));
    match sc3k_sim::dirt_bag::cell_water_vert_count(t, x, z) {
        _ if force_land => (Kind::Land, dirt),
        0 => (Kind::Land, dirt),
        4 => (Kind::Water, water),
        _ => (Kind::Shore, dirt.max(water)),
    }
}

/// The palettes the clods read: the landscape (factory palette 4) and the water by depth
/// (palette 6, `palwater.bmp`).
pub struct Palettes<'a> {
    pub land: &'a ColorTable,
    pub water: &'a ColorTable,
}

/// The two noise tables (`GenerateBumpMaps`).
pub struct Bumps<'a> {
    pub land: &'a [u8; 0x400],
    pub water: &'a [u8; 0x400],
}

/// The grey that marks a wet shore corner (`cSC3DirtClodShore::getColor`).
const SHORE_WET: [u8; 3] = [0xA0, 0xA0, 0xA0];

/// A land, shore or water clod of cell (x, z) at a zoom and rotation, as its `pregetPoints`
/// leaves it.
pub struct Clod {
    pub kind: Kind,
    pub ty: usize,
    pub points: Points,
}

/// The vertices of cell (x, z) and the slots they fill for rotation `rot`: (x, z), (x, z + 1),
/// (x + 1, z + 1), (x + 1, z).
fn corners(x: u32, z: u32, rot: u32) -> [((u32, u32), usize); 4] {
    let slot = |k: u32| (k.wrapping_sub(rot) & 3) as usize;
    [
        ((x, z), slot(1)),
        ((x, z + 1), slot(2)),
        ((x + 1, z + 1), slot(3)),
        ((x + 1, z), slot(0)),
    ]
}

impl Clod {
    /// `pregetPoints`: the corners at their altitudes, the top and bottom ones half a cell up
    /// and down, then classified. Folded when the top and bottom are level and the sides are
    /// not; for the shore, when the top and bottom are equally wet and the sides are not.
    /// - land (libSimDirt 0x48EA8, SIMDIRT.DLL 0x10012E44): the dirt;
    /// - water (0x49730, SIMDIRT.DLL 0x10013E5B): the water surface;
    /// - shore (0x4A020, SIMDIRT.DLL 0x100142EB): the higher of the two, a corner wet when the
    ///   water is not below the dirt.
    pub fn new(t: &Terrain, kind: Kind, x: u32, z: u32, zoom: u32, rot: u32) -> Clod {
        let half = cell_width(zoom) as i16 / 2;
        let mut p: Points = [[0, 0], [half, 0], [cell_width(zoom) as i16, 0], [half, 0]];
        let mut wet = [false; 4];
        for ((vx, vz), slot) in corners(x, z, rot) {
            let (dirt, water) = (t.altitude.get(vx, vz), t.water.get(vx, vz));
            let alt = match kind {
                Kind::Land => dirt,
                Kind::Water => water,
                Kind::Shore => {
                    wet[slot] = water >= dirt;
                    dirt.max(water)
                }
            };
            p[slot][1] = (alt as i16).wrapping_mul(-alt_step(zoom) as i16);
        }
        let fold = match kind {
            Kind::Shore => wet[1] == wet[3] && wet[0] != wet[2],
            _ => p[1][1] == p[3][1] && p[0][1] != p[2][1],
        };
        let h = (cell_height(zoom) / 2) as i16;
        p[1][1] -= h;
        p[3][1] += h;
        let ty = classify(&mut p, fold);
        Clod {
            kind,
            ty,
            points: p,
        }
    }

    /// `GetAllSpans` (libSimDirt 0x48C70, SIMDIRT.DLL 0x10012B91) and `GetAreaFromPt`
    /// (0x4E1B4, SIMDIRT.DLL 0x10012C7D): the clod's area when the sprite cell map puts it
    /// at (px, py). The corner of vertex (x, z) is at py, moved half a cell for rotations 0
    /// and 2.
    pub fn area_from_pt(&self, zoom: u32, rot: u32, px: i32, py: i32) -> Area {
        let (mut top, mut height) = (0, 0);
        if self.ty != 0 {
            let h = cell_height(zoom) / 2;
            top = self.points[(1u32.wrapping_sub(rot) & 3) as usize][1] as i32;
            height = self.points[TYPES[self.ty][18] as usize][1] as i32 - top;
            match rot {
                0 => (top, height) = (top + h, height - h),
                2 => (top, height) = (top - h, height + h),
                _ => {}
            }
        }
        Area {
            x1: px,
            y1: py - top,
            x2: cell_width(zoom) + px,
            y2: height + py,
        }
    }

    /// `getColor` for a vertex the factory has not cached, hazed below zoom 3:
    /// - land (libSimDirt 0x49060, SIMDIRT.DLL 0x1001309E): row `0x100 − (dirt − water)` of
    ///   the landscape palette, entry `light >> 1`;
    /// - water (0x498E8, SIMDIRT.DLL 0x10013FAB): the water palette at the depth, 0 when the
    ///   dirt is above the water. Without pollution;
    /// - shore (0x4A318, SIMDIRT.DLL 0x100144FF): as land where the water is below the dirt,
    ///   else the wet grey.
    pub fn colour(
        &self,
        t: &Terrain,
        light: u8,
        pal: &Palettes,
        (x, z): (u32, u32),
        zoom: u32,
    ) -> [u8; 3] {
        let (dirt, water) = (t.altitude.get(x, z), t.water.get(x, z));
        let land = || {
            let row = 0x100u16
                .wrapping_add(water as u16)
                .wrapping_sub(dirt as u16);
            pal.land.get(row as usize, (light >> 1) as usize)
        };
        haze(
            match self.kind {
                Kind::Land => land(),
                Kind::Water => pal
                    .water
                    .get((water as i32 - dirt as i32).max(0) as usize, 0),
                Kind::Shore if water < dirt => land(),
                Kind::Shore => SHORE_WET,
            },
            zoom,
        )
    }

    /// `Draw` after `cSC3DirtClodX::Draw` (libSimDirt 0x4E224, SIMDIRT.DLL 0x10012CB7) placed
    /// the clod at (px, py): the corner colours, then the rasterizer. The grid darkens by 16,
    /// by 8 at zooms 0 and 1.
    /// - land (libSimDirt 0x49258, SIMDIRT.DLL 0x10013241): three land noise bytes a pixel;
    /// - water (0x49C44, SIMDIRT.DLL 0x100140D8): one water noise byte a pixel;
    /// - shore (0x4A5B8, SIMDIRT.DLL 0x1001461D): the shore shading against the hazed wet
    ///   grey and the hazed shallowest water colour, without pollution.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        target: &mut impl Target,
        t: &Terrain,
        light: &sc3k_sim::cellmap::CellMap<u8>,
        pal: &Palettes,
        bumps: &Bumps,
        (x, z): (u32, u32),
        (zoom, rot): (u32, u32),
        (px, py): (i32, i32),
        clip: Option<Area>,
        grid: bool,
    ) {
        if self.ty == 0 {
            return;
        }
        let mut cols = [[0u8; 3]; 4];
        for ((vx, vz), slot) in corners(x, z, rot) {
            cols[slot] = self.colour(t, light.get(vx, vz), pal, (vx, vz), zoom);
        }
        let grid = match (grid, zoom) {
            (false, _) => 0,
            (true, 0 | 1) => -8,
            _ => -16,
        };
        let start = x.wrapping_mul(0x6B9).wrapping_add(z.wrapping_mul(0x757)) & 0x3FF;
        let (bump, shade) = match self.kind {
            Kind::Land => (Bump::Three(bumps.land, start), Shade::Plain),
            Kind::Water => (Bump::One(bumps.water, start), Shade::Plain),
            Kind::Shore => (
                Bump::Three(bumps.land, start),
                Shade::Shore {
                    grey: haze(SHORE_WET, zoom),
                    water: haze(pal.water.get(0, 0), zoom),
                    water_bump: bumps.water,
                },
            ),
        };
        let area = self.area_from_pt(zoom, rot, px, py);
        rasterize(
            target,
            area,
            clip,
            self.ty,
            &self.points,
            &cols,
            bump,
            shade,
            grid,
        );
    }
}

/// The edge classifier (SIMDIRT.DLL 0x10016389): types 3 to 15 by how the top corners (0, 1)
/// and the bottom corners (3, 2) compare. The corners are not moved.
pub fn classify_edge(p: &Points) -> usize {
    let c = if p[0][1] < p[1][1] {
        3
    } else if p[0][1] == p[1][1] {
        8
    } else {
        13
    };
    if p[3][1] < p[2][1] {
        c + 2
    } else if p[3][1] == p[2][1] {
        c + 1
    } else {
        c
    }
}

/// The edge rasterizer (libSimDirt 0x48238, SIMDIRT.DLL 0x10015F44): one value per corner,
/// a row of `ramp`, interpolated like the colours of [`rasterize`]. Without noise the value
/// steps along a row in double precision, with noise in float. Noise is one byte of `bump`
/// per pixel, from `start` on through every row: a non-zero byte adds the packed (8, 8, 8),
/// 0x0841, to the 16-bit pixel. A value outside the ramp reads 0 here; the original reads
/// past it.
#[allow(clippy::too_many_arguments)]
pub fn rasterize_edge(
    target: &mut impl Target,
    area: Area,
    clip: Option<Area>,
    ty: usize,
    p: &Points,
    idx: [i32; 4],
    ramp: &[u16; 0x200],
    bump: Option<(&[u8; 0x400], u32)>,
) {
    let (ox, oy) = (area.x1, area.y1);
    let c = clip.unwrap_or(area);
    let (cl, ct, cr, cb) = (c.x1 - ox, c.y1 - oy, c.x2 - ox, c.y2 - oy);
    let at = |i: u32| ramp.get((i & 0xFFFF) as usize).copied().unwrap_or(0);
    let mut b = bump.map_or(0, |(_, s)| s as usize);
    let row = &TYPES[ty];
    for piece in 0..row[0] as usize {
        let chains = &row[1 + piece * 6..7 + piece * 6];
        let (nl, nr) = (row[13 + piece] as usize, row[15 + piece] as usize);
        let (left, right) = (&chains[..nl], &chains[nl..nl + nr]);
        let bottom = (p[left[nl - 1] as usize][1] as i32).min(cb);
        let top = p[left[0] as usize][1] as i32;
        let (mut lnext, mut rnext) = (top, top);
        let (mut li, mut ri) = (0, 0);
        // x, its step, the value, its step.
        let mut le = [0f32; 4];
        let mut re = [0f32; 4];
        let edge = |a: usize, n: usize| {
            let inv = 1.0 / (p[n][1] as i32 - p[a][1] as i32) as f64;
            let step = |d: i32| (d as f64 * inv) as f32;
            [
                p[a][0] as f32,
                step(p[n][0] as i32 - p[a][0] as i32),
                idx[a] as f32,
                step(idx[n] - idx[a]),
            ]
        };
        for y in top..bottom {
            if y >= lnext && li + 1 < nl {
                let a = left[li] as usize;
                li += 1;
                let n = left[li] as usize;
                lnext = p[n][1] as i32;
                le = edge(a, n);
            }
            if y >= rnext && ri + 1 < nr {
                let a = right[ri] as usize;
                ri += 1;
                let n = right[ri] as usize;
                rnext = p[n][1] as i32;
                re = edge(a, n);
            }
            let w = (re[0] as f64 - le[0] as f64) + 1.0;
            if y >= ct && w > 0.0 {
                let end = round(re[0]).min(cr);
                let step = ((re[2] as f64 - le[2] as f64) * (1.0 / w)) as f32;
                let mut x = round(le[0]);
                match bump {
                    None => {
                        let mut v = le[2] as f64;
                        while x < end {
                            if x >= cl {
                                let i = (v + 4_503_601_774_854_144.0).to_bits() as u32;
                                target.set(ox + x, oy + y, at(i));
                            }
                            v += step as f64;
                            x += 1;
                        }
                    }
                    Some((table, _)) => {
                        let mut v = le[2];
                        while x < end {
                            let noise = if table[b] != 0 { pack(8, 8, 8) } else { 0 };
                            b = (b + 1) & 0x3FF;
                            if x >= cl {
                                target.set(ox + x, oy + y, at(round(v) as u32).wrapping_add(noise));
                            }
                            v += step;
                            x += 1;
                        }
                    }
                }
            }
            le[0] += le[1];
            re[0] += re[1];
            le[2] += le[3];
            re[2] += re[3];
        }
    }
}

/// The two map-edge colour ramps for a zoom (SIMDIRT.DLL 0x10024AA4 and 0x10024EA4, built by
/// `cSC3DirtClodEdge::Draw`): entry 0 of each of the first 512 rows of `paledgel.bmp`
/// (factory palette 7) and `paledged.bmp` (palette 8), hazed below zoom 3.
pub fn edge_ramps(light: &ColorTable, dark: &ColorTable, zoom: u32) -> [[u16; 0x200]; 2] {
    [light, dark].map(|table| {
        let mut ramp = [0u16; 0x200];
        for (i, v) in ramp.iter_mut().enumerate() {
            let [r, g, b] = table.get(i, 0);
            let c = if zoom < 3 {
                haze(unpack(pack(r as u32, g as u32, b as u32)), zoom)
            } else {
                [r, g, b]
            };
            *v = pack(c[0] as u32, c[1] as u32, c[2] as u32);
        }
        ramp
    })
}

/// A map-edge clod (`cSC3DirtClodEdge`): the skirt under one cell side along the border.
/// `side` (flag 0x80) picks which side of the cell and which ramp; `dry` (flag 0x40) ignores
/// the water and ends the soil at the surface.
pub struct EdgeClod {
    pub ty: usize,
    /// Top left, top right, bottom right, bottom left; not moved to the anchor.
    pub points: Points,
    /// Where the soil starts under each top corner: the dirt surface.
    pub soil: [i16; 2],
    side: bool,
    dry: bool,
}

/// The two vertices of an edge clod's side of cell (x, z), left then right on screen.
fn edge_vertices(x: u32, z: u32, rot: u32, side: bool) -> ((u32, u32), (u32, u32)) {
    match (rot + side as u32) & 3 {
        0 => ((x + 1, z + 1), (x, z + 1)),
        1 => ((x + 1, z), (x + 1, z + 1)),
        2 => ((x, z), (x + 1, z)),
        _ => ((x, z + 1), (x, z)),
    }
}

impl EdgeClod {
    /// `cSC3DirtClodEdge::pregetPoints` (libSimDirt 0x4BBB4, SIMDIRT.DLL 0x100163C7).
    pub fn new(
        t: &Terrain,
        x: u32,
        z: u32,
        side: bool,
        dry: bool,
        zoom: u32,
        rot: u32,
    ) -> EdgeClod {
        let half = cell_width(zoom) as i16 / 2;
        let mut x0 = match rot {
            1 => 0,
            3 => -2 * half,
            _ => -half,
        };
        if !side {
            x0 += half;
        }
        let x1 = x0 + half;
        let step = alt_step(zoom) as i16;
        let (a, b) = edge_vertices(x, z, rot, side);
        let top = |(vx, vz): (u32, u32)| {
            let (dirt, water) = (t.altitude.get(vx, vz), t.water.get(vx, vz));
            let soil = (dirt as i16).wrapping_mul(-step);
            if !dry && dirt < water {
                ((water as i16).wrapping_mul(-step), soil)
            } else {
                (soil, soil)
            }
        };
        let ((mut y0, mut s0), (mut y1, mut s1)) = (top(a), top(b));
        let h = (cell_height(zoom) / 2) as i16;
        let mut y3 = match rot {
            0 => 2 * h,
            2 => -2 * h,
            _ => 0,
        };
        s0 += y3;
        y1 += y3;
        y0 += y3;
        s1 += y3;
        let mut y2 = y3;
        if !side {
            y0 += h;
            y3 += h;
            s0 += h;
        } else {
            y1 += h;
            y2 = y3 + h;
            s1 += h;
        }
        if !dry {
            y2 += step * 4;
            y3 += step * 4;
        }
        let points = [[x0, y0], [x1, y1], [x1, y2], [x0, y3]];
        EdgeClod {
            ty: classify_edge(&points),
            points,
            soil: [s0, s1],
            side,
            dry,
        }
    }

    /// `cSC3DirtClodEdge::GetAllSpans` (SIMDIRT.DLL 0x1001598F) and `GetAreaFromPt`
    /// (0x10012C7D) at (px, py).
    pub fn area_from_pt(&self, zoom: u32, rot: u32, px: i32, py: i32) -> Area {
        let p = &self.points;
        let h = cell_height(zoom) / 2;
        let mut top = -(p[TYPES[self.ty][17] as usize][1] as i32);
        let mut bottom = p[TYPES[self.ty][18] as usize][1] as i32;
        match rot {
            0 => (top, bottom) = (top + h, bottom - h),
            2 => (top, bottom) = (top - h, bottom + h),
            _ => {}
        }
        Area {
            x1: px + p[0][0] as i32,
            y1: py - top,
            x2: p[1][0] as i32 + px,
            y2: bottom + py,
        }
    }

    /// `cSC3DirtClodEdge::Draw` (libSimDirt 0x4B360, SIMDIRT.DLL 0x10015ACF) after
    /// `cSC3DirtClodX::Draw` placed it at (px, py): the water between its surface and the
    /// dirt, by depth, then the soil from the dirt down, values `0x100` (`0x104` when dry) at
    /// the top to `0x104 + dirt` at the bottom, with water noise. `ramps` from [`edge_ramps`]
    /// for this zoom.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        target: &mut impl Target,
        t: &Terrain,
        ramps: &[[u16; 0x200]; 2],
        water_bump: &[u8; 0x400],
        (x, z): (u32, u32),
        (zoom, rot): (u32, u32),
        (px, py): (i32, i32),
        clip: Option<Area>,
    ) {
        let area = self.area_from_pt(zoom, rot, px, py);
        let ramp = &ramps[(if self.side { rot } else { !rot } & 1) as usize];
        let (a, b) = edge_vertices(x, z, rot, self.side);
        let (da, wa) = (
            t.altitude.get(a.0, a.1) as i32,
            t.water.get(a.0, a.1) as i32,
        );
        let (db, wb) = (
            t.altitude.get(b.0, b.1) as i32,
            t.water.get(b.0, b.1) as i32,
        );
        let p = &self.points;
        let [s0, s1] = self.soil;
        let anchor = p[TYPES[self.ty][17] as usize][1];
        let (x0, w) = (p[0][0], p[1][0] - p[0][0]);
        let rel = |q: [[i16; 2]; 4]| q.map(|[qx, qy]| [qx, qy - anchor]);
        if !self.dry && (p[0][1] < s0 || p[1][1] < s1) {
            let q = rel([[x0, p[0][1]], [x0 + w, p[1][1]], [x0 + w, s1], [x0, s0]])
                .map(|[qx, qy]| [qx - x0, qy]);
            let idx = [
                0,
                0,
                if p[1][1] < s1 { wb - db } else { 0 },
                if p[0][1] < s0 { wa - da } else { 0 },
            ];
            rasterize_edge(target, area, clip, classify_edge(&q), &q, idx, ramp, None);
        }
        if s0 < p[3][1] || s1 < p[2][1] {
            let q = rel([[x0, s0], [x0 + w, s1], [x0 + w, p[2][1]], [x0, p[3][1]]])
                .map(|[qx, qy]| [qx - x0, qy]);
            let first = if self.dry { 0x104 } else { 0x100 };
            let idx = [
                first,
                first,
                (db + 0x104).min(0x1FF),
                (da + 0x104).min(0x1FF),
            ];
            let start = x.wrapping_mul(0x6B9).wrapping_add(z.wrapping_mul(0x757)) & 0x3FF;
            rasterize_edge(
                target,
                area,
                clip,
                classify_edge(&q),
                &q,
                idx,
                ramp,
                Some((water_bump, start)),
            );
        }
    }
}
