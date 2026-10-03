//! Maxis IXF container: the archive format used by `.DAT`, `.IXF`, `.sc3`,
//! `.sct`, `.st3`, `.SNR`, `.bld` and `SC3.cfg`. See `docs/formats/ixf.md`.

use std::fmt;
use std::path::Path;

pub const MAGIC: [u8; 4] = [0xD7, 0x81, 0xC3, 0x80];
const RECORD_LEN: usize = 20;

/// Type / Group / Instance key identifying a resource (the game's `cGZResourceKey`).
///
/// Index records store it as **group, instance, type**
/// (`cGZDBSegmentIndexedFile::WriteNewIndexEntryForKey`). The type is constant per kind of
/// resource (`62B9DA24` UI image, `2026960B` string), the group usually matches the archive's
/// file-name prefix, and the instance is the id the code asks for.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Tgi {
    pub type_id: u32,
    pub group_id: u32,
    pub instance_id: u32,
}

impl fmt::Display for Tgi {
    /// `TTTTTTTT-GGGGGGGG-IIIIIIII`, the usual GZ notation.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:08X}-{:08X}-{:08X}", self.type_id, self.group_id, self.instance_id)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Entry {
    pub tgi: Tgi,
    pub offset: u32,
    pub size: u32,
}

/// How the index table ended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Terminator {
    /// A record of five zero dwords (most files).
    Zero,
    /// A record of five `0xFFFFFFFF` dwords (some localized text files, `BAT.DAT`).
    Ones,
    /// The file ended without a terminator record (empty archives: magic only).
    Eof,
}

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    BadMagic,
    OutOfBounds { record: usize, tgi: Tgi, offset: u32, size: u32, file_len: usize },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io error: {e}"),
            Error::BadMagic => write!(f, "not an IXF container (bad magic)"),
            Error::OutOfBounds { record, tgi, offset, size, file_len } => write!(
                f,
                "record {record} ({tgi}) spans {offset}+{size}, past end of file ({file_len} bytes)"
            ),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

/// An IXF archive held in memory.
pub struct Archive {
    data: Vec<u8>,
    entries: Vec<Entry>,
    terminator: Terminator,
}

impl Archive {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::from_bytes(std::fs::read(path)?)
    }

    pub fn from_bytes(data: Vec<u8>) -> Result<Self, Error> {
        if !is_ixf(&data) {
            return Err(Error::BadMagic);
        }
        let mut entries = Vec::new();
        let mut terminator = Terminator::Eof;
        let mut pos = MAGIC.len();
        while pos + RECORD_LEN <= data.len() {
            let r: [u32; 5] = std::array::from_fn(|i| read_u32(&data, pos + i * 4));
            if r == [0; 5] {
                terminator = Terminator::Zero;
                break;
            }
            if r == [u32::MAX; 5] {
                terminator = Terminator::Ones;
                break;
            }
            let entry = Entry {
                tgi: Tgi { group_id: r[0], instance_id: r[1], type_id: r[2] },
                offset: r[3],
                size: r[4],
            };
            if entry.offset as u64 + entry.size as u64 > data.len() as u64 {
                return Err(Error::OutOfBounds {
                    record: entries.len(),
                    tgi: entry.tgi,
                    offset: entry.offset,
                    size: entry.size,
                    file_len: data.len(),
                });
            }
            entries.push(entry);
            pos += RECORD_LEN;
        }
        Ok(Archive { data, entries, terminator })
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn terminator(&self) -> Terminator {
        self.terminator
    }

    /// Raw bytes of an entry. Entries are bounds-checked at parse time.
    pub fn data(&self, entry: &Entry) -> &[u8] {
        &self.data[entry.offset as usize..(entry.offset + entry.size) as usize]
    }

    pub fn get(&self, tgi: Tgi) -> Option<&[u8]> {
        self.entries.iter().find(|e| e.tgi == tgi).map(|e| self.data(e))
    }
}

pub fn is_ixf(data: &[u8]) -> bool {
    data.starts_with(&MAGIC)
}

fn read_u32(data: &[u8], pos: usize) -> u32 {
    u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(records: &[[u32; 5]], payload: &[u8]) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        for r in records {
            for v in r {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn parses_zero_terminated() {
        // magic(4) + 2 records(40) = payload at 44.
        let data = build(&[[1, 2, 3, 44, 3], [0; 5]], b"abc");
        let a = Archive::from_bytes(data).unwrap();
        assert_eq!(a.terminator(), Terminator::Zero);
        assert_eq!(a.entries().len(), 1);
        // Stored as group, instance, type.
        let tgi = Tgi { group_id: 1, instance_id: 2, type_id: 3 };
        assert_eq!(a.get(tgi), Some(&b"abc"[..]));
        assert_eq!(tgi.to_string(), "00000003-00000001-00000002");
    }

    #[test]
    fn parses_ones_terminated() {
        let a = Archive::from_bytes(build(&[[u32::MAX; 5]], b"")).unwrap();
        assert_eq!(a.terminator(), Terminator::Ones);
        assert!(a.entries().is_empty());
    }

    #[test]
    fn parses_magic_only() {
        let a = Archive::from_bytes(MAGIC.to_vec()).unwrap();
        assert_eq!(a.terminator(), Terminator::Eof);
    }

    #[test]
    fn rejects_bad_magic_and_out_of_bounds() {
        assert!(matches!(Archive::from_bytes(b"RIFF".to_vec()), Err(Error::BadMagic)));
        let data = build(&[[1, 2, 3, 44, 100], [0; 5]], b"abc");
        assert!(matches!(Archive::from_bytes(data), Err(Error::OutOfBounds { .. })));
    }
}
