//! Land clods (`cSC3DirtClodLand`): each cell drawn as one or two Gouraud-shaded polygons.
//! See `docs/render/terrain.md`.

use crate::light::vertex_light;
use crate::palette::{ColorTable, DirtPalettes};
use sc3k_sim::dirt::Terrain;
use sc3k_sim::rng::Random;
use sc3k_ui::surface::{quantize, Rect};
use sc3k_ui::Surface;

/// Closest zoom level. Zooms run 0..=4.
pub const MAX_ZOOM: u32 = 4;
/// Length of the bump-noise tables (`GenerateBumpMaps`).
const BUMP_LEN: usize = 0x400;

/// Camera: zoom, rotation and where draw-grid vertex (0, 0) at altitude 0 lands on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct View {
    pub zoom: u32,
    /// 0..=3.
    pub rotation: u32,
    pub origin_x: i32,
    pub origin_y: i32,
}

impl View {
    /// Cell width `8 << zoom` (`cSC3DirtClodFactory::mCellWidth`).
    pub fn cell_width(&self) -> i32 {
        8 << self.zoom
    }

    /// Cell height `4 << zoom`.
    pub fn cell_height(&self) -> i32 {
        4 << self.zoom
    }

    /// Screen pixels per altitude unit, `1 << zoom`.
    pub fn altitude_step(&self) -> i32 {
        1 << self.zoom
    }

    /// The view at `zoom` and `rotation` that puts the middle of the map, at `altitude`, in
    /// the middle of a `width × height` screen.
    pub fn centred(size: u32, altitude: u8, zoom: u32, rotation: u32, width: i32, height: i32) -> View {
        let mut v = View { zoom, rotation, origin_x: 0, origin_y: 0 };
        let mid = size as i32 / 2;
        let (x, y) = v.project(mid, mid, altitude);
        v.origin_x = width / 2 - x;
        v.origin_y = height / 2 - y;
        v
    }

    /// Screen position of draw-grid vertex (i, j) at altitude `a`.
    pub fn project(&self, i: i32, j: i32, a: u8) -> (i32, i32) {
        let half_w = self.cell_width() / 2;
        let half_h = self.cell_height() / 2;
        (
            self.origin_x + (j - i) * half_w,
            self.origin_y + (i + j) * half_h - a as i32 * self.altitude_step(),
        )
    }
}

/// The draw-grid vertex (i, j) of a map with `size` cells per side, as a map vertex. The
/// inverse of `ActualGridToDrawGrid` (libSimSpr 0x99990), applied to vertices.
fn map_vertex(size: u32, rotation: u32, i: u32, j: u32) -> (u32, u32) {
    match rotation & 3 {
        0 => (i, j),
        1 => (j, size - i),
        2 => (size - i, size - j),
        _ => (size - j, i),
    }
}

/// The map cell drawn at draw-grid cell (i, j): the inverse of `ActualGridToDrawGrid`.
pub(crate) fn map_cell(size: u32, rotation: u32, i: u32, j: u32) -> (u32, u32) {
    match rotation & 3 {
        0 => (i, j),
        1 => (j, size - 1 - i),
        2 => (size - 1 - i, size - 1 - j),
        _ => (size - 1 - j, i),
    }
}

/// What the city view draws the ground from: the terrain, its vertex colours, the edge and
/// water palettes and the bump noise.
pub struct TerrainScene {
    terrain: Terrain,
    /// Land colour per vertex, `x · vertices + y`. `cSC3DirtClodLand::getColor` (0x49060)
    /// caches the same per vertex.
    land: Vec<[u8; 3]>,
    dirt: DirtPalettes,
    land_bump: [u8; BUMP_LEN],
    water_bump: [u8; BUMP_LEN],
}

/// Grey that marks a wet shore corner (`cSC3DirtClodShore::getColor`, 0x4A318).
const SHORE_WET: [u8; 3] = [0xA0, 0xA0, 0xA0];
/// Squared colour distances from `SHORE_WET` at which a shore pixel turns from water to land
/// (`Draw__C17cSC3DirtClodShore` passes them to the rasterizer at 0x47088).
const SHORE_WATER: i32 = 0x100;
const SHORE_LAND: i32 = 0x400;

impl TerrainScene {
    /// `bump_seed` replaces the original's clock seed for the bump noise.
    pub fn new(terrain: Terrain, land: &ColorTable, dirt: &DirtPalettes, bump_seed: u32) -> TerrainScene {
        let light = vertex_light(&terrain);
        let v = terrain.vertices();
        let mut colours = Vec::with_capacity((v * v) as usize);
        for x in 0..v {
            for y in 0..v {
                colours.push(land_colour(&terrain, land, light.get(x, y), x, y));
            }
        }
        let (land_bump, water_bump) = bump_maps(bump_seed);
        TerrainScene { terrain, land: colours, dirt: dirt.clone(), land_bump, water_bump }
    }

    pub fn terrain(&self) -> &Terrain {
        &self.terrain
    }

    /// Draw the cells back to front (rows of increasing draw-grid `i + j`; the original's
    /// order is not traced), then the skirts along the two front edges of the map.
    pub fn draw(&self, screen: &mut Surface, view: &View) {
        self.draw_with(screen, view, |_, _, _| {});
    }

    /// [`draw`](Self::draw), calling `on_cell(screen, draw cell, map cell)` right after each
    /// cell's clod, so that what stands on a cell is painted in the same back-to-front order
    /// (the cell map keeps a cell's terrain and occupant sprites in one record).
    pub fn draw_with(
        &self,
        screen: &mut Surface,
        view: &View,
        mut on_cell: impl FnMut(&mut Surface, (u32, u32), (u32, u32)),
    ) {
        let size = self.terrain.size;
        let clip = screen.area();
        for diagonal in 0..2 * size - 1 {
            let first = diagonal.saturating_sub(size - 1);
            let last = diagonal.min(size - 1);
            for i in first..=last {
                let j = diagonal - i;
                self.draw_cell(screen, clip, view, i, j);
                on_cell(screen, (i, j), map_cell(size, view.rotation, i, j));
            }
        }
        for k in 0..size {
            // Left front edge: draw-vertex row i = size. Right front edge: column j = size.
            self.draw_edge(screen, clip, view, (size, k), (size, k + 1), &self.dirt.edge_light);
            self.draw_edge(screen, clip, view, (k + 1, size), (k, size), &self.dirt.edge_dark);
        }
    }

    fn vertex(&self, view: &View, i: u32, j: u32) -> (u32, u32) {
        map_vertex(self.terrain.size, view.rotation, i, j)
    }

    /// One clod. `cSC3DirtBag::GetDirtClod` (0x40AE4) picks the kind by how many corners are
    /// at or below the water: none land, all four water, otherwise shore.
    fn draw_cell(&self, screen: &mut Surface, clip: Rect, view: &View, i: u32, j: u32) {
        let t = &self.terrain;
        let v = t.vertices();
        // Corners in slot order: left, top, right, bottom.
        let corners = [(i + 1, j), (i, j), (i, j + 1), (i + 1, j + 1)].map(|(ci, cj)| (ci, cj, self.vertex(view, ci, cj)));
        let dirt = corners.map(|(_, _, (x, y))| t.altitude.get(x, y));
        let water = corners.map(|(_, _, (x, y))| t.water.get(x, y));
        let wet = [0, 1, 2, 3].map(|k| dirt[k] <= water[k]);
        let wet_count = wet.iter().filter(|&&w| w).count();
        let hazed = |c: [u8; 3]| haze(c, view.zoom).map(|v| v as f32);
        let (alts, cols, mode) = match wet_count {
            // cSC3DirtClodLand.
            0 => {
                let cols = corners.map(|(_, _, (x, y))| hazed(self.land[(x * v + y) as usize]));
                (dirt, cols, Mode::Land)
            }
            // cSC3DirtClodWater: the surface at the water, coloured by depth (palette 6, 0x498E8).
            4 => {
                let cols = [0, 1, 2, 3].map(|k| hazed(self.dirt.water.get(water[k].saturating_sub(dirt[k]) as usize, 0)));
                (water, cols, Mode::Water)
            }
            // cSC3DirtClodShore: corners at max(dirt, water), wet ones marked grey.
            _ => {
                let cols = [0, 1, 2, 3].map(|k| {
                    let (_, _, (x, y)) = corners[k];
                    hazed(if wet[k] { SHORE_WET } else { self.land[(x * v + y) as usize] })
                });
                let alts = [0, 1, 2, 3].map(|k| dirt[k].max(water[k]));
                let grey = haze(SHORE_WET, view.zoom);
                let water = haze(self.dirt.water.get(0, 0), view.zoom);
                (alts, cols, Mode::Shore { grey, water })
            }
        };
        let pts = [0, 1, 2, 3].map(|k| view.project(corners[k].0 as i32, corners[k].1 as i32, alts[k]));
        let (top, bottom) = (pts[1].1, pts[3].1);
        // Type 0: the top is not above the bottom, nothing is drawn.
        if top >= bottom {
            return;
        }
        let min_y = pts.iter().map(|p| p.1).min().unwrap();
        let bounds = Rect::new(pts[0].0, min_y, view.cell_width(), bottom - min_y);
        if bounds.intersect(&clip).is_empty() {
            return;
        }
        let (left, right) = (side(pts[0].1, top, bottom), side(pts[2].1, top, bottom));
        // The shore takes its fold from the wet corners, the others from the altitudes.
        let fold = match mode {
            Mode::Shore { .. } => wet[1] == wet[3] && wet[0] != wet[2],
            _ => alts[1] == alts[3] && alts[0] != alts[2],
        };
        let split = fold || (left <= 1 && right <= 1) || (left >= 3 && right >= 3);
        let fill = Fill {
            pts: &pts,
            cols: &cols,
            mode,
            land_bump: &self.land_bump,
            water_bump: &self.water_bump,
            bump_start: cell_bump_start(map_cell(t.size, view.rotation, i, j)),
            row0: min_y,
        };
        if split {
            fill.polygon(screen, clip, &[0, 1, 3]);
            fill.polygon(screen, clip, &[1, 2, 3]);
        } else {
            fill.polygon(screen, clip, &[0, 1, 2, 3]);
        }
    }

    /// One `cSC3DirtClodEdge` skirt under the map edge between draw-grid vertices `a` and `b`
    /// (`pregetPoints` 0x4BBB4, `Draw` 0x4B360). The water part runs from the water surface
    /// down to the dirt, indexed by depth below the water. The soil part runs from the dirt
    /// down to altitude −4, indexed `0x100` at the surface to `0x104 + dirt` at the bottom.
    /// The rasterizer it uses (0x48238) is not decompiled; this fills the index like the
    /// colours of the other clods and adds the water bump noise to the soil only.
    fn draw_edge(&self, screen: &mut Surface, clip: Rect, view: &View, a: (u32, u32), b: (u32, u32), ramp: &ColorTable) {
        let t = &self.terrain;
        let va = self.vertex(view, a.0, a.1);
        let vb = self.vertex(view, b.0, b.1);
        let (da, db) = (t.altitude.get(va.0, va.1), t.altitude.get(vb.0, vb.1));
        let (wa, wb) = (t.water.get(va.0, va.1).max(da), t.water.get(vb.0, vb.1).max(db));
        let at = |p: (u32, u32), alt: i32| {
            let (x, y) = view.project(p.0 as i32, p.1 as i32, 0);
            (x, y - alt * view.altitude_step())
        };
        let below = -4;
        let pts = [at(a, wa as i32), at(b, wb as i32), at(b, db as i32), at(a, da as i32)];
        let soil = [at(a, da as i32), at(b, db as i32), at(b, below), at(a, below)];
        let all = pts.iter().chain(soil.iter());
        let (x0, x1) = (all.clone().map(|p| p.0).min().unwrap(), all.clone().map(|p| p.0).max().unwrap());
        let (y0, y1) = (all.clone().map(|p| p.1).min().unwrap(), all.map(|p| p.1).max().unwrap());
        if Rect::new(x0, y0, x1 - x0, y1 - y0).intersect(&clip).is_empty() {
            return;
        }
        let index = |v: i32| [v as f32, 0.0, 0.0];
        let start = cell_bump_start(va);
        if wa > da || wb > db {
            let cols = [index(0), index(0), index(wb as i32 - db as i32), index(wa as i32 - da as i32)];
            let row0 = pts.iter().map(|p| p.1).min().unwrap();
            let fill = Fill { pts: &pts, cols: &cols, mode: Mode::Edge { ramp, view_zoom: view.zoom, bump: false }, land_bump: &self.land_bump, water_bump: &self.water_bump, bump_start: start, row0 };
            fill.polygon(screen, clip, &[0, 1, 2, 3]);
        }
        let cols = [index(0x100), index(0x100), index((0x104 + db as i32).min(0x1FF)), index((0x104 + da as i32).min(0x1FF))];
        let row0 = soil.iter().map(|p| p.1).min().unwrap();
        let fill = Fill { pts: &soil, cols: &cols, mode: Mode::Edge { ramp, view_zoom: view.zoom, bump: true }, land_bump: &self.land_bump, water_bump: &self.water_bump, bump_start: start, row0 };
        fill.polygon(screen, clip, &[0, 1, 2, 3]);
    }
}

/// Where a cell's bump noise starts: `(x · 0x6B9 + y · 0x757) & 0x3FF`.
fn cell_bump_start((x, y): (u32, u32)) -> usize {
    (x.wrapping_mul(0x6B9).wrapping_add(y.wrapping_mul(0x757)) & 0x3FF) as usize
}

/// Where a side corner's y lies against the top and bottom corners: 0 above the top, 1 level
/// with it, 2 between, 3 level with the bottom, 4 below.
fn side(y: i32, top: i32, bottom: i32) -> u8 {
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
}

/// `cSC3DirtClodLand::getColor`: row `0x100 − (dirt − water)` of the landscape palette,
/// entry `light >> 1`.
fn land_colour(t: &Terrain, palette: &ColorTable, light: u8, x: u32, y: u32) -> [u8; 3] {
    let row = 0x100 - (t.altitude.get(x, y) as i32 - t.water.get(x, y) as i32);
    palette.get(row as u16 as usize, (light >> 1) as usize)
}

/// Haze at zooms 0–2 (`getColor`, tables at libSimDirt .data 0x63480 and 0x634A0): each
/// channel loses `round(c / 10)` per zoom step below 3 and gains a blue-grey offset.
fn haze(c: [u8; 3], zoom: u32) -> [u8; 3] {
    const OFFSET: [[u8; 3]; 3] = [[0x31, 0x33, 0x35], [0x21, 0x22, 0x23], [0x10, 0x11, 0x12]];
    if zoom >= 3 {
        return c;
    }
    let steps = (3 - zoom) as u8;
    [0, 1, 2].map(|k| {
        let t = ((c[k] as u32 * 10 + 50) / 100) as u8;
        c[k].wrapping_sub(t.wrapping_mul(steps)).wrapping_add(OFFSET[zoom as usize][k])
    })
}

/// `GenerateBumpMaps` (0x48814), from one `cRZRandom`:
/// - land: each byte −8, 0 or +8;
/// - water: runs of 1–8 bytes of +8 or −9, each followed by 1–8 zero bytes.
fn bump_maps(seed: u32) -> ([u8; BUMP_LEN], [u8; BUMP_LEN]) {
    let mut rng = Random::new(seed);
    let mut land = [0u8; BUMP_LEN];
    for b in land.iter_mut() {
        *b = ((rng.next_u32() % 3) as u8).wrapping_mul(8).wrapping_sub(8);
    }
    let mut water = [0u8; BUMP_LEN];
    let mut k = 0;
    while k < BUMP_LEN {
        let run = (rng.next_u32() & 7) + 1;
        let v = if rng.next_u32() & 1 != 0 { 8 } else { 0xF7 };
        for _ in 0..run {
            if k < BUMP_LEN {
                water[k] = v;
                k += 1;
            }
        }
        let gap = (rng.next_u32() & 7) + 1;
        for _ in 0..gap {
            if k < BUMP_LEN {
                water[k] = 0;
                k += 1;
            }
        }
    }
    (land, water)
}

/// How a clod turns interpolated values into pixels.
#[derive(Clone, Copy)]
enum Mode<'a> {
    /// Rasterizer 0x463D0 with the land bump map: three noise bytes per pixel, one per channel.
    Land,
    /// Rasterizer 0x463D0 with the water bump map: one noise byte per pixel for all channels.
    Water,
    /// Rasterizer 0x47088. By squared distance `d` of the colour from the hazed wet grey:
    /// - `d ≥ 0x400`: land, the colour plus land noise;
    /// - `0x100 < d < 0x400`: a blend from the water colour to the land colour, plus land noise;
    /// - `d ≤ 0x100`: the water colour plus water noise.
    Shore { grey: [u8; 3], water: [u8; 3] },
    /// The skirt: channel 0 holds a palette row.
    Edge { ramp: &'a ColorTable, view_zoom: u32, bump: bool },
}

/// One clod's scanline fill (libSimDirt 0x463D0 and relatives).
struct Fill<'a> {
    pts: &'a [(i32, i32); 4],
    cols: &'a [[f32; 3]; 4],
    mode: Mode<'a>,
    land_bump: &'a [u8; BUMP_LEN],
    water_bump: &'a [u8; BUMP_LEN],
    bump_start: usize,
    /// Row 0 of the clod: the bump index advances 0x8F per row from here.
    row0: i32,
}

impl Fill<'_> {
    /// Fill the y-monotone polygon through `corners`, interpolating the corner values down
    /// its edges and across each scanline. Rows run from the top corner to the bottom one,
    /// bottom excluded; a span runs from the rounded left edge to the rounded right edge,
    /// right excluded.
    fn polygon(&self, screen: &mut Surface, clip: Rect, corners: &[usize]) {
        let ys = corners.iter().map(|&k| self.pts[k].1);
        let (y0, y1) = (ys.clone().min().unwrap(), ys.max().unwrap());
        let n = corners.len();
        for y in y0.max(clip.y)..y1.min(clip.bottom()) {
            let mut hits: [(f32, [f32; 3]); 2] = [(f32::MAX, [0.0; 3]), (f32::MIN, [0.0; 3])];
            for e in 0..n {
                let (a, b) = (corners[e], corners[(e + 1) % n]);
                let (pa, pb) = (self.pts[a], self.pts[b]);
                let (lo, hi) = if pa.1 < pb.1 { (a, b) } else { (b, a) };
                let (plo, phi) = (self.pts[lo], self.pts[hi]);
                if plo.1 == phi.1 || y < plo.1 || y >= phi.1 {
                    continue;
                }
                let t = (y - plo.1) as f32 / (phi.1 - plo.1) as f32;
                let x = plo.0 as f32 + (phi.0 - plo.0) as f32 * t;
                let c = [0, 1, 2].map(|k| self.cols[lo][k] + (self.cols[hi][k] - self.cols[lo][k]) * t);
                if x < hits[0].0 {
                    hits[0] = (x, c);
                }
                if x > hits[1].0 {
                    hits[1] = (x, c);
                }
            }
            let ((lx, lc), (rx, rc)) = (hits[0], hits[1]);
            if rx - lx + 1.0 <= 0.0 {
                continue;
            }
            let inv = 1.0 / (rx - lx + 1.0);
            let step = [0, 1, 2].map(|k| (rc[k] - lc[k]) * inv);
            let mut c = lc;
            let mut b = ((y - self.row0) as usize * 0x8F + self.bump_start) & 0x3FF;
            let row = (y * screen.width) as usize;
            for x in lx.round() as i32..rx.round() as i32 {
                let rgb = self.pixel(c, &mut b);
                if x >= clip.x && x < clip.right() {
                    screen.pixels[row + x as usize] = quantize(rgb);
                }
                for k in 0..3 {
                    c[k] += step[k];
                }
            }
        }
    }

    /// One pixel from the interpolated value `c`, advancing the bump index `b`. Noise is added
    /// to the rounded channels and wraps, as in the original.
    fn pixel(&self, c: [f32; 3], b: &mut usize) -> u32 {
        let pack = |p: [u8; 3]| (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32;
        let rounded = c.map(|v| v.round() as i32 as u8);
        let land_noise = |b: &mut usize| {
            let n = [0, 1, 2].map(|k| self.land_bump[(*b + k) & 0x3FF]);
            *b = (*b + 3) & 0x3FF;
            n
        };
        match self.mode {
            Mode::Land => {
                let n = land_noise(b);
                pack([0, 1, 2].map(|k| rounded[k].wrapping_add(n[k])))
            }
            Mode::Water => {
                let n = self.water_bump[*b];
                *b = (*b + 1) & 0x3FF;
                pack(rounded.map(|v| v.wrapping_add(n)))
            }
            Mode::Shore { grey, water } => {
                let n = land_noise(b);
                let d: i32 = (0..3).map(|k| (rounded[k] as i32 - grey[k] as i32).pow(2)).sum();
                if d >= SHORE_LAND {
                    pack([0, 1, 2].map(|k| rounded[k].wrapping_add(n[k])))
                } else if d > SHORE_WATER {
                    let (to_water, to_land) = (SHORE_LAND - d, d - SHORE_WATER);
                    let span = (SHORE_LAND - SHORE_WATER) as f32;
                    pack([0, 1, 2].map(|k| {
                        let v = (water[k] as i32 * to_water + rounded[k] as i32 * to_land) as f32 / span;
                        (v.round() as i32 as u8).wrapping_add(n[k])
                    }))
                } else {
                    let w = self.water_bump[*b];
                    pack(water.map(|v| v.wrapping_add(w)))
                }
            }
            Mode::Edge { ramp, view_zoom, bump } => {
                let row = c[0].round().clamp(0.0, (ramp.row_count() - 1) as f32) as usize;
                let colour = haze(ramp.get(row, 0), view_zoom);
                let n = if bump { self.water_bump[*b] } else { 0 };
                *b = (*b + 1) & 0x3FF;
                pack(colour.map(|v| v.wrapping_add(n)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sc3k_sim::dirt::{generate, Params};

    #[test]
    fn rotation_maps_vertices() {
        assert_eq!(map_vertex(4, 0, 1, 2), (1, 2));
        assert_eq!(map_vertex(4, 1, 1, 2), (2, 3));
        assert_eq!(map_vertex(4, 2, 1, 2), (3, 2));
        assert_eq!(map_vertex(4, 3, 1, 2), (2, 1));
        // Cells agree with ActualGridToDrawGrid: map cell (x, y) = (1, 2) of a 4 × 4 map
        // is drawn at (4 − 1 − 2, 1) = (1, 1) in rotation 1.
        assert_eq!(map_cell(4, 1, 1, 1), (1, 2));
        // Rotation 3 draws it at (y, 4 − 1 − x) = (2, 2).
        assert_eq!(map_cell(4, 3, 2, 2), (1, 2));
    }

    #[test]
    fn projection_matches_corner_layout() {
        let v = View { zoom: 4, rotation: 0, origin_x: 0, origin_y: 0 };
        // Left, top, right and bottom corners of cell (0, 0), flat at altitude 0.
        assert_eq!(v.project(1, 0, 0), (-64, 32));
        assert_eq!(v.project(0, 0, 0), (0, 0));
        assert_eq!(v.project(0, 1, 0), (64, 32));
        assert_eq!(v.project(1, 1, 0), (0, 64));
        assert_eq!(v.project(0, 0, 3), (0, -48));
    }

    #[test]
    fn haze_table() {
        assert_eq!(haze([200, 100, 0], 3), [200, 100, 0]);
        // T[200] = 20, T[100] = 10: zoom 0 removes three steps and adds (0x31, 0x33, 0x35).
        assert_eq!(haze([200, 100, 0], 0), [200 - 60 + 0x31, 100 - 30 + 0x33, 0x35]);
        assert_eq!(haze([15, 4, 5], 2), [15 - 2 + 0x10, 4 + 0x11, 5 - 1 + 0x12]);
    }

    #[test]
    fn shore_pixels() {
        let pts = [(0, 0); 4];
        let cols = [[0.0; 3]; 4];
        let (land, water) = ([0u8; BUMP_LEN], [5u8; BUMP_LEN]);
        let mode = Mode::Shore { grey: [0xA0; 3], water: [10, 20, 30] };
        let fill = Fill { pts: &pts, cols: &cols, mode, land_bump: &land, water_bump: &water, bump_start: 0, row0: 0 };
        let mut b = 0;
        // Next to the grey: water colour plus water noise.
        assert_eq!(fill.pixel([0xA0 as f32 + 5.0, 0xA0 as f32, 0xA0 as f32], &mut b), 15 << 16 | 25 << 8 | 35);
        assert_eq!(b, 3, "three land noise bytes per pixel");
        // Far from it: the land colour unchanged (zero land noise).
        assert_eq!(fill.pixel([0.0, 0.0, 0.0], &mut b), 0);
        // d = 3 · 15² = 675: (water · 349 + land · 419) / 768.
        let p = fill.pixel([0xA0 as f32 - 15.0; 3], &mut b);
        let blend = |w: i32| ((w * 349 + 145 * 419) as f32 / 768.0).round() as u32;
        assert_eq!(p, blend(10) << 16 | blend(20) << 8 | blend(30));
    }

    #[test]
    fn bump_values() {
        let (land, water) = bump_maps(1);
        assert!(land.iter().all(|&v| [0xF8, 0, 8].contains(&v)));
        assert!(water.iter().all(|&v| [0xF7, 0, 8].contains(&v)));
        assert!(water.contains(&0) && water.contains(&8));
    }

    #[test]
    fn draws_flat_cell_without_gaps() {
        let terrain = generate(64, 1, Params::new_city(3));
        let table = |w: u32, h: u32| ColorTable::from_bmp(&sc3k_formats::bmp::Bmp { width: w, height: h, pixels: vec![[96, 128, 64]; (w * h) as usize] });
        let dirt = DirtPalettes { water: table(1, 256), edge_light: table(1, 512), edge_dark: table(1, 512) };
        let scene = TerrainScene::new(terrain, &table(32, 512), &dirt, 0);
        let mut s = Surface::new(320, 240);
        s.fill(0xFF00FF);
        let view = View::centred(64, scene.terrain().sea_level, 2, 0, 320, 240);
        scene.draw(&mut s, &view);
        // The middle of the screen is well inside the map: covered, no background showing.
        for y in 100..140 {
            for x in 140..180 {
                assert_ne!(s.pixels[(y * 320 + x) as usize], quantize(0xFF00FF), "hole at {x},{y}");
            }
        }
    }
}
