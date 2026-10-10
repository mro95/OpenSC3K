//! Sprites from `Res/Sprites/*.DAT` (`cSC3DBSegmentSprite`). See `docs/formats/sprite.md`.
//!
//! Each sprite is two IXF records sharing (group, instance): type [`TYPE_DATA`] holds the
//! pixels, type [`TYPE_INFO`] an 8-byte image-info block. Only three encodings occur in the
//! whole install, and all are QFS-compressed:
//!
//! - span buffers in RGB565 (62,462 sprites) or RGB555 (90): one run of covered pixels per
//!   row, the rest of the row transparent;
//! - 8-bit alpha masks (1,139), 5-bit values 0..=31, paired with a colour sprite of the same
//!   size (instance `…0001` next to the colour sprite's `…0000`).

use crate::qfs;
use std::fmt;

pub const TYPE_DATA: u32 = 0;
pub const TYPE_INFO: u32 = 1;

/// Flags word: the stream is QFS ("LZ2") compressed.
const FLAG_LZ2: u32 = 0x0008_0000;
/// Flags word: the stream is the older "LZ1" codec (never used by shipped sprites).
const FLAG_LZ1: u32 = 0x0000_8000;
/// Flags word: an 8-bit alpha mask rather than colour.
const FLAG_ALPHA8: u32 = 0x1000_0000;

const KIND_BUFFER: u8 = 0;
const KIND_SPAN: u8 = 1;

const HEADER_LEN: usize = 16;
/// `cGZSpanBuffer` data header: u32 size, u16 width, u16 height, u16 4, u16 colour type,
/// u32 transparent colour.
const SPAN_HEADER_LEN: usize = 16;
/// Row-entry flag: the run has no transparent pixels, so it is copied without a key test.
const ROW_SOLID: u16 = 0x8000;

#[derive(Debug)]
pub enum Error {
    Truncated,
    Unsupported { color_type: u8, kind: u8, flags: u32 },
    Qfs(qfs::Error),
    SizeMismatch { expected: usize, found: usize },
    BadRow { row: u32 },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Truncated => write!(f, "sprite record truncated"),
            Error::Unsupported { color_type, kind, flags } => {
                write!(f, "unsupported sprite encoding (colour type {color_type}, kind {kind}, flags {flags:08X})")
            }
            Error::Qfs(e) => write!(f, "{e}"),
            Error::SizeMismatch { expected, found } => {
                write!(f, "sprite header says {expected} bytes, data has {found}")
            }
            Error::BadRow { row } => write!(f, "row {row} runs outside the sprite"),
        }
    }
}

impl std::error::Error for Error {}

impl From<qfs::Error> for Error {
    fn from(e: qfs::Error) -> Self {
        Error::Qfs(e)
    }
}

/// One row of a span sprite: `len` pixels starting at column `x`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    pub x: u16,
    pub len: u16,
    /// Index of the run's first pixel in [`SpanSprite::pixels`].
    pub offset: u32,
    /// No pixel in the run equals the colour key.
    pub solid: bool,
}

/// A colour sprite. Pixels are RGB565 whatever the stored format; RGB555 sprites are
/// widened on load (the game converts them to the screen format the same way).
#[derive(Clone, Debug)]
pub struct SpanSprite {
    pub width: u32,
    pub height: u32,
    /// Colour key (RGB565); key-coloured pixels in non-solid runs are transparent.
    pub key: u16,
    pub rows: Vec<Run>,
    pub pixels: Vec<u16>,
}

impl SpanSprite {
    /// RGB565 pixel at (x, y), or `None` where the sprite is transparent.
    pub fn get(&self, x: u32, y: u32) -> Option<u16> {
        let run = self.rows.get(y as usize)?;
        if x < run.x as u32 || x >= run.x as u32 + run.len as u32 {
            return None;
        }
        let p = self.pixels[(run.offset + x - run.x as u32) as usize];
        (run.solid || p != self.key).then_some(p)
    }
}

/// The [`TYPE_INFO`] record of a sprite: four `i16`, copied into `cSC3ImageInfo` +0x10..+0x16
/// by `cSC3DBSegmentSprite::LoadImageInfo` (libSimSpr Ghidra 0xB260C).
/// `cSC3CitySpriteAttrib::SprAttDraw` (0x7C724) draws a sprite anchored at (x, y) into the
/// rectangle (x − left, y − up) .. (x + right, y + down), so `right + left` is the width and
/// `up + down` the height.
/// Unchecked: no check runs the SIMSPR.DLL readers yet; SprAttDraw has no Windows address
/// known yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageInfo {
    pub left: i16,
    pub up: i16,
    pub right: i16,
    pub down: i16,
}

impl ImageInfo {
    pub fn parse(data: &[u8]) -> Result<ImageInfo, Error> {
        let v = |i: usize| -> Result<i16, Error> {
            Ok(i16::from_le_bytes(data.get(i * 2..i * 2 + 2).ok_or(Error::Truncated)?.try_into().unwrap()))
        };
        Ok(ImageInfo { left: v(0)?, up: v(1)?, right: v(2)?, down: v(3)? })
    }
}

/// An alpha mask: one value per pixel, 0 (transparent) ..= 31 (opaque).
#[derive(Clone, Debug)]
pub struct AlphaMask {
    pub width: u32,
    pub height: u32,
    pub alpha: Vec<u8>,
}

#[derive(Clone, Debug)]
pub enum Sprite {
    Span(SpanSprite),
    Alpha(AlphaMask),
}

impl Sprite {
    pub fn parse(data: &[u8]) -> Result<Sprite, Error> {
        if data.len() < HEADER_LEN + 4 {
            return Err(Error::Truncated);
        }
        let word = |i: usize| u32::from_le_bytes(data[i * 4..i * 4 + 4].try_into().unwrap());
        let (code, flags, width, height) = (word(0), word(1), word(2), word(3));
        let (color_type, kind) = (code as u8, (code >> 8) as u8);
        // After the header: u32 size of the rest of the record, then the QFS stream.
        let stream = &data[HEADER_LEN + 4..];
        let unsupported = Error::Unsupported { color_type, kind, flags };
        if flags & FLAG_LZ2 == 0 || flags & FLAG_LZ1 != 0 {
            return Err(unsupported);
        }
        match (kind, flags & FLAG_ALPHA8 != 0) {
            (KIND_SPAN, false) if color_type == 5 || color_type == 7 => {
                parse_span(&qfs::decompress(stream)?, width, height).map(Sprite::Span)
            }
            (KIND_BUFFER, true) => {
                let alpha = qfs::decompress(stream)?;
                let expected = width as usize * height as usize;
                if alpha.len() != expected {
                    return Err(Error::SizeMismatch { expected, found: alpha.len() });
                }
                Ok(Sprite::Alpha(AlphaMask { width, height, alpha }))
            }
            _ => Err(unsupported),
        }
    }

    pub fn width(&self) -> u32 {
        match self {
            Sprite::Span(s) => s.width,
            Sprite::Alpha(a) => a.width,
        }
    }

    pub fn height(&self) -> u32 {
        match self {
            Sprite::Span(s) => s.height,
            Sprite::Alpha(a) => a.height,
        }
    }

    /// 8-bit RGBA for export. Alpha masks come out as white with the mask as alpha.
    pub fn to_rgba8(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity((self.width() * self.height() * 4) as usize);
        match self {
            Sprite::Span(s) => {
                for y in 0..s.height {
                    for x in 0..s.width {
                        match s.get(x, y) {
                            Some(p) => {
                                let [r, g, b] = crate::image::rgb565(p);
                                out.extend_from_slice(&[r, g, b, 0xFF]);
                            }
                            None => out.extend_from_slice(&[0; 4]),
                        }
                    }
                }
            }
            Sprite::Alpha(a) => {
                for &v in &a.alpha {
                    out.extend_from_slice(&[0xFF, 0xFF, 0xFF, (v.min(31) as u32 * 255 / 31) as u8]);
                }
            }
        }
        out
    }
}

/// `cGZSpanBuffer::ImportFromData`: header, `height` 8-byte row entries
/// {u32 pixel offset, u16 x, u16 count | 0x8000 solid}, then the 16-bit pixels.
fn parse_span(data: &[u8], width: u32, height: u32) -> Result<SpanSprite, Error> {
    if data.len() < SPAN_HEADER_LEN {
        return Err(Error::Truncated);
    }
    let u16_at = |i: usize| u16::from_le_bytes([data[i], data[i + 1]]);
    let size = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
    if size != data.len() {
        return Err(Error::SizeMismatch { expected: size, found: data.len() });
    }
    let (w, h, color_type) = (u16_at(4) as u32, u16_at(6) as u32, u16_at(10));
    if (w, h) != (width, height) {
        return Err(Error::SizeMismatch { expected: (width * height) as usize, found: (w * h) as usize });
    }
    let key = u16_at(12);
    let pixel_base = SPAN_HEADER_LEN + height as usize * 8;
    if data.len() < pixel_base {
        return Err(Error::Truncated);
    }
    let widen = color_type == 5;
    let pixels: Vec<u16> = data[pixel_base..]
        .chunks_exact(2)
        .map(|p| {
            let v = u16::from_le_bytes([p[0], p[1]]);
            if widen { rgb555_to_565(v) } else { v }
        })
        .collect();
    let mut rows = Vec::with_capacity(height as usize);
    for row in 0..height {
        let e = SPAN_HEADER_LEN + row as usize * 8;
        let offset = u32::from_le_bytes(data[e..e + 4].try_into().unwrap());
        let (x, count) = (u16_at(e + 4), u16_at(e + 6));
        let len = count & !ROW_SOLID;
        let in_row = x as u32 + len as u32 <= width;
        let in_data = offset as usize + len as usize <= pixels.len();
        if len > 0 && !(in_row && in_data) {
            return Err(Error::BadRow { row });
        }
        rows.push(Run { x, len, offset, solid: count & ROW_SOLID != 0 });
    }
    let key = if widen { rgb555_to_565(key) } else { key };
    Ok(SpanSprite { width, height, key, rows, pixels })
}

/// Green gains a low bit copied from its top bit, so 555 white stays 565 white.
fn rgb555_to_565(p: u16) -> u16 {
    let g = (p >> 5) & 0x1F;
    (p & 0x7C00) << 1 | (g << 1 | g >> 4) << 5 | p & 0x1F
}

#[cfg(test)]
mod tests {
    use super::*;

    /// QFS stream that stores `data` as literals only.
    fn qfs_literal(data: &[u8]) -> Vec<u8> {
        let mut out = vec![0x10, 0xFB];
        out.extend_from_slice(&(data.len() as u32).to_be_bytes()[1..]);
        for chunk in data.chunks(112) {
            let n = chunk.len() / 4 * 4;
            if n > 0 {
                out.push(0xE0 | ((n - 4) / 4) as u8);
                out.extend_from_slice(&chunk[..n]);
            }
            let rest = &chunk[n..];
            if !rest.is_empty() {
                out.push(0xFC | rest.len() as u8);
                out.extend_from_slice(rest);
                return out;
            }
        }
        out.push(0xFC);
        out
    }

    fn record(code: u32, flags: u32, w: u32, h: u32, payload: &[u8]) -> Vec<u8> {
        let stream = qfs_literal(payload);
        let mut out = Vec::new();
        for v in [code, flags, w, h, stream.len() as u32 + 4] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&stream);
        out
    }

    #[test]
    fn decodes_span_rows_and_key() {
        // 3x2: row 0 = one solid pixel at x 1; row 1 = three pixels, the middle one keyed.
        let mut span = Vec::new();
        let pixels: [u16; 4] = [0x1234, 0xFFFF, 0xF81F, 0x0001];
        let size = 16 + 2 * 8 + pixels.len() * 2;
        span.extend_from_slice(&(size as u32).to_le_bytes());
        for v in [3u16, 2, 4, 7] {
            span.extend_from_slice(&v.to_le_bytes());
        }
        span.extend_from_slice(&0xF81Fu32.to_le_bytes());
        for (offset, x, count) in [(0u32, 1u16, 1u16 | ROW_SOLID), (1, 0, 3)] {
            span.extend_from_slice(&offset.to_le_bytes());
            span.extend_from_slice(&x.to_le_bytes());
            span.extend_from_slice(&count.to_le_bytes());
        }
        for p in pixels {
            span.extend_from_slice(&p.to_le_bytes());
        }
        let Sprite::Span(s) = Sprite::parse(&record(0x107, FLAG_LZ2, 3, 2, &span)).unwrap() else {
            panic!("expected a span sprite");
        };
        assert_eq!([s.get(0, 0), s.get(1, 0), s.get(2, 0)], [None, Some(0x1234), None]);
        assert_eq!([s.get(0, 1), s.get(1, 1), s.get(2, 1)], [Some(0xFFFF), None, Some(0x0001)]);
    }

    #[test]
    fn decodes_alpha_mask() {
        let mask = [0u8, 31, 16, 8];
        let Sprite::Alpha(a) = Sprite::parse(&record(0x3, FLAG_LZ2 | FLAG_ALPHA8, 2, 2, &mask)).unwrap()
        else {
            panic!("expected an alpha mask");
        };
        assert_eq!(a.alpha, mask);
    }

    #[test]
    fn widens_555() {
        assert_eq!(rgb555_to_565(0x7FFF), 0xFFFF);
        assert_eq!(rgb555_to_565(0x7C1F), 0xF81F);
    }
}
