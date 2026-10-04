//! Software 2D compositing. The original draws into a 16-bit RGB565 back buffer; here pixels
//! are `0x00RRGGBB`, but every colour that is drawn is first quantised through RGB565, so the
//! output matches what the original could show.

use sc3k_formats::image::{rgb565, Image};
use sc3k_formats::sprite::{AlphaMask, SpanSprite};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }

    pub fn right(&self) -> i32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }

    pub fn offset(&self, dx: i32, dy: i32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.w, self.h)
    }

    pub fn intersect(&self, o: &Rect) -> Rect {
        let (x, y) = (self.x.max(o.x), self.y.max(o.y));
        let (r, b) = (self.right().min(o.right()), self.bottom().min(o.bottom()));
        Rect::new(x, y, (r - x).max(0), (b - y).max(0))
    }

    pub fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }
}

/// `0x00RRGGBB` from an RGB565 pixel.
pub fn from_565(p: u16) -> u32 {
    let [r, g, b] = rgb565(p);
    (r as u32) << 16 | (g as u32) << 8 | b as u32
}

/// RGB565 value of a `0x00RRGGBB` colour (truncating, as the game's colour conversion does).
pub fn to_565(rgb: u32) -> u16 {
    let (r, g, b) = ((rgb >> 16) & 0xFF, (rgb >> 8) & 0xFF, rgb & 0xFF);
    ((r >> 3) << 11 | (g >> 2) << 5 | b >> 3) as u16
}

/// A colour as the 16-bit back buffer would hold it.
pub fn quantize(rgb: u32) -> u32 {
    from_565(to_565(rgb))
}

/// An image prepared for blitting: pixels equal to the colour key are transparent.
#[derive(Clone)]
pub struct Sprite {
    pub width: i32,
    pub height: i32,
    /// `0x00RRGGBB`, or `TRANSPARENT`.
    pixels: Vec<u32>,
}

const TRANSPARENT: u32 = 0xFF00_0000;

impl Sprite {
    pub fn new(image: &Image, color_key: Option<u16>) -> Sprite {
        let pixels = image
            .pixels
            .iter()
            .map(|&p| if Some(p) == color_key { TRANSPARENT } else { from_565(p) })
            .collect();
        Sprite { width: image.width as i32, height: image.height as i32, pixels }
    }

    /// A span-buffer sprite; pixels outside each row's run, and key-coloured pixels in runs
    /// that are not solid, are transparent.
    pub fn from_span(span: &SpanSprite) -> Sprite {
        let (w, h) = (span.width as i32, span.height as i32);
        let mut pixels = vec![TRANSPARENT; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                if let Some(p) = span.get(x as u32, y as u32) {
                    pixels[(y * w + x) as usize] = from_565(p);
                }
            }
        }
        Sprite { width: w, height: h, pixels }
    }

    pub fn area(&self) -> Rect {
        Rect::new(0, 0, self.width, self.height)
    }

    /// The sprite scaled by `num / den`, nearest pixel.
    pub fn scaled(&self, num: i32, den: i32) -> Sprite {
        let (w, h) = ((self.width * num / den).max(1), (self.height * num / den).max(1));
        let mut pixels = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            let sy = (y * den / num).min(self.height - 1);
            for x in 0..w {
                let sx = (x * den / num).min(self.width - 1);
                pixels.push(self.pixels[(sy * self.width + sx) as usize]);
            }
        }
        Sprite { width: w, height: h, pixels }
    }

    /// True if (x, y) is outside the sprite or a colour-keyed pixel.
    pub fn is_transparent(&self, x: i32, y: i32) -> bool {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return true;
        }
        self.pixels[(y * self.width + x) as usize] == TRANSPARENT
    }
}

pub struct Surface {
    pub width: i32,
    pub height: i32,
    /// `0x00RRGGBB`, row-major.
    pub pixels: Vec<u32>,
}

impl Surface {
    pub fn new(width: i32, height: i32) -> Surface {
        Surface { width, height, pixels: vec![0; (width * height) as usize] }
    }

    pub fn fill(&mut self, rgb: u32) {
        self.pixels.fill(quantize(rgb));
    }

    pub fn put(&mut self, x: i32, y: i32, rgb: u32) {
        if x >= 0 && y >= 0 && x < self.width && y < self.height {
            self.pixels[(y * self.width + x) as usize] = rgb;
        }
    }

    pub fn area(&self) -> Rect {
        Rect::new(0, 0, self.width, self.height)
    }

    /// `cGZBuffer::Fill`.
    pub fn fill_rect(&mut self, r: Rect, rgb: u32) {
        let r = r.intersect(&self.area());
        let rgb = quantize(rgb);
        for y in r.y..r.bottom() {
            let row = (y * self.width) as usize;
            self.pixels[row + r.x as usize..row + r.right() as usize].fill(rgb);
        }
    }

    /// `cGZBuffer::DrawLine` for the horizontal and vertical lines the UI draws; both end
    /// points are included.
    pub fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, rgb: u32) {
        let (l, r) = (x0.min(x1), x0.max(x1));
        let (t, b) = (y0.min(y1), y0.max(y1));
        if x0 == x1 || y0 == y1 {
            self.fill_rect(Rect::new(l, t, r - l + 1, b - t + 1), rgb);
        }
    }

    /// Copy `src` (a rectangle of `sprite`) to (x, y), skipping transparent pixels.
    pub fn blit(&mut self, sprite: &Sprite, src: Rect, x: i32, y: i32) {
        self.blit_clip(sprite, src, x, y, self.area());
    }

    /// [`blit`](Self::blit), drawing only inside `clip`.
    pub fn blit_clip(&mut self, sprite: &Sprite, src: Rect, x: i32, y: i32, clip: Rect) {
        let Some((src, dst)) = self.clip_copy(sprite, src, x, y, clip) else { return };
        for row in 0..dst.h {
            let s = ((src.y + row) * sprite.width + src.x) as usize;
            let d = ((dst.y + row) * self.width + dst.x) as usize;
            let w = dst.w as usize;
            for (out, &p) in self.pixels[d..d + w].iter_mut().zip(&sprite.pixels[s..s + w]) {
                if p != TRANSPARENT {
                    *out = p;
                }
            }
        }
    }

    /// The parts of a copy of `src` to (x, y) that fall inside the sprite, `clip` and the
    /// surface, as (source, destination) rectangles of equal size.
    fn clip_copy(&self, sprite: &Sprite, src: Rect, x: i32, y: i32, clip: Rect) -> Option<(Rect, Rect)> {
        let valid = src.intersect(&sprite.area());
        let placed = valid.offset(x - src.x, y - src.y);
        let dst = placed.intersect(&clip).intersect(&self.area());
        if dst.is_empty() {
            return None;
        }
        Some((Rect::new(valid.x + dst.x - placed.x, valid.y + dst.y - placed.y, dst.w, dst.h), dst))
    }

    /// `cGZBuffer::BltTiled`: repeat `src` across `dst`, starting at its top-left corner.
    pub fn blit_tiled(&mut self, sprite: &Sprite, src: Rect, dst: Rect, clip: Rect) {
        let src = src.intersect(&sprite.area());
        if src.is_empty() {
            return;
        }
        let clip = clip.intersect(&dst);
        let mut ty = dst.y;
        while ty < dst.bottom() {
            let mut tx = dst.x;
            while tx < dst.right() {
                self.blit_clip(sprite, src, tx, ty, clip);
                tx += src.w;
            }
            ty += src.h;
        }
    }

    /// `cGZSpanBuffer::SpanBufferAlphaBlt`: blend `sprite` onto the surface with a 5-bit
    /// alpha mask of the same size, through the game's `blendTable5Bit`.
    pub fn blit_alpha(&mut self, sprite: &Sprite, mask: &AlphaMask, x: i32, y: i32, clip: Rect) {
        let Some((src, dst)) = self.clip_copy(sprite, sprite.area(), x, y, clip) else { return };
        for row in 0..dst.h {
            for col in 0..dst.w {
                let (sx, sy) = (src.x + col, src.y + row);
                let p = sprite.pixels[(sy * sprite.width + sx) as usize];
                if p == TRANSPARENT {
                    continue;
                }
                let a = mask.alpha.get((sy as u32 * mask.width + sx as u32) as usize).copied().unwrap_or(0);
                let d = ((dst.y + row) * self.width + dst.x + col) as usize;
                self.pixels[d] = from_565(alpha565(to_565(p), to_565(self.pixels[d]), a));
            }
        }
    }

    /// Draw a whole sprite centred on the surface (the title screens).
    pub fn blit_centered(&mut self, sprite: &Sprite) {
        let x = (self.width - sprite.width) / 2;
        let y = (self.height - sprite.height) / 2;
        self.blit(sprite, Rect::new(0, 0, sprite.width, sprite.height), x, y);
    }
}

/// `blendTable5Bit[s][d][a]` (built by `InitAlphaSystem`): `(s*a + d*(32-a)) >> 5`, at most 31.
fn blend5(s: u16, d: u16, a: u16) -> u16 {
    ((s * a + d * (32 - a)) >> 5).min(31)
}

/// `Alpha565`: each channel through the 5-bit table; green uses its top five bits only.
fn alpha565(src: u16, dst: u16, a: u8) -> u16 {
    let a = (a as u16).min(31);
    let r = blend5(src >> 11, dst >> 11, a);
    let g = blend5((src >> 6) & 0x1F, (dst >> 6) & 0x1F, a);
    let b = blend5(src & 0x1F, dst & 0x1F, a);
    r << 11 | g << 6 | b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantizes_through_565() {
        assert_eq!(to_565(0xFF00FF), 0xF81F);
        assert_eq!(from_565(0xF81F), 0xFF00FF);
        assert_eq!(quantize(0xFFD778), 0xFFD77B);
    }

    #[test]
    fn blit_skips_key_and_clips() {
        let image = Image { width: 2, height: 2, pixels: vec![0xF81F, 0xFFFF, 0x0000, 0xF81F] };
        let sprite = Sprite::new(&image, Some(0xF81F));
        let mut s = Surface::new(2, 2);
        s.fill(0x123456);
        s.blit(&sprite, Rect::new(0, 0, 2, 2), -1, 0);
        assert_eq!(s.pixels, vec![0xFFFFFF, quantize(0x123456), quantize(0x123456), quantize(0x123456)]);
        assert!(sprite.is_transparent(0, 0) && !sprite.is_transparent(1, 0) && sprite.is_transparent(5, 5));
    }

    #[test]
    fn tiles_and_clips() {
        let image = Image { width: 2, height: 1, pixels: vec![0xFFFF, 0x0000] };
        let sprite = Sprite::new(&image, None);
        let mut s = Surface::new(5, 1);
        s.blit_tiled(&sprite, sprite.area(), Rect::new(0, 0, 5, 1), Rect::new(0, 0, 4, 1));
        assert_eq!(s.pixels, vec![0xFFFFFF, 0, 0xFFFFFF, 0, 0]);
        s.blit_clip(&sprite, Rect::new(1, 0, 1, 1), 4, 0, s.area());
        assert_eq!(s.pixels[4], 0);
    }

    #[test]
    fn alpha_blend_matches_table() {
        // Full alpha is 31/32 of the source, not all of it.
        assert_eq!(alpha565(0xFFFF, 0x0000, 31), 30 << 11 | 30 << 6 | 30);
        assert_eq!(alpha565(0xFFFF, 0x1234, 0), 0x1234 & !0x0020);
    }
}
