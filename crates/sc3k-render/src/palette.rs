//! `cSC3DirtClodColorLightTable`: colour tables with one row per height and one entry per
//! light level. See `docs/render/terrain.md`, "Palettes".

use sc3k_assets::Assets;
use sc3k_formats::bmp::Bmp;
use sc3k_formats::ini::parse_u32_list;
use sc3k_formats::ixf::Tgi;
use sc3k_ui::surface::{from_565, to_565};
use std::collections::HashMap;

/// Archive with the landscape palettes (palette 4).
const LAND_PALETTES: &str = "Res/Dirt/PALETTE/455A72B1_LandPalettes.IXF";
/// Type of the palette images in it.
const TYPE_PALETTE: u32 = 0x62B9_DA24;

/// A loaded colour table. `Init_FromBuffer` reads it through a 16-bit buffer, so every entry
/// is stored as RGB565 and expanded again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColorTable {
    width: u32,
    rows: Vec<[u8; 3]>,
}

impl ColorTable {
    /// `Init_FromBuffer` (libSimDirt 0x44DB8, SIMDIRT.DLL 0x10012342): pixel (x, y) becomes
    /// row y, entry x.
    /// Unchecked: no check feeds it a buffer yet.
    pub fn from_bmp(bmp: &Bmp) -> ColorTable {
        let rows = bmp
            .pixels
            .iter()
            .map(|&[r, g, b]| {
                let c = from_565(to_565((r as u32) << 16 | (g as u32) << 8 | b as u32));
                [(c >> 16) as u8, (c >> 8) as u8, c as u8]
            })
            .collect();
        ColorTable { width: bmp.width, rows }
    }

    pub fn row_count(&self) -> usize {
        self.rows.len() / self.width as usize
    }

    /// `GetColorInNative16Bit(palette, row, light)`. Out-of-range indices are clamped; the
    /// original does not check them.
    pub fn get(&self, row: usize, light: usize) -> [u8; 3] {
        let row = row.min(self.row_count() - 1);
        let light = light.min(self.width as usize - 1);
        self.rows[row * self.width as usize + light]
    }
}

/// The fixed dirt palettes (`RegenPalette` 6–8), loaded with `Init_FromFile`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirtPalettes {
    /// Palette 6, `palwater.bmp`: water colour by depth.
    pub water: ColorTable,
    /// Palettes 7 and 8, `paledgel.bmp` and `paledged.bmp`: the map-edge skirts. Rows 0–255
    /// are water by depth, rows from 0x100 soil by depth below the surface.
    pub edge_light: ColorTable,
    pub edge_dark: ColorTable,
}

impl DirtPalettes {
    pub fn load(assets: &Assets) -> Result<DirtPalettes, String> {
        let load = |name: &str| -> Result<ColorTable, String> {
            let path = format!("Res/Dirt/PALETTE/{name}");
            let data = assets.file(&path).map_err(|e| e.to_string())?;
            let bmp = Bmp::parse(&data).map_err(|e| format!("{path}: {e}"))?;
            Ok(ColorTable::from_bmp(&bmp))
        };
        Ok(DirtPalettes {
            water: load("PALWATER.BMP")?,
            edge_light: load("PALEDGEL.BMP")?,
            edge_dark: load("PALEDGED.BMP")?,
        })
    }
}

/// The landscape palettes by `LandScapes` scheme key. The scheme entry's second
/// group/instance pair names the palette image (`SetAltitudePalette`).
pub fn load_land_palettes(assets: &Assets) -> Result<HashMap<u32, ColorTable>, String> {
    let ini = assets.sys_ini("SC3CityScheme.ini").map_err(|e| e.to_string())?;
    let archive = assets.archive(LAND_PALETTES).map_err(|e| e.to_string())?;
    let section = ini.section("LandScapes").ok_or("SC3CityScheme.ini has no [LandScapes]")?;
    let mut out = HashMap::new();
    for (key, value) in section {
        let bad = || format!("SC3CityScheme.ini [LandScapes] {key}: {value}");
        let key = sc3k_formats::ini::parse_u32(key).ok_or_else(bad)?;
        let f = parse_u32_list(value).filter(|f| f.len() >= 4).ok_or_else(bad)?;
        let tgi = Tgi { type_id: TYPE_PALETTE, group_id: f[2], instance_id: f[3] };
        let data = archive
            .get(tgi)
            .ok_or_else(|| format!("{LAND_PALETTES}: no palette {:08X}/{:08X}", f[2], f[3]))?;
        let bmp = Bmp::parse(data).map_err(|e| format!("palette {:08X}/{:08X}: {e}", f[2], f[3]))?;
        out.insert(key, ColorTable::from_bmp(&bmp));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_lookup_and_565() {
        let bmp = Bmp { width: 2, height: 2, pixels: vec![[255, 255, 255], [7, 3, 9], [1, 2, 3], [0, 0, 0]] };
        let t = ColorTable::from_bmp(&bmp);
        assert_eq!(t.row_count(), 2);
        assert_eq!(t.get(0, 0), [255, 255, 255]);
        // 565 truncation: 7 -> 0, 3 -> 0, 9 -> 8 (expanded as 8 | 8 >> 5).
        assert_eq!(t.get(0, 1), [0, 0, 8]);
        assert_eq!(t.get(9, 9), [0, 0, 0], "clamped to the last row and entry");
    }

    #[test]
    fn land_palettes_from_install() {
        let Ok(assets) = Assets::from_env("ENGLISH") else { return };
        let p = load_land_palettes(&assets).unwrap();
        let t = &p[&0];
        assert_eq!(t.row_count(), 512);
        // The water line row is lighter towards the high light entries.
        let sum = |c: [u8; 3]| c.iter().map(|&v| v as u32).sum::<u32>();
        assert!(sum(t.get(250, 31)) > sum(t.get(250, 0)));
        let d = DirtPalettes::load(&assets).unwrap();
        assert_eq!((d.water.row_count(), d.edge_light.row_count(), d.edge_dark.row_count()), (256, 512, 512));
    }
}
