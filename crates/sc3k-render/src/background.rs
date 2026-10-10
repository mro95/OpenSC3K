//! The texture behind the map: `Res/Sprites/BACK<zoom>.BMP`, one per zoom
//! (`cSC3CityViewIso::SetBackgroundBitmapForZoom`, libSimSpr Ghidra 0xAFC8C, builds the name
//! by replacing the `n` of `res/sprites/backn.bmp` with the zoom digit).
//! `cSC3CitySpriteCellMap::DrawBackground` (0x8BF7C) tiles it, offset by the view origin so
//! that it scrolls with the map.
//! Unchecked: no Windows address known yet.

use crate::terrain::{View, MAX_ZOOM};
use sc3k_assets::Assets;
use sc3k_formats::bmp::Bmp;
use sc3k_ui::surface::quantize;
use sc3k_ui::Surface;

struct Tile {
    width: i32,
    height: i32,
    /// `0x00RRGGBB` through 5:6:5, like the original's 16-bit buffer.
    pixels: Vec<u32>,
}

pub struct Background {
    tiles: Vec<Tile>,
}

impl Background {
    pub fn load(assets: &Assets) -> Result<Background, String> {
        let tiles = (0..=MAX_ZOOM)
            .map(|z| {
                let path = format!("Res/Sprites/BACK{z}.BMP");
                let data = assets.file(&path).map_err(|e| e.to_string())?;
                let bmp = Bmp::parse(&data).map_err(|e| format!("{path}: {e}"))?;
                let pixels = bmp
                    .pixels
                    .iter()
                    .map(|&[r, g, b]| quantize((r as u32) << 16 | (g as u32) << 8 | b as u32))
                    .collect();
                Ok(Tile { width: bmp.width as i32, height: bmp.height as i32, pixels })
            })
            .collect::<Result<_, String>>()?;
        Ok(Background { tiles })
    }

    /// Fill the screen with the texture of the view's zoom.
    pub fn draw(&self, screen: &mut Surface, view: &View) {
        let tile = &self.tiles[view.zoom.min(MAX_ZOOM) as usize];
        let width = screen.width as usize;
        for y in 0..screen.height {
            let ty = (y - view.origin_y).rem_euclid(tile.height);
            let src = &tile.pixels[(ty * tile.width) as usize..((ty + 1) * tile.width) as usize];
            let row = &mut screen.pixels[y as usize * width..(y as usize + 1) * width];
            let mut tx = (-view.origin_x).rem_euclid(tile.width) as usize;
            for d in row.iter_mut() {
                *d = src[tx];
                tx += 1;
                if tx == src.len() {
                    tx = 0;
                }
            }
        }
    }
}
