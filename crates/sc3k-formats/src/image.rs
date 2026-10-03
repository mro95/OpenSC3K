//! Flat 16-bit images used by the UI archives (`Res/UI/Shared/*.ixf`, `MAIN.IXF`, `SYS.IXF`…).
//! See `docs/formats/image.md`.

use crate::qfs;
use std::fmt;

const HEADER_LEN: usize = 24;

#[derive(Debug)]
pub enum Error {
    Truncated,
    UnknownVersion(u32),
    UnknownFormat(u32),
    Qfs(qfs::Error),
    SizeMismatch { width: u32, height: u32, bytes: usize },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Truncated => write!(f, "image record shorter than its header"),
            Error::UnknownVersion(v) => write!(f, "unknown image version {v}"),
            Error::UnknownFormat(v) => write!(f, "unknown image format {v}"),
            Error::Qfs(e) => write!(f, "{e}"),
            Error::SizeMismatch { width, height, bytes } => {
                write!(f, "{width}x{height} image but {bytes} bytes of pixels")
            }
        }
    }
}

impl std::error::Error for Error {}

impl From<qfs::Error> for Error {
    fn from(e: qfs::Error) -> Self {
        Error::Qfs(e)
    }
}

/// Pixel encoding named by the header's format field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// 7: QFS-compressed RGB565. All but one UI image.
    Rgb565,
    /// 3: QFS-compressed 8 bits per pixel (one image, in `DLG.IXF`). No palette is stored
    /// with it, so it is decoded as greyscale.
    Gray8,
}

/// A decoded image with 16-bit RGB565 pixels, row-major, top row first.
#[derive(Clone, Debug)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u16>,
}

impl Image {
    pub fn parse(data: &[u8]) -> Result<Image, Error> {
        if data.len() < HEADER_LEN {
            return Err(Error::Truncated);
        }
        let word = |i: usize| u32::from_le_bytes(data[i * 4..i * 4 + 4].try_into().unwrap());
        let (version, width, height, format) = (word(0), word(2), word(3), word(4));
        if version != 2 {
            return Err(Error::UnknownVersion(version));
        }
        let format = match format {
            7 => Format::Rgb565,
            3 => Format::Gray8,
            _ => return Err(Error::UnknownFormat(format)),
        };
        let raw = qfs::decompress(&data[HEADER_LEN..])?;
        let bpp = if format == Format::Rgb565 { 2 } else { 1 };
        if raw.len() != width as usize * height as usize * bpp {
            return Err(Error::SizeMismatch { width, height, bytes: raw.len() });
        }
        let pixels = match format {
            Format::Rgb565 => raw.chunks_exact(2).map(|p| u16::from_le_bytes([p[0], p[1]])).collect(),
            Format::Gray8 => raw.iter().map(|&v| gray565(v)).collect(),
        };
        Ok(Image { width, height, pixels })
    }

    /// True if `data` looks like an image record (version 2 header).
    pub fn sniff(data: &[u8]) -> bool {
        data.len() >= HEADER_LEN + 2 && data[..4] == [2, 0, 0, 0] && qfs::is_qfs(&data[HEADER_LEN..])
    }

    /// Pixels as 8-bit RGBA, with the low bits filled from the high bits.
    pub fn to_rgba8(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pixels.len() * 4);
        for &p in &self.pixels {
            let [r, g, b] = rgb565(p);
            out.extend_from_slice(&[r, g, b, 0xFF]);
        }
        out
    }
}

fn gray565(v: u8) -> u16 {
    let v = v as u16;
    (v >> 3) << 11 | (v >> 2) << 5 | v >> 3
}

pub fn rgb565(p: u16) -> [u8; 3] {
    let r = (p >> 11) as u8 & 0x1F;
    let g = (p >> 5) as u8 & 0x3F;
    let b = p as u8 & 0x1F;
    [r << 3 | r >> 2, g << 2 | g >> 4, b << 3 | b >> 2]
}
