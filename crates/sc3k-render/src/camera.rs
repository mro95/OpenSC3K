//! The city view camera (`cSC3CityViewIso`, libSimSpr): scrolling by pixels, zooming and
//! rotating about the focus cell. See `docs/render/terrain.md`, "Camera".

use crate::terrain::{map_cell, View, MAX_ZOOM};
use sc3k_sim::dirt::Terrain;

/// Pixels per scroll step at every zoom (`cSC3WinCityView::Init` and `setTweakableSettings`
/// set all five per-zoom speeds to 32.0).
pub const SCROLL_STEP: i32 = 32;

/// Scroll directions, as the original's four flags (`cSC3WinCityView` +0x1D1..+0x1D4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Scroll {
    pub up: bool,
    pub down: bool,
    pub right: bool,
    pub left: bool,
}

impl Scroll {
    pub fn any(&self) -> bool {
        self.up || self.down || self.right || self.left
    }

    pub fn union(self, o: Scroll) -> Scroll {
        Scroll { up: self.up || o.up, down: self.down || o.down, right: self.right || o.right, left: self.left || o.left }
    }

    /// The step `paintScroll` (libSimSpr 0x10392C) takes: one flag scrolls straight, two
    /// adjacent flags diagonally; anything else (opposites, three or four) does not move.
    /// Unchecked: no Windows address known yet.
    pub fn step(&self) -> Option<(i32, i32)> {
        let s = SCROLL_STEP;
        match (self.up, self.down, self.right, self.left) {
            (true, false, false, false) => Some((0, -s)),
            (false, true, false, false) => Some((0, s)),
            (false, false, true, false) => Some((s, 0)),
            (false, false, false, true) => Some((-s, 0)),
            (true, false, false, true) => Some((-s, -s)),
            (false, true, false, true) => Some((-s, s)),
            (true, false, true, false) => Some((s, -s)),
            (false, true, true, false) => Some((s, s)),
            _ => None,
        }
    }

    /// Edge scrolling (`UpdateScroll`, 0x1034D0, with the regions of `updateScrollRegions`,
    /// 0x1258D8): 64 pixels at the left and right, 48 at the top and bottom of the view.
    /// Inside the inner rectangle nothing scrolls.
    /// Unchecked: no check runs the city view camera yet; updateScrollRegions has no Windows
    /// address known yet.
    pub fn from_edge(x: i32, y: i32, width: i32, height: i32) -> Scroll {
        let inner = x >= 64 && y >= 48 && x < width - 64 && y < height - 48;
        if inner {
            return Scroll::default();
        }
        Scroll {
            left: x >= 0 && y >= 0 && x < 64 && y < height,
            up: x >= 0 && y >= 0 && x < width && y < 48,
            right: x >= width - 64 && y >= 0 && x <= width && y <= height,
            down: x >= 0 && y >= height - 48 && x <= width && y <= height,
        }
    }
}

/// The view plus the focus cell: the map cell under the middle of the screen, which zooming
/// and rotating keep in place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Camera {
    pub view: View,
    /// Map cell (x, y).
    pub focus: (u32, u32),
    width: i32,
    height: i32,
}

impl Camera {
    /// A camera on the middle of the map.
    pub fn new(t: &Terrain, zoom: u32, rotation: u32, width: i32, height: i32) -> Camera {
        let mid = t.size / 2;
        let view = View { zoom: zoom.min(MAX_ZOOM), rotation: rotation & 3, origin_x: 0, origin_y: 0 };
        let mut c = Camera { view, focus: (mid, mid), width, height };
        c.centre(t);
        c
    }

    /// `Translate` (Ghidra 0xB071C): move by (dx, dy) screen pixels if the middle of
    /// the screen still lands on the map; that cell becomes the focus.
    /// Unchecked: no check runs the city view camera yet.
    pub fn translate(&mut self, t: &Terrain, dx: i32, dy: i32) -> bool {
        let mut v = self.view;
        v.origin_x -= dx;
        v.origin_y -= dy;
        match pick(t, &v, self.width / 2, self.height / 2) {
            Some(cell) => {
                self.view = v;
                self.focus = cell;
                true
            }
            None => false,
        }
    }

    /// `ZoomIn`: up to zoom 4.
    pub fn zoom_in(&mut self, t: &Terrain) {
        if self.view.zoom < MAX_ZOOM {
            self.view.zoom += 1;
            self.centre(t);
        }
    }

    /// `ZoomOut`: down to zoom 0.
    pub fn zoom_out(&mut self, t: &Terrain) {
        if self.view.zoom > 0 {
            self.view.zoom -= 1;
            self.centre(t);
        }
    }

    /// `RotateCameraCW`: rotation 0 becomes 3, otherwise one less.
    pub fn rotate_cw(&mut self, t: &Terrain) {
        self.view.rotation = (self.view.rotation + 3) & 3;
        self.centre(t);
    }

    /// `RotateCameraCCW`: the other way.
    pub fn rotate_ccw(&mut self, t: &Terrain) {
        self.view.rotation = (self.view.rotation + 1) & 3;
        self.centre(t);
    }

    /// Put the middle of the focus cell, at its surface, in the middle of the screen.
    fn centre(&mut self, t: &Terrain) {
        let (i, j) = draw_cell(t.size, self.view.rotation, self.focus);
        let mut v = self.view;
        v.origin_x = 0;
        v.origin_y = 0;
        let (x, y) = v.project(i as i32, j as i32, surface(t, self.focus));
        // The cell middle is half a cell right of vertex (i, j)'s column and half a cell down.
        v.origin_x = self.width / 2 - x;
        v.origin_y = self.height / 2 - (y + v.cell_height() / 2);
        self.view = v;
    }
}

/// `ActualGridToDrawGrid` (0x99990): the draw-grid cell of map cell (x, y).
/// Unchecked: no Windows address known yet.
fn draw_cell(size: u32, rotation: u32, (x, y): (u32, u32)) -> (u32, u32) {
    match rotation & 3 {
        0 => (x, y),
        1 => (size - 1 - y, x),
        2 => (size - 1 - x, size - 1 - y),
        _ => (y, size - 1 - x),
    }
}

/// Mean surface altitude (dirt or water, whichever is higher) of a cell's corners.
fn surface(t: &Terrain, (x, y): (u32, u32)) -> u8 {
    let corners = [(x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1)];
    let sum: u32 = corners.iter().map(|&(cx, cy)| t.altitude.get(cx, cy).max(t.water.get(cx, cy)) as u32).sum();
    (sum / 4) as u8
}

/// The map cell under screen pixel (sx, sy), or `None` off the map. Stands in for
/// `cSC3CitySpriteCellMap::WindowPixelToActualGrid`, which is not ported: it intersects the
/// ray with the plane at the sea level, then a few times with the plane at the surface of
/// the cell found.
pub fn pick(t: &Terrain, v: &View, sx: i32, sy: i32) -> Option<(u32, u32)> {
    let half_w = (v.cell_width() / 2) as f64;
    let half_h = (v.cell_height() / 2) as f64;
    let at = |a: u8| -> Option<(u32, u32)> {
        let u = (sx - v.origin_x) as f64 / half_w;
        let w = (sy - v.origin_y + a as i32 * v.altitude_step()) as f64 / half_h;
        let (i, j) = (((w - u) / 2.0).floor(), ((w + u) / 2.0).floor());
        let n = t.size as f64;
        if i < 0.0 || j < 0.0 || i >= n || j >= n {
            return None;
        }
        Some(map_cell(t.size, v.rotation, i as u32, j as u32))
    };
    let mut cell = at(t.sea_level)?;
    for _ in 0..4 {
        match at(surface(t, cell)) {
            Some(c) if c != cell => cell = c,
            _ => break,
        }
    }
    Some(cell)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sc3k_sim::cellmap::CellMap;

    fn flat(size: u32, alt: u8) -> Terrain {
        let v = size + 1;
        Terrain {
            size,
            sea_level: 0,
            altitude: CellMap::new(v, v, alt),
            water: CellMap::new(v, v, 0),
            salt: CellMap::new(v, v, false),
            flora: CellMap::new(v, v, 0),
        }
    }

    #[test]
    fn steps() {
        let up = Scroll { up: true, ..Scroll::default() };
        let left = Scroll { left: true, ..Scroll::default() };
        let down = Scroll { down: true, ..Scroll::default() };
        assert_eq!(up.step(), Some((0, -32)));
        assert_eq!(up.union(left).step(), Some((-32, -32)));
        assert_eq!(up.union(down).step(), None);
        assert_eq!(Scroll::from_edge(400, 300, 800, 600), Scroll::default());
        assert_eq!(Scroll::from_edge(10, 10, 800, 600), up.union(left));
        assert_eq!(Scroll::from_edge(400, 590, 800, 600), down);
    }

    #[test]
    fn draw_cell_inverts_map_cell() {
        for r in 0..4 {
            for (x, y) in [(0, 0), (3, 1), (5, 6)] {
                let (i, j) = draw_cell(8, r, (x, y));
                assert_eq!(map_cell(8, r, i, j), (x, y), "rotation {r}");
            }
        }
    }

    #[test]
    fn centre_pick_and_keep_focus() {
        let t = flat(16, 5);
        let mut c = Camera::new(&t, 2, 0, 640, 480);
        assert_eq!(pick(&t, &c.view, 320, 480 / 2), Some((8, 8)));
        assert!(c.translate(&t, 0, 16));
        let focus = c.focus;
        assert_ne!(focus, (8, 8));
        for r in 0..4 {
            c.rotate_cw(&t);
            assert_eq!(pick(&t, &c.view, 320, 240), Some(focus), "rotation {r}");
        }
        c.zoom_in(&t);
        assert_eq!(pick(&t, &c.view, 320, 240), Some(focus));
        // Far off the map the centre leaves it, so the move is refused.
        assert!(!c.translate(&t, 100_000, 0));
    }
}
