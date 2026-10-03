//! `.FBF` bitmap fonts (`Res/Text/<LANG>/*.FBF`), loaded by `cGZFont::LoadFromFile`.
//! See `docs/formats/fbf.md`.

use std::fmt;

const HEADER_LEN: usize = 16;
pub const GLYPHS: usize = 256;
const GLYPH_LEN: usize = 20;

#[derive(Debug)]
pub enum Error {
    Truncated { need: usize, have: usize },
    UnsupportedColorType(u32),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Truncated { need, have } => write!(f, "font file needs {need} bytes, has {have}"),
            Error::UnsupportedColorType(t) => write!(f, "unsupported font color type {t}"),
        }
    }
}

impl std::error::Error for Error {}

/// One character cell in the atlas. `left..right` × `top..bottom` is the cell that gets
/// blitted; the pen then moves by `advance` (usually one pixel more than the cell width).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Glyph {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub advance: i32,
}

impl Glyph {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

/// An 8-bit paletted glyph atlas. Index 1 is the ink colour, 2..=15 are anti-aliasing steps
/// towards index 16, which is the background (and the colour key: it is never drawn).
#[derive(Clone, Debug)]
pub struct Font {
    pub width: u32,
    pub height: u32,
    /// `width * height` palette indices.
    pub pixels: Vec<u8>,
    /// The palette stored in the file (RGB565). The game overwrites entries 1..=16 from the
    /// text and anti-alias colours at draw time, so only the indices matter.
    pub palette: [u16; 256],
    pub glyphs: [Glyph; GLYPHS],
}

/// Palette index of the background, which is also the colour key.
pub const BACKGROUND: u8 = 16;

impl Font {
    pub fn parse(data: &[u8]) -> Result<Font, Error> {
        let need = |n: usize| {
            if data.len() < n {
                Err(Error::Truncated { need: n, have: data.len() })
            } else {
                Ok(())
            }
        };
        need(HEADER_LEN)?;
        let word = |i: usize| u32::from_le_bytes(data[i * 4..i * 4 + 4].try_into().unwrap());
        let (color_type, width, height) = (word(0), word(1), word(2));
        // 1 = 8-bit paletted; other values are 16-bit atlases, which no shipped font uses.
        if color_type != 1 {
            return Err(Error::UnsupportedColorType(color_type));
        }
        let atlas = width as usize * height as usize;
        let pal_at = HEADER_LEN + atlas;
        let glyph_at = pal_at + 512;
        need(glyph_at + GLYPHS * GLYPH_LEN)?;

        let mut palette = [0u16; 256];
        for (i, p) in palette.iter_mut().enumerate() {
            *p = u16::from_le_bytes([data[pal_at + 2 * i], data[pal_at + 2 * i + 1]]);
        }
        let mut glyphs = [Glyph::default(); GLYPHS];
        for (i, g) in glyphs.iter_mut().enumerate() {
            let at = glyph_at + i * GLYPH_LEN;
            let v = |k: usize| i32::from_le_bytes(data[at + 4 * k..at + 4 * k + 4].try_into().unwrap());
            *g = Glyph { left: v(0), top: v(1), right: v(2), bottom: v(3), advance: v(4) };
        }
        Ok(Font {
            width,
            height,
            pixels: data[HEADER_LEN..pal_at].to_vec(),
            palette,
            glyphs,
        })
    }

    /// Line height: the height of glyph 0's cell (`cGZFont::InitBitmapped`).
    pub fn line_height(&self) -> i32 {
        self.glyphs[0].height()
    }

    /// Sum of advances, as `cGZFont::GetStringWidth` (character 0 ends the string).
    pub fn text_width(&self, text: &[u8]) -> i32 {
        text.iter().take_while(|&&c| c != 0).map(|&c| self.glyphs[c as usize].advance).sum()
    }

    pub fn pixel(&self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return BACKGROUND;
        }
        self.pixels[y as usize * self.width as usize + x as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_font() {
        let (w, h) = (4u32, 2u32);
        let mut data = Vec::new();
        for v in [1u32, w, h, 1] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        data.extend_from_slice(&[16, 1, 1, 16, 16, 2, 16, 16]);
        data.extend_from_slice(&[0; 512]);
        for i in 0..GLYPHS as i32 {
            let g: [i32; 5] = if i == b'A' as i32 { [1, 0, 3, 2, 3] } else { [0, 0, 1, 2, 2] };
            for v in g {
                data.extend_from_slice(&v.to_le_bytes());
            }
        }
        let f = Font::parse(&data).unwrap();
        assert_eq!(f.line_height(), 2);
        assert_eq!(f.glyphs[b'A' as usize].width(), 2);
        assert_eq!(f.text_width(b"AA\0A"), 6);
        assert_eq!(f.pixel(1, 0), 1);
        assert_eq!(f.pixel(9, 9), BACKGROUND);
    }
}
