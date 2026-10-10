//! Tree sprites (`cSC3Flora` occupants). See `docs/render/flora.md`.
//!
//! An occupant ID resolves in three steps, each looked up first in the chosen flora set's
//! archive (`Res/Sprites/FloraSets/<set id in hex>/`, registered by
//! `cSC3CitySchemeMgr::SetFloraSet`, libSimInit Ghidra 0x44990), then in the base files:
//! 1. the occupant record (`OccupantAttribs.IXF`), whose property 0x67 names
//! 2. the sprite attributes (`CSATTRIB.IXF`), which list one sprite per zoom and rotation
//! 3. in a sprite archive (`00000009_Landscape.DAT`).
//!
//! Set 0, the default, registers no directory and uses the base files alone.

use crate::occupants::{Image, load_occupant};
use crate::terrain::View;
use sc3k_assets::Assets;
use sc3k_formats::ixf::Tgi;
use sc3k_formats::occupant::{GROUP_OCCUPANT, TYPE_OCCUPANT};
use sc3k_sim::cellmap::CellMap;
use sc3k_sim::flora::{Flora, OCCUPANTS};
use sc3k_ui::Surface;
use std::collections::HashMap;

/// Base archives, after the flora set's.
const BASE: [&str; 3] =
    ["Res/Occupant/OccupantAttribs.IXF", "Res/Sprites/CSATTRIB.IXF", "Res/Sprites/00000009_Landscape.DAT"];
/// Sprites per zoom in a flora attribute record: one per rotation.
const ROTATIONS: usize = 4;


/// The sprites of every flora occupant of one flora set.
pub struct FloraSprites {
    /// Occupant ID to its images, indexed `zoom · 4 + rotation`.
    images: HashMap<u16, Vec<Option<Image>>>,
}

impl FloraSprites {
    /// Load the occupants of [`OCCUPANTS`] for flora set `set` (an `SC3CityScheme.ini`
    /// `[FloraSets]` key).
    pub fn load(assets: &Assets, set: u32) -> Result<FloraSprites, String> {
        let mut archives = Vec::new();
        if set != 0 {
            let dir = format!("Res/Sprites/FloraSets/{set:x}");
            archives.extend(assets.archives_in(&dir).map_err(|e| e.to_string())?);
        }
        for path in BASE {
            archives.push(assets.archive(path).map_err(|e| e.to_string())?);
        }
        let find = |tgi: Tgi| archives.iter().find_map(|a| a.get(tgi));
        let mut images = HashMap::new();
        for &id in OCCUPANTS.iter().flatten().filter(|&&id| id != 0) {
            if images.contains_key(&id) {
                continue;
            }
            let key = Tgi { type_id: TYPE_OCCUPANT, group_id: GROUP_OCCUPANT, instance_id: id as u32 };
            images.insert(id, load_occupant(&find, key)?);
        }
        Ok(FloraSprites { images })
    }

    /// Draw the tree standing on map cell `cell`, drawn at draw-grid cell (i, j).
    ///
    /// The cell map anchors a cell's sprites at the left of the cell's bounding box and the
    /// height of its top corner, raised by the occupant's altitude
    /// (`GetCityPixelRectForSpriteDrawGrid`, libSimSpr 0x9AE80); `SprAttDraw` (0x7C724)
    /// then places the image by its [`ImageInfo`].
    pub fn draw(&self, screen: &mut Surface, view: &View, (i, j): (u32, u32), flora: Flora) {
        let Some(images) = self.images.get(&flora.occupant) else { return };
        let k = view.zoom as usize * ROTATIONS + view.rotation as usize;
        let Some(Some(img)) = images.get(k) else { return };
        let (top_x, top_y) = view.project(i as i32, j as i32, flora.altitude);
        let x = top_x - view.cell_width() / 2;
        let clip = screen.area();
        screen.blit_clip(&img.sprite, img.sprite.area(), x - img.info.left as i32, top_y - img.info.up as i32, clip);
    }
}

/// The tree sprites of every `[FloraSets]` entry of `SC3CityScheme.ini`, by key.
pub fn load_all(assets: &Assets) -> Result<HashMap<u32, FloraSprites>, String> {
    let ini = assets.sys_ini("SC3CityScheme.ini").map_err(|e| e.to_string())?;
    let section = ini.section("FloraSets").ok_or("SC3CityScheme.ini has no [FloraSets]")?;
    let mut out = HashMap::new();
    for (key, _) in section {
        let set = sc3k_formats::ini::parse_u32(key).ok_or_else(|| format!("SC3CityScheme.ini [FloraSets] {key}"))?;
        out.insert(set, FloraSprites::load(assets, set)?);
    }
    Ok(out)
}

/// Draw `flora` (one entry per map cell) with `sprites`, as a [`TerrainScene::draw_with`]
/// hook.
///
/// [`TerrainScene::draw_with`]: crate::terrain::TerrainScene::draw_with
pub fn draw_cell<'a>(
    sprites: &'a FloraSprites,
    flora: &'a CellMap<Option<Flora>>,
    view: &View,
) -> impl Fn(&mut Surface, (u32, u32), (u32, u32)) + 'a {
    let view = *view;
    move |screen, draw, (x, y)| {
        if let Some(f) = flora.get(x, y) {
            sprites.draw(screen, &view, draw, f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_set_has_every_tree() {
        let Ok(assets) = Assets::from_env("ENGLISH") else { return };
        let sets = load_all(&assets).unwrap();
        assert_eq!(sets.len(), 5);
        for (set, sprites) in &sets {
            for (id, images) in &sprites.images {
                assert_eq!(images.len(), 20, "set {set:x}, occupant {id:#X}");
                for (k, img) in images.iter().enumerate() {
                    let img = img.as_ref().unwrap_or_else(|| panic!("set {set:x}, occupant {id:#X}, sprite {k}"));
                    assert_eq!(img.info.left as i32 + img.info.right as i32, img.sprite.width);
                    assert_eq!(img.info.up as i32 + img.info.down as i32, img.sprite.height);
                }
            }
        }
    }
}
