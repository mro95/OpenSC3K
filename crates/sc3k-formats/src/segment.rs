//! Compressed DB segments (`cRZDBSegmentCompressed`): how saved cities (`.sc3`), terrains
//! (`.sct`), starter towns and scenarios hold their records. See `docs/formats/save.md`.
//!
//! The file is an ordinary IXF container. One record, [`DESC_KEY`], lists the packed
//! records; each packed record is a whole memory segment (`cRZDBSegmentMemory`) of further
//! records, QFS-compressed. A record is looked up in the container first, then in the
//! packed segments in the order the description lists them (`DoOpenRecord`,
//! sc3u_demo 0x0833E290). The Windows copy of the class is in `GZResourceD.dll`.

use crate::ixf::{Archive, Tgi};
use crate::qfs;
use crate::serial::{self, Reader};
use std::fmt;

/// `kCompressedRecordHeaderKey`: the description record.
pub const DESC_KEY: Tgi = Tgi { type_id: 0x035F_62A4, group_id: 0x035F_62A4, instance_id: 0 };

/// Packing methods: `decoderef` and `cRZFastCompression3`. Both are QFS behind a 4-byte size.
const METHOD_REF: u32 = 0x65;
const METHOD_FAST3: u32 = 0x67;

#[derive(Debug)]
pub enum Error {
    Serial(serial::Error),
    Qfs(qfs::Error),
    BadMethod(u32),
    MissingPacked(Tgi),
    BadDirectory,
    SizeMismatch { expected: usize, produced: usize },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Serial(e) => e.fmt(f),
            Error::Qfs(e) => e.fmt(f),
            Error::BadMethod(m) => write!(f, "unknown packing method {m:#X}"),
            Error::MissingPacked(k) => write!(f, "packed record {k} is not in the container"),
            Error::BadDirectory => write!(f, "packed segment directory out of bounds"),
            Error::SizeMismatch { expected, produced } => {
                write!(f, "packed segment should unpack to {expected} bytes, got {produced}")
            }
        }
    }
}

impl std::error::Error for Error {}

impl From<serial::Error> for Error {
    fn from(e: serial::Error) -> Error {
        Error::Serial(e)
    }
}

/// One unpacked memory segment: its bytes and its records, sorted by key.
struct Packed {
    data: Vec<u8>,
    /// (key, offset), sorted by key; the first of equal keys wins.
    index: Vec<(Tgi, usize)>,
    /// Where the directory starts: the end of the last record.
    end: usize,
}

pub struct Segment<'a> {
    archive: &'a Archive,
    packed: Vec<Packed>,
}

impl<'a> Segment<'a> {
    /// Open `archive`. A container without a description record has no packed records.
    pub fn open(archive: &'a Archive) -> Result<Segment<'a>, Error> {
        let mut packed = Vec::new();
        if let Some(desc) = archive.get(DESC_KEY) {
            for key in read_desc(desc)? {
                let data = archive.get(key).ok_or(Error::MissingPacked(key))?;
                packed.push(unpack(data)?);
            }
        }
        Ok(Segment { archive, packed })
    }

    /// A record's bytes. A packed record has no stored size: it runs to the next record in
    /// the segment (the original lets it run to the end of the segment memory).
    pub fn get(&self, key: Tgi) -> Option<&[u8]> {
        if let Some(d) = self.archive.get(key) {
            return Some(d);
        }
        self.packed.iter().find_map(|p| {
            let i = p.index.binary_search_by_key(&key, |&(k, _)| k).ok()?;
            let start = p.index[i].1;
            let end = p.index.iter().map(|&(_, o)| o).filter(|&o| o > start).min().unwrap_or(p.end);
            Some(&p.data[start..end])
        })
    }

    /// Every packed record's key, sorted.
    pub fn packed_keys(&self) -> impl Iterator<Item = Tgi> + '_ {
        self.packed.iter().flat_map(|p| p.index.iter().map(|&(k, _)| k))
    }
}

/// `readCompressDescRecord` (sc3u_demo 0x0833FBB0): method, count, then the packed records'
/// keys.
fn read_desc(data: &[u8]) -> Result<Vec<Tgi>, Error> {
    let mut r = Reader::new(data);
    let method = r.u32()?;
    if method != METHOD_REF && method != METHOD_FAST3 {
        return Err(Error::BadMethod(method));
    }
    let count = r.u32()? as i32;
    (0..count.max(0)).map(|_| Ok(r.key()?)).collect()
}

/// `cRZDBSegmentMemory::serial_read` (sc3u_demo 0x08340F90) and
/// `construct_keylist_from_segment_memory` (0x083412E0).
///
/// ```text
/// u32     method            0x65 or 0x67
/// string  version           "0.90"
/// u32     packed size
/// u32     unpacked size
/// bytes   packed            u32 size of the whole block, then a QFS stream
/// ```
///
/// The unpacked segment starts with `u32 count, u32 directory offset`; the directory holds
/// `count` entries of `{type, group, instance, offset}`. Entries whose offset is not below
/// the directory are skipped.
fn unpack(data: &[u8]) -> Result<Packed, Error> {
    let mut r = Reader::new(data);
    let method = r.u32()?;
    let _version = r.string()?;
    let packed_len = r.u32()? as usize;
    let len = r.u32()? as usize;
    let packed = r.bytes(packed_len)?;
    if method != METHOD_REF && method != METHOD_FAST3 {
        return Err(Error::BadMethod(method));
    }
    let stream = packed.get(4..).ok_or(Error::Qfs(qfs::Error::BadHeader))?;
    let out = qfs::decompress(stream).map_err(Error::Qfs)?;
    if out.len() != len {
        return Err(Error::SizeMismatch { expected: len, produced: out.len() });
    }
    let mut d = Reader::new(&out);
    let count = d.u32()? as usize;
    let dir = d.u32()? as usize;
    if count.checked_mul(16).and_then(|n| n.checked_add(dir)).is_none_or(|n| n > out.len()) {
        return Err(Error::BadDirectory);
    }
    let mut d = Reader::new(&out[dir..]);
    let mut index: Vec<(Tgi, usize)> = Vec::with_capacity(count);
    for _ in 0..count {
        let key = d.key()?;
        let offset = d.u32()? as usize;
        if offset < dir && !index.iter().any(|&(k, _)| k == key) {
            index.push((key, offset));
        }
    }
    index.sort_by_key(|&(k, _)| k);
    Ok(Packed { data: out, index, end: dir })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ixf::Archive;

    #[test]
    fn opens_terrains_and_saves() {
        let Some(root) = crate::data_dir() else {
            eprintln!("SC3K_DATA not set; skipping");
            return;
        };
        let dirt = Tgi { type_id: 0x206C_6E7C, group_id: 0x2173_7DE5, instance_id: 0 };
        for path in ["Cities/Terrains/Boston, MA.sct", "Cities/Madison, WI.sc3"] {
            let archive = Archive::open(root.join(path)).unwrap();
            let seg = Segment::open(&archive).unwrap();
            assert!(seg.packed_keys().count() > 10, "{path}");
            let rec = seg.get(dirt).unwrap();
            let v = crate::serial::Version::read(&mut Reader::new(rec));
            assert_eq!(v.version, 3, "{path}");
            if v.markers {
                assert_eq!(&rec[rec.len() - 11..], b"DirtBag End", "{path}");
            }
        }
    }
}
