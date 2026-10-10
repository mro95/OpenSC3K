//! Road tile sprites (`cSTNetworkOcc` occupants). See `docs/render/roads.md`.
//!
//! A tile id is the instance of a network occupant key; the chain from there to the pictures
//! is the one in `occupants.rs`. There are no road sets: everything comes from the base files.
//!
//! [`RoadTile`] is one tile; [`RoadSprites`] holds the tiles a city uses and draws them on
//! their cells; [`draw_cell`] draws a whole network map during the terrain pass.

use sc3k_assets::Assets;
use sc3k_formats::{
    ixf::{Archive, Tgi},
    occupant::{GROUP_NETWORK_OCCUPANT, TYPE_NETWORK_OCCUPANT},
    sprite::ImageInfo,
};
use sc3k_sim::{cellmap::CellMap, transit::NetworkTile};
use sc3k_ui::Surface;
use std::collections::HashMap;

use crate::occupants::{load_occupant, Image};
use crate::terrain::View;

/// The occupant records, sprite attributes and the two sprite archives road tiles use
/// (group 5 and group 0xB).
const BASE: [&str; 4] = [
    "Res/Occupant/OccupantAttribs.IXF",
    "Res/Sprites/CSATTRIB.IXF",
    "Res/Sprites/00000005_Roads.DAT",
    "Res/Sprites/0000000B_Utilities.DAT",
];

/// The frames of one network tile, indexed `layer · 20 + zoom · 4 + rotation`.
pub struct RoadTile {
    /// The sprite attributes' class; see [`slope`].
    class: u32,
    images: Vec<Option<Image>>,
}

/// The tiles of a city's networks, loaded once for the ids the city uses.
pub struct RoadSprites {
    /// Tile id to its frames.
    sprites: HashMap<u16, RoadTile>,
}

impl RoadSprites {
    /// Load every tile of `tile_ids`, opening the archives once. Ids the install has no
    /// occupant record or sprite attributes for are skipped with a warning.
    pub fn load(assets: &Assets, tile_ids: &[u16]) -> Result<Self, String> {
        let archives = base_archives(assets)?;
        let mut sprites = HashMap::new();
        for &id in tile_ids {
            if sprites.contains_key(&id) {
                continue;
            }
            match RoadTile::load_from(&archives, id as u32) {
                Ok(tile) => {
                    sprites.insert(id, tile);
                }
                Err(e) => eprintln!("road tile {id}: {e}"),
            }
        }
        Ok(Self { sprites })
    }

    /// Draw `network_tile` at draw-grid cell (i, j), placed as a tree is
    /// (`FloraSprites::draw`): anchored at the left of the cell's bounding box and the height
    /// of its top corner, raised by the tile's own altitude. The frame's rotation is the
    /// tile's plus the view's. A tile without sprites draws nothing.
    pub fn draw(&self, screen: &mut Surface, view: &View, (i, j): (u32, u32), network_tile: NetworkTile) {
        let Some(road_tile) = self.sprites.get(&network_tile.tile_id) else { return };
        let (top_x, top_y) = view.project(i as i32, j as i32, network_tile.altitude);
        let x = top_x - view.cell_width() / 2;
        let y = top_y + road_tile.slope_shift(view.zoom, network_tile.rotation);
        let rotation = (network_tile.rotation as u32 + view.rotation) & 3;
        road_tile.draw(screen, (x, y), view.zoom, rotation);
    }
}

impl RoadTile {
    /// Load tile `tile`. Fails if the install has no occupant record or sprite attributes for
    /// it, as for some ids in `ROAD_GRND_Set.txt`.
    pub fn load(assets: &Assets, tile: u32) -> Result<Self, String> {
        Self::load_from(&base_archives(assets)?, tile)
    }

    /// Load tile `tile` from archives opened with [`base_archives`].
    fn load_from(archives: &[Archive], tile: u32) -> Result<Self, String> {
        let find = |tgi: Tgi| archives.iter().find_map(|a| a.get(tgi));
        let key = Tgi { type_id: TYPE_NETWORK_OCCUPANT, group_id: GROUP_NETWORK_OCCUPANT, instance_id: tile };
        let sprites = load_occupant(&find, key)?;
        Ok(Self { class: sprites.class, images: sprites.images })
    }

    /// Pixels to move the anchor down for a tile with rotation `rotation` (the tile's own,
    /// without the view's) at `zoom`: `-(dh << zoom)` for a [`slope`] class whose rotations
    /// include it, else 0. `cSC3CitySpriteInstSloped<dh, r1, r2>::Draw` (libSimSpr Ghidra
    /// 0x13E094) moves the image rectangle this way through its `GetAllSpans` (0x13D7A0),
    /// which adds `dh << zoom` to the image info's `up` and takes it from `down`.
    /// Unchecked: no Windows address known yet.
    pub fn slope_shift(&self, zoom: u32, rotation: u8) -> i32 {
        match slope(self.class) {
            Some((dh, rotations)) if rotations.contains(&(rotation & 3)) => -(dh << zoom),
            _ => 0,
        }
    }

    /// Draw layer 0 of the frame for `zoom` and `rotation`, anchored at `(x, y)`: the left of
    /// the cell's bounding box and the height of its top corner, as for trees. `rotation` is the
    /// tile's rotation plus the view's, modulo 4. A missing frame draws nothing.
    pub fn draw(&self, screen: &mut Surface, (x, y): (i32, i32), zoom: u32, rotation: u32) {
        let Some(Some(img)) = self.images.get(frame(zoom, rotation)) else {
            return;
        };
        let clip = screen.area();
        screen.blit_clip(
            &img.sprite,
            img.sprite.area(),
            x - img.info.left as i32,
            y - img.info.up as i32,
            clip,
        );
    }

    /// The image info of layer 0's frame for `zoom` and `rotation`: the frame covers
    /// `(x − left, y − up)` to `(x + right, y + down)` around the anchor. `None` if it is missing.
    pub fn info(&self, zoom: u32, rotation: u32) -> Option<ImageInfo> {
        self.images
            .get(frame(zoom, rotation))?
            .as_ref()
            .map(|img| img.info)
    }

    /// The number of frames: 20 for one layer, 40 for two.
    pub fn frame_count(&self) -> usize {
        self.images.len()
    }
}

/// Draw `tiles` (one entry per map cell) with `sprites`, as a [`TerrainScene::draw_with`]
/// hook.
///
/// [`TerrainScene::draw_with`]: crate::terrain::TerrainScene::draw_with
pub fn draw_cell<'a>(
    sprites: &'a RoadSprites,
    tiles: &'a CellMap<Option<NetworkTile>>,
    view: &View,
) -> impl Fn(&mut Surface, (u32, u32), (u32, u32)) + 'a {
    let view = *view;
    move |screen, draw, (x, y)| {
        if let Some(tile) = tiles.get(x, y) {
            sprites.draw(screen, &view, draw, tile);
        }
    }
}

/// The template arguments `(dh, [r1, r2])` of the `cSC3CitySpriteInstSloped<dh, r1, r2>` that
/// `cSC3CitySpriteInstSpecialFactory` creates for sprite class `class` (libSimSpr Ghidra
/// 0xA41D4 to 0xA4354): at tile rotation r1 or r2 the picture moves `dh` altitude units up.
/// The Low classes move the picture of a descending slope onto its lowest corner, the High
/// classes that of a rising one onto its highest; the tile's altitude is its corner (x, y).
fn slope(class: u32) -> Option<(i32, [u8; 2])> {
    match class {
        0x59D => Some((-1, [0, 1])), // SlopedOneLow: 11202
        0x59E => Some((1, [2, 3])),  // SlopedOneHigh: 11201
        0x59F => Some((-2, [0, 1])), // SlopedTwoLow: 32
        0x5A0 => Some((2, [2, 3])),  // SlopedTwoHigh: 31
        _ => None,
    }
}

/// The layer-0 frame index (`cSC3CitySpriteInst::zoom_and_compass_to_frame_no`, exe 0x0821DCF8).
fn frame(zoom: u32, rotation: u32) -> usize {
    const ROTATIONS: usize = 4;
    zoom as usize * ROTATIONS + rotation as usize
}

/// Open the [`BASE`] archives, in lookup order.
fn base_archives(assets: &Assets) -> Result<Vec<Archive>, String> {
    let mut archives = Vec::new();
    for path in BASE {
        archives.push(assets.archive(path).map_err(|e| e.to_string())?);
    }
    Ok(archives)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sc3k_sim::cellmap::CellMap;

    /// Tiles 29 (Road, one layer), 67 (Wire Crossing Road, two layers), 15045 (sprite
    /// attributes through property 0x67) and 11600 (no occupant record), with 29 twice.
    fn sprites() -> Option<RoadSprites> {
        let assets = Assets::from_env("ENGLISH").ok()?;
        Some(RoadSprites::load(&assets, &[29, 67, 15045, 11600, 29]).unwrap())
    }

    /// Zoom 2, rotation `rotation`, with draw-grid vertex (0, 0) near the top middle of a
    /// 256 × 256 screen.
    fn view(rotation: u32) -> View {
        View { zoom: 2, rotation, origin_x: 128, origin_y: 96 }
    }

    fn road(tile_id: u16, rotation: u8, altitude: u8) -> NetworkTile {
        NetworkTile { tile_id, rotation, altitude }
    }

    #[test]
    fn loads_known_tiles_once_and_skips_missing() {
        let Some(sprites) = sprites() else { return };
        assert_eq!(sprites.sprites.len(), 3);
        assert_eq!(sprites.sprites[&29].frame_count(), 20);
        assert_eq!(sprites.sprites[&67].frame_count(), 40);
        assert!(sprites.sprites.contains_key(&15045));
        assert!(!sprites.sprites.contains_key(&11600));
    }

    #[test]
    fn draws_at_the_cell_anchor_with_tile_plus_view_rotation() {
        let Some(sprites) = sprites() else { return };
        let view = view(2);
        let mut drawn = Surface::new(256, 256);
        sprites.draw(&mut drawn, &view, (1, 1), road(29, 3, 3));

        let (top_x, top_y) = view.project(1, 1, 3);
        let mut expected = Surface::new(256, 256);
        sprites.sprites[&29].draw(&mut expected, (top_x - view.cell_width() / 2, top_y), 2, 1);

        assert!(drawn.pixels.iter().any(|&p| p != 0), "nothing drawn");
        assert!(drawn.pixels == expected.pixels);
    }

    #[test]
    fn sloped_tiles_shift_by_their_class() {
        let Ok(assets) = Assets::from_env("ENGLISH") else { return };
        let sprites = RoadSprites::load(&assets, &[29, 31, 32, 11201, 11202]).unwrap();
        let shift = |id: u16, rotation| sprites.sprites[&id].slope_shift(2, rotation);
        // Altitude step 4 at zoom 2; positive is down.
        assert_eq!([0, 1, 2, 3].map(|r| shift(29, r)), [0, 0, 0, 0]);
        assert_eq!([0, 1, 2, 3].map(|r| shift(32, r)), [8, 8, 0, 0]);
        assert_eq!([0, 1, 2, 3].map(|r| shift(11202, r)), [4, 4, 0, 0]);
        assert_eq!([0, 1, 2, 3].map(|r| shift(31, r)), [0, 0, -8, -8]);
        assert_eq!([0, 1, 2, 3].map(|r| shift(11201, r)), [0, 0, -4, -4]);

        // Tile 32 at rotation 0 draws two altitude units below a flat tile's anchor.
        let view = view(0);
        let mut drawn = Surface::new(256, 256);
        sprites.draw(&mut drawn, &view, (1, 1), road(32, 0, 3));
        let (top_x, top_y) = view.project(1, 1, 3);
        let mut expected = Surface::new(256, 256);
        sprites.sprites[&32].draw(&mut expected, (top_x - view.cell_width() / 2, top_y + 8), 2, 0);
        assert!(drawn.pixels.iter().any(|&p| p != 0), "nothing drawn");
        assert!(drawn.pixels == expected.pixels);
    }

    #[test]
    fn tile_without_sprites_draws_nothing() {
        let Some(sprites) = sprites() else { return };
        let mut screen = Surface::new(256, 256);
        sprites.draw(&mut screen, &view(0), (1, 1), road(11600, 0, 0));
        sprites.draw(&mut screen, &view(0), (1, 1), road(0, 0, 0));
        assert!(screen.pixels.iter().all(|&p| p == 0));
    }

    #[test]
    fn draw_cell_draws_only_occupied_cells() {
        let Some(sprites) = sprites() else { return };
        let view = view(0);
        let mut tiles = CellMap::new(2, 2, None);
        tiles.set(1, 0, Some(road(29, 0, 0)));

        let mut drawn = Surface::new(256, 256);
        let draw = draw_cell(&sprites, &tiles, &view);
        for y in 0..2 {
            for x in 0..2 {
                draw(&mut drawn, (x, y), (x, y));
            }
        }

        let mut expected = Surface::new(256, 256);
        sprites.draw(&mut expected, &view, (1, 0), road(29, 0, 0));

        assert!(drawn.pixels.iter().any(|&p| p != 0), "nothing drawn");
        assert!(drawn.pixels == expected.pixels);
    }
}
