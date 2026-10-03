//! Uncompressed 24-bit Windows bitmaps, as used for the terrain palettes
//! (`Apps/Res/Dirt/PALETTE/*.BMP` and the `LandPalettes` archive).

use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    NotBmp,
    Truncated,
    /// Only 24 bits per pixel without compression is supported.
    Unsupported { bits: u16, compression: u32 },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotBmp => write!(f, "not a BMP file"),
            Error::Truncated => write!(f, "BMP shorter than its header says"),
            Error::Unsupported { bits, compression } => {
                write!(f, "unsupported BMP: {bits} bits per pixel, compression {compression}")
            }
        }
    }
}

impl std::error::Error for Error {}

/// A decoded bitmap: `[r, g, b]` pixels, row-major, top row first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bmp {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 3]>,
}

impl Bmp {
    pub fn parse(data: &[u8]) -> Result<Bmp, Error> {
        if data.len() < 54 || &data[..2] != b"BM" {
            return Err(Error::NotBmp);
        }
        let u32_at = |i: usize| u32::from_le_bytes(data[i..i + 4].try_into().unwrap());
        let offset = u32_at(10) as usize;
        let width = u32_at(18) as i32;
        let height = u32_at(22) as i32;
        let bits = u16::from_le_bytes([data[28], data[29]]);
        let compression = u32_at(30);
        if bits != 24 || compression != 0 || width <= 0 || height == 0 {
            return Err(Error::Unsupported { bits, compression });
        }
        // A positive height is stored bottom row first.
        let (w, h) = (width as usize, height.unsigned_abs() as usize);
        let stride = (w * 3).div_ceil(4) * 4;
        if data.len() < offset + stride * h {
            return Err(Error::Truncated);
        }
        let mut pixels = Vec::with_capacity(w * h);
        for row in 0..h {
            let stored = if height > 0 { h - 1 - row } else { row };
            let line = &data[offset + stored * stride..][..w * 3];
            pixels.extend(line.chunks_exact(3).map(|p| [p[2], p[1], p[0]]));
        }
        Ok(Bmp { width: w as u32, height: h as u32, pixels })
    }

    pub fn get(&self, x: u32, y: u32) -> [u8; 3] {
        self.pixels[(y * self.width + x) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2 × 2 bottom-up bitmap: rows are padded to 4 bytes.
    fn sample() -> Vec<u8> {
        let mut d = vec![0u8; 54];
        d[..2].copy_from_slice(b"BM");
        d[10..14].copy_from_slice(&54u32.to_le_bytes());
        d[18..22].copy_from_slice(&2i32.to_le_bytes());
        d[22..26].copy_from_slice(&2i32.to_le_bytes());
        d[28..30].copy_from_slice(&24u16.to_le_bytes());
        // Bottom row: blue, green. Top row: red, white. Stored as BGR.
        d.extend_from_slice(&[255, 0, 0, 0, 255, 0, 0, 0]);
        d.extend_from_slice(&[0, 0, 255, 255, 255, 255, 0, 0]);
        d
    }

    #[test]
    fn bottom_up_rows() {
        let b = Bmp::parse(&sample()).unwrap();
        assert_eq!((b.width, b.height), (2, 2));
        assert_eq!(b.get(0, 0), [255, 0, 0]);
        assert_eq!(b.get(1, 0), [255, 255, 255]);
        assert_eq!(b.get(0, 1), [0, 0, 255]);
        assert_eq!(b.get(1, 1), [0, 255, 0]);
    }

    #[test]
    fn rejects_other_formats() {
        assert_eq!(Bmp::parse(b"nope").unwrap_err(), Error::NotBmp);
        let mut d = sample();
        d[28] = 8;
        assert!(matches!(Bmp::parse(&d), Err(Error::Unsupported { bits: 8, .. })));
    }
}
