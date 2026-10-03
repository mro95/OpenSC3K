//! `cRZCellMap<T>`: a fixed-size 2D grid. See `docs/sim/random.md`.
//!
//! The original stores one array per x, each `CellCountY` long, and does not bounds-check
//! `GetValue`/`SetValue`. Here out-of-range access panics.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellMap<T> {
    width: u32,
    height: u32,
    cells: Vec<T>,
}

impl<T: Copy> CellMap<T> {
    /// `cRZCellMap(x, y, fill)`.
    pub fn new(width: u32, height: u32, fill: T) -> CellMap<T> {
        CellMap {
            width,
            height,
            cells: vec![fill; width as usize * height as usize],
        }
    }

    /// `CellCountX`.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// `CellCountY`.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// `InBounds(x, y)`.
    pub fn in_bounds(&self, x: u32, y: u32) -> bool {
        x < self.width && y < self.height
    }

    fn index(&self, x: u32, y: u32) -> usize {
        assert!(
            self.in_bounds(x, y),
            "cell ({x}, {y}) outside {}x{}",
            self.width,
            self.height
        );
        x as usize * self.height as usize + y as usize
    }

    /// `GetValue(x, y)`.
    pub fn get(&self, x: u32, y: u32) -> T {
        self.cells[self.index(x, y)]
    }

    /// `SetValue(x, y, v)`.
    pub fn set(&mut self, x: u32, y: u32, v: T) {
        let i = self.index(x, y);
        self.cells[i] = v;
    }

    /// `SetValue(x0, y0, x1, y1, v)`: the inclusive rectangle.
    pub fn set_rect(&mut self, x0: u32, y0: u32, x1: u32, y1: u32, v: T) {
        for x in x0..=x1 {
            for y in y0..=y1 {
                self.set(x, y, v);
            }
        }
    }

    /// `SetAllCells(v)`.
    pub fn fill(&mut self, v: T) {
        self.cells.fill(v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_and_rect() {
        let mut m = CellMap::new(4, 3, 0u8);
        m.set(3, 2, 9);
        assert_eq!(m.get(3, 2), 9);
        assert_eq!(m.get(2, 3 - 1), 0);
        m.set_rect(1, 0, 2, 1, 5);
        let filled: Vec<_> = (0..4)
            .flat_map(|x| (0..3).map(move |y| (x, y)))
            .filter(|&(x, y)| m.get(x, y) == 5)
            .collect();
        assert_eq!(filled, vec![(1, 0), (1, 1), (2, 0), (2, 1)]);
        m.fill(1);
        assert_eq!(m.get(0, 0), 1);
        assert!(!m.in_bounds(4, 0));
    }

    #[test]
    #[should_panic]
    fn out_of_range_panics() {
        CellMap::new(2, 2, 0u8).get(2, 0);
    }
}
