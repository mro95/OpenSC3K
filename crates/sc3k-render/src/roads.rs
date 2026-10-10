//! Road tile sprites (`cSTNetworkOcc` occupants). See `docs/render/roads.md`.
//!
//! A tile id is the instance of a network occupant key; the chain from there to the pictures
//! is the one in `occupants.rs`. There are no road sets: everything comes from the base files.

use sc3k_assets::Assets;
use sc3k_formats::{
    ixf::Tgi,
    occupant::{GROUP_NETWORK_OCCUPANT, TYPE_NETWORK_OCCUPANT},
    sprite::ImageInfo,
};
use sc3k_ui::Surface;

use crate::occupants::{load_occupant, Image};

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
    images: Vec<Option<Image>>,
}

impl RoadTile {
    /// Load tile `tile`. Fails if the install has no occupant record or sprite attributes for
    /// it, as for some ids in `ROAD_GRND_Set.txt`.
    pub fn load(assets: &Assets, tile: u32) -> Result<Self, String> {
        let mut archives = Vec::new();
        for path in BASE {
            archives.push(assets.archive(path).map_err(|e| e.to_string())?);
        }
        let find = |tgi: Tgi| archives.iter().find_map(|a| a.get(tgi));
        let key = Tgi {
            type_id: TYPE_NETWORK_OCCUPANT,
            group_id: GROUP_NETWORK_OCCUPANT,
            instance_id: tile,
        };

        Ok(Self {
            images: load_occupant(&find, key)?,
        })
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

/// The layer-0 frame index (`cSC3CitySpriteInst::zoom_and_compass_to_frame_no`, exe 0x0821DCF8).
fn frame(zoom: u32, rotation: u32) -> usize {
    const ROTATIONS: usize = 4;
    zoom as usize * ROTATIONS + rotation as usize
}
