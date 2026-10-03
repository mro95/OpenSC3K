//! Bitmap text, as `cGZFont::DrawText` with the palette set up by
//! `cGZFont::SetFontColorAndAntialiasColor`: index 1 is the text colour, indices 2..=15 step
//! linearly towards the anti-alias colour, and index 16 (the anti-alias colour itself) is the
//! colour key, so it is never drawn.

use crate::surface::{from_565, quantize, Rect, Surface};
use sc3k_formats::fbf::{Font, BACKGROUND};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextColors {
    pub text: u32,
    pub antialias: u32,
}

impl TextColors {
    pub const fn new(text: u32, antialias: u32) -> TextColors {
        TextColors { text, antialias }
    }

    /// The colours stored in the font file: ink (palette 1) and anti-alias (palette 16).
    pub fn of_font(font: &Font) -> TextColors {
        TextColors::new(from_565(font.palette[1]), from_565(font.palette[BACKGROUND as usize]))
    }

    /// `cGZFont::SetFontColor`: new ink, keeping the font's anti-alias colour. The game does
    /// this to every system font except the title font (`cSC3App::ctrlmgr_setup`).
    pub fn ink(font: &Font, text: u32) -> TextColors {
        TextColors::new(text, from_565(font.palette[BACKGROUND as usize]))
    }

    /// Colour of palette index `i` (1..=15), quantised like the 16-bit palette entries.
    fn shade(&self, i: u8) -> u32 {
        let step = (i as i32) - 1;
        let channel = |shift: u32| {
            let t = ((self.text >> shift) & 0xFF) as i32;
            let a = ((self.antialias >> shift) & 0xFF) as i32;
            // C integer division (truncates toward zero), as in the original loop.
            (t + (a - t) * step / 16) as u32
        };
        quantize(channel(16) << 16 | channel(8) << 8 | channel(0))
    }
}

/// Draw `text` (Windows-1252 bytes) with its top-left at (x, y); stops at a NUL byte.
pub fn draw_text(surface: &mut Surface, font: &Font, x: i32, y: i32, text: &[u8], colors: TextColors) {
    let clip = surface.area();
    draw_text_clipped(surface, font, x, y, text, colors, clip);
}

/// `draw_text`, with only the pixels inside `clip` drawn.
pub fn draw_text_clipped(surface: &mut Surface, font: &Font, x: i32, y: i32, text: &[u8], colors: TextColors, clip: Rect) {
    let shades: Vec<u32> = (0..=15u8).map(|i| if i == 0 { 0 } else { colors.shade(i) }).collect();
    let mut pen = x;
    for &c in text.iter().take_while(|&&c| c != 0) {
        if c == b'\n' {
            continue;
        }
        let g = font.glyphs[c as usize];
        for row in 0..g.height() {
            for col in 0..g.width() {
                let i = font.pixel(g.left + col, g.top + row);
                if (1..BACKGROUND).contains(&i) && clip.contains(pen + col, y + row) {
                    surface.put(pen + col, y + row, shades[i as usize]);
                }
            }
        }
        pen += g.advance;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shades_step_toward_antialias_colour() {
        let c = TextColors::new(0xFFFFFF, 0x000000);
        assert_eq!(c.shade(1), quantize(0xFFFFFF));
        // step 8 of 16: 255 - 255 * 8 / 16 = 128
        assert_eq!(c.shade(9), quantize(0x808080));
    }
}
