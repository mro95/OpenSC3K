use crate::cellmap::CellMap;

enum NetworkTypes {
    Road = 1,
    Rail = 2,
    Power = 3,
    Highway = 4,
    Pipe = 5,
    Subway = 6,
    OnRamp = 8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NetworkTile {
    pub tile_id: u16,
    pub rotation: u8,
    pub altitude: u8,
}

pub struct Networks {
    pub surface: CellMap<Option<NetworkTile>>,
    pub plumbing: CellMap<Option<NetworkTile>>,
    pub subway: CellMap<Option<NetworkTile>>,
}

impl Networks {
    pub fn new(size: u32) -> Self {
        Self {
            surface: CellMap::new(size, size, None),
            plumbing: CellMap::new(size, size, None),
            subway: CellMap::new(size, size, None),
        }
    }
}
