//! The city view's ground: one dirt clod per cell (`crate::clod`) and the skirts along the
//! map's front edges. See `docs/render/terrain.md`.

use crate::clod::{dirt_clod, edge_ramps, Area, Bumps, Clod, EdgeClod, Palettes};
use crate::light::vertex_light;
use crate::palette::{ColorTable, DirtPalettes};
use sc3k_sim::cellmap::CellMap;
use sc3k_sim::dirt::Terrain;
use sc3k_sim::rng::Random;
use sc3k_ui::Surface;

/// Closest zoom level. Zooms run 0..=4.
pub const MAX_ZOOM: u32 = 4;
/// Length of the bump-noise tables (`GenerateBumpMaps`).
pub const BUMP_LEN: usize = 0x400;

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

/// The map cell drawn at draw-grid cell (i, j): the inverse of `ActualGridToDrawGrid`.
pub(crate) fn map_cell(size: u32, rotation: u32, i: u32, j: u32) -> (u32, u32) {
    match rotation & 3 {
        0 => (i, j),
        1 => (j, size - 1 - i),
        2 => (size - 1 - i, size - 1 - j),
        _ => (size - 1 - j, i),
    }
}

/// What the city view draws the ground from: the terrain, its vertex light, the palettes and
/// the bump noise.
pub struct TerrainScene {
    terrain: Terrain,
    light: CellMap<u8>,
    land: ColorTable,
    dirt: DirtPalettes,
    land_bump: [u8; BUMP_LEN],
    water_bump: [u8; BUMP_LEN],
}

impl TerrainScene {
    /// `bump_seed` replaces the original's clock seed for the bump noise.
    pub fn new(terrain: Terrain, land: &ColorTable, dirt: &DirtPalettes, bump_seed: u32) -> TerrainScene {
        let light = vertex_light(&terrain);
        let (land_bump, water_bump) = bump_maps(bump_seed);
        TerrainScene { terrain, light, land: land.clone(), dirt: dirt.clone(), land_bump, water_bump }
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
        let clip = Area { x1: clip.x, y1: clip.y, x2: clip.right(), y2: clip.bottom() };
        for diagonal in 0..2 * size - 1 {
            let first = diagonal.saturating_sub(size - 1);
            let last = diagonal.min(size - 1);
            for i in first..=last {
                let j = diagonal - i;
                self.draw_cell(screen, clip, view, i, j);
                on_cell(screen, (i, j), map_cell(size, view.rotation, i, j));
            }
        }
        let ramps = edge_ramps(&self.dirt.edge_light, &self.dirt.edge_dark, view.zoom);
        for k in 0..size {
            // The left front edge under draw row i = size − 1, the right one under column
            // j = size − 1.
            self.draw_edge(screen, clip, view, &ramps, (size - 1, k), true);
            self.draw_edge(screen, clip, view, &ramps, (k, size - 1), false);
        }
    }

    /// Where the sprite cell map puts the clods of draw cell (i, j): the cell's left corner at
    /// the altitude `GetDirtClod` gives the cell.
    fn anchor(&self, view: &View, (i, j): (u32, u32), alt: u8) -> (i32, i32) {
        view.project(i as i32 + 1, j as i32, alt)
    }

    /// One clod (`sc3k_render::clod`), of the kind `GetDirtClod` picks.
    fn draw_cell(&self, screen: &mut Surface, clip: Area, view: &View, i: u32, j: u32) {
        let t = &self.terrain;
        let (x, z) = map_cell(t.size, view.rotation, i, j);
        let (kind, alt) = dirt_clod(t, x, z, false);
        let clod = Clod::new(t, kind, x, z, view.zoom, view.rotation);
        let at = self.anchor(view, (i, j), alt);
        let area = clod.area_from_pt(view.zoom, view.rotation, at.0, at.1);
        if area.y2 <= clip.y1 || area.y1 >= clip.y2 || area.x2 <= clip.x1 || area.x1 >= clip.x2 {
            return;
        }
        let pal = Palettes { land: &self.land, water: &self.dirt.water };
        let bumps = Bumps { land: &self.land_bump, water: &self.water_bump };
        let v = (view.zoom, view.rotation);
        clod.draw(screen, t, &self.light, &pal, &bumps, (x, z), v, at, Some(clip), false);
    }

    /// One map-edge skirt under draw cell `cell`: its left side (`side`) or its right one.
    /// `GetDirtClodEdge` (libSimDirt 0x40F00, SIMDIRT.DLL 0x10005D80) gives the cell the higher
    /// of the dirt and the water.
    fn draw_edge(&self, screen: &mut Surface, clip: Area, view: &View, ramps: &[[u16; 0x200]; 2], cell: (u32, u32), side: bool) {
        let t = &self.terrain;
        let (x, z) = map_cell(t.size, view.rotation, cell.0, cell.1);
        let alt = t.altitude.get(x, z).max(t.water.get(x, z));
        let edge = EdgeClod::new(t, x, z, side, false, view.zoom, view.rotation);
        let at = self.anchor(view, cell, alt);
        let v = (view.zoom, view.rotation);
        edge.draw(screen, t, ramps, &self.water_bump, (x, z), v, at, Some(clip));
    }
}

/// `GenerateBumpMaps` (0x48814, SIMDIRT.DLL 0x100128E1), from one `cRZRandom`:
/// - land: each byte −8, 0 or +8;
/// - water: runs of 1–8 bytes of +8 or −9, each followed by 1–8 zero bytes.
///
/// The original seeds it with the clock; `seed` stands in for the clock's value.
pub fn bump_maps(seed: u32) -> ([u8; BUMP_LEN], [u8; BUMP_LEN]) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use sc3k_sim::dirt::{generate, Params};
    use sc3k_ui::surface::quantize;

    #[test]
    fn rotation_maps_cells() {
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
