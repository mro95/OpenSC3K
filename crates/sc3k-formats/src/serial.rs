//! Serial records: the sequential field stream behind every saved city layer
//! (`cIGZDBSerialRecord`). See `docs/formats/save.md`.
//!
//! Fields are little-endian and unaligned. A layer's record starts with the version header
//! that `cSLAutoSaveRecordVersionInfo` reads (sc3u_demo 0x0823D924).

use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Truncated { at: usize, want: usize },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Truncated { at, want } => write!(f, "serial record ends at byte {at}, {want} more wanted"),
        }
    }
}

impl std::error::Error for Error {}

/// The guard after the version header.
pub const GUARD: u32 = 0xDEAD_BEEF;

/// A reader over one record's bytes.
#[derive(Clone)]
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Reader<'a> {
        Reader { data, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    /// `GetFieldVoid(dest, n)` (vtable 0x20): `n` raw bytes.
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let have = self.data.len() - self.pos;
        if n > have {
            return Err(Error::Truncated { at: self.data.len(), want: n - have });
        }
        let out = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        Ok(self.bytes(N)?.try_into().unwrap())
    }

    /// `GetFieldUint8` (0x1C).
    pub fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.array::<1>()?[0])
    }

    /// `GetFieldUint16` (0x2C).
    pub fn u16(&mut self) -> Result<u16, Error> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    /// `GetFieldUint32` (0x3C).
    pub fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    /// `GetFieldFloat32` (0x44).
    pub fn f32(&mut self) -> Result<f32, Error> {
        Ok(f32::from_le_bytes(self.array()?))
    }

    /// `GetFieldString` (0x5C): a `u32` length, then that many bytes, no terminator.
    pub fn string(&mut self) -> Result<&'a [u8], Error> {
        let n = self.u32()? as usize;
        self.bytes(n)
    }

    /// `GetFieldResKey` (0x60): type, group, instance.
    pub fn key(&mut self) -> Result<crate::ixf::Tgi, Error> {
        Ok(crate::ixf::Tgi { type_id: self.u32()?, group_id: self.u32()?, instance_id: self.u32()? })
    }
}

/// The header of a layer's record, as `cSLAutoSaveRecordVersionInfo` reads it:
///
/// ```text
/// u16  version
/// u8   flags     bit 0: the record has "<Layer> Start" / "<Layer> End" marker strings
///                bit 1: one more byte follows (read and ignored)
/// u8   (only with flags bit 1)
/// u32  0xDEADBEEF
/// ```
///
/// A record without the guard is from before the header existed: the reader starts over
/// at byte 0 with version 0 and no markers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Version {
    pub version: u16,
    pub markers: bool,
}

impl Version {
    pub fn read(r: &mut Reader) -> Version {
        let start = r.clone();
        let header = (|| {
            let version = r.u16()?;
            let flags = r.u8()?;
            if flags & 2 != 0 {
                r.u8()?;
            }
            Ok::<_, Error>((version, flags, r.u32()?))
        })();
        match header {
            Ok((version, flags, GUARD)) => Version { version, markers: flags & 1 != 0 },
            _ => {
                *r = start;
                Version { version: 0, markers: false }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_header_and_fields() {
        let mut data = vec![3, 0, 3, 0, 0xEF, 0xBE, 0xAD, 0xDE];
        data.extend(4u32.to_le_bytes());
        data.extend(b"Test");
        data.extend(3.266f32.to_le_bytes());
        let mut r = Reader::new(&data);
        assert_eq!(Version::read(&mut r), Version { version: 3, markers: true });
        assert_eq!(r.string().unwrap(), b"Test");
        assert_eq!(r.f32().unwrap(), 3.266);
        assert_eq!(r.u8(), Err(Error::Truncated { at: data.len(), want: 1 }));

        // No guard: start over, version 0.
        let mut r = Reader::new(&[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(Version::read(&mut r), Version { version: 0, markers: false });
        assert_eq!(r.pos(), 0);
    }
}
