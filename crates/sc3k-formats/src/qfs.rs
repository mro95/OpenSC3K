//! EA/Maxis QFS ("RefPack") LZ77 compression, recognised by the `10 FB` header.
//! See `docs/formats/qfs.md`.

use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    BadHeader,
    Truncated { at: usize },
    BadBackref { at: usize, offset: usize, produced: usize },
    SizeMismatch { expected: usize, produced: usize },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadHeader => write!(f, "not QFS data (missing 10 FB header)"),
            Error::Truncated { at } => write!(f, "compressed stream truncated at byte {at}"),
            Error::BadBackref { at, offset, produced } => write!(
                f,
                "back-reference at byte {at} reaches {offset} bytes back but only {produced} produced"
            ),
            Error::SizeMismatch { expected, produced } => {
                write!(f, "header says {expected} bytes, stream produced {produced}")
            }
        }
    }
}

impl std::error::Error for Error {}

/// True if `data` starts with a QFS header (flag byte with bit 4 set, then `FB`).
pub fn is_qfs(data: &[u8]) -> bool {
    data.len() >= 5 && data[1] == 0xFB && data[0] & 0x3E == 0x10
}

/// Size of the decompressed data announced in the header, as
/// `cRZFastCompression3::GetLengthOfDecompressedData` (SIMBABLD.DLL 0x12059F30).
pub fn decompressed_len(data: &[u8]) -> Result<usize, Error> {
    header(data).map(|(len, _)| len)
}

/// (decompressed length, offset of the first control byte).
fn header(data: &[u8]) -> Result<(usize, usize), Error> {
    if !is_qfs(data) {
        return Err(Error::BadHeader);
    }
    let flags = data[0];
    let width = if flags & 0x80 != 0 { 4 } else { 3 };
    // Bit 0: a compressed-size field of the same width precedes the decompressed size.
    let mut pos = 2 + if flags & 0x01 != 0 { width } else { 0 };
    let field = data.get(pos..pos + width).ok_or(Error::Truncated { at: pos })?;
    let len = field.iter().fold(0usize, |acc, &b| acc << 8 | b as usize);
    pos += width;
    Ok((len, pos))
}

/// The decoder of `cRZFastCompression3::DecompressData` (SIMBABLD.DLL 0x12059EBE), checked
/// against it by `tools/diffcheck/run.py qfs`.
pub fn decompress(data: &[u8]) -> Result<Vec<u8>, Error> {
    let (len, mut pos) = header(data)?;
    let mut out = Vec::with_capacity(len);
    loop {
        let b0 = *data.get(pos).ok_or(Error::Truncated { at: pos })?;
        let byte = |i: usize| data.get(pos + i).map(|&b| b as usize).ok_or(Error::Truncated { at: pos + i });
        // (control length, literal bytes, copy length, copy offset)
        let (ctl, literal, copy, offset) = match b0 {
            0x00..=0x7F => {
                let b1 = byte(1)?;
                let b0 = b0 as usize;
                (2, b0 & 3, ((b0 & 0x1C) >> 2) + 3, ((b0 & 0x60) << 3) + b1 + 1)
            }
            0x80..=0xBF => {
                let (b1, b2) = (byte(1)?, byte(2)?);
                (3, b1 >> 6, (b0 as usize & 0x3F) + 4, ((b1 & 0x3F) << 8) + b2 + 1)
            }
            0xC0..=0xDF => {
                let (b1, b2, b3) = (byte(1)?, byte(2)?, byte(3)?);
                let b0 = b0 as usize;
                (4, b0 & 3, ((b0 & 0x0C) << 6) + b3 + 5, ((b0 & 0x10) << 12) + (b1 << 8) + b2 + 1)
            }
            0xE0..=0xFB => (1, ((b0 as usize & 0x1F) << 2) + 4, 0, 0),
            0xFC..=0xFF => (1, b0 as usize & 3, 0, 0),
        };
        pos += ctl;
        let lit = data.get(pos..pos + literal).ok_or(Error::Truncated { at: pos })?;
        out.extend_from_slice(lit);
        pos += literal;
        if copy > 0 {
            if offset > out.len() {
                return Err(Error::BadBackref { at: pos, offset, produced: out.len() });
            }
            // Overlapping copies repeat the pattern, so copy byte by byte.
            let start = out.len() - offset;
            for i in 0..copy {
                out.push(out[start + i]);
            }
        }
        if b0 >= 0xFC {
            break;
        }
    }
    if out.len() != len {
        return Err(Error::SizeMismatch { expected: len, produced: out.len() });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_and_backrefs() {
        // "abcd" as a 4-byte literal block, then a 2-byte op: 0 literals, copy 5 from 4 back,
        // then stop with 1 trailing literal "!".
        let data = [0x10, 0xFB, 0x00, 0x00, 0x0A, 0xE0, b'a', b'b', b'c', b'd', 0x08, 0x03, 0xFD, b'!'];
        assert_eq!(decompress(&data).unwrap(), b"abcdabcda!");
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(decompress(b"hello"), Err(Error::BadHeader));
        let backref_too_far = [0x10, 0xFB, 0x00, 0x00, 0x03, 0x00, 0x05, 0xFC];
        assert!(matches!(decompress(&backref_too_far), Err(Error::BadBackref { .. })));
    }
}
