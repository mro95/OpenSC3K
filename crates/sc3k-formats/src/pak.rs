//! `Apps/Sys/SYS.PAK`: the game's configuration INI files in one archive. See
//! `docs/formats/sys-pak.md`.
//!
//! Layout: u32 file count, then per file {u32 name length, name, u32 data offset}; each
//! file's data runs to the next file's offset (the last to the end). A file is a list of
//! lines: u32 line count, then per line {u32 length, text}.

use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Truncated,
    BadOffset { name: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Truncated => write!(f, "pak truncated"),
            Error::BadOffset { name } => write!(f, "{name}: data offset outside the pak"),
        }
    }
}

impl std::error::Error for Error {}

pub struct Pak {
    data: Vec<u8>,
    /// (name, start, end) in file order.
    files: Vec<(String, usize, usize)>,
}

impl Pak {
    pub fn parse(data: Vec<u8>) -> Result<Pak, Error> {
        let mut pos = 0;
        let count = read_u32(&data, &mut pos)? as usize;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let len = read_u32(&data, &mut pos)? as usize;
            let name = data.get(pos..pos + len).ok_or(Error::Truncated)?;
            let name = String::from_utf8_lossy(name).into_owned();
            pos += len;
            let offset = read_u32(&data, &mut pos)? as usize;
            if offset > data.len() {
                return Err(Error::BadOffset { name });
            }
            entries.push((name, offset));
        }
        let mut starts: Vec<usize> = entries.iter().map(|&(_, o)| o).collect();
        starts.sort_unstable();
        let files = entries
            .into_iter()
            .map(|(name, start)| {
                let end = starts.iter().copied().find(|&s| s > start).unwrap_or(data.len());
                (name, start, end)
            })
            .collect();
        Ok(Pak { data, files })
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.files.iter().map(|(n, _, _)| n.as_str())
    }

    /// Raw data of `name` (matched case-insensitively).
    pub fn file(&self, name: &str) -> Option<&[u8]> {
        self.files
            .iter()
            .find(|(n, _, _)| n.eq_ignore_ascii_case(name))
            .map(|&(_, s, e)| &self.data[s..e])
    }

    /// The lines of `name`, as Windows-1252 bytes.
    pub fn lines(&self, name: &str) -> Option<Result<Vec<Vec<u8>>, Error>> {
        self.file(name).map(parse_lines)
    }
}

pub fn parse_lines(data: &[u8]) -> Result<Vec<Vec<u8>>, Error> {
    let mut pos = 0;
    let count = read_u32(data, &mut pos)? as usize;
    let mut lines = Vec::with_capacity(count);
    for _ in 0..count {
        let len = read_u32(data, &mut pos)? as usize;
        lines.push(data.get(pos..pos + len).ok_or(Error::Truncated)?.to_vec());
        pos += len;
    }
    Ok(lines)
}

fn read_u32(data: &[u8], pos: &mut usize) -> Result<u32, Error> {
    let b = data.get(*pos..*pos + 4).ok_or(Error::Truncated)?;
    *pos += 4;
    Ok(u32::from_le_bytes(b.try_into().unwrap()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines_blob(lines: &[&str]) -> Vec<u8> {
        let mut out = (lines.len() as u32).to_le_bytes().to_vec();
        for l in lines {
            out.extend_from_slice(&(l.len() as u32).to_le_bytes());
            out.extend_from_slice(l.as_bytes());
        }
        out
    }

    #[test]
    fn reads_files_and_lines() {
        let a = lines_blob(&["[S]", "k=v"]);
        let b = lines_blob(&["x"]);
        let dir_len = 4 + (4 + 5 + 4) * 2;
        let mut pak = 2u32.to_le_bytes().to_vec();
        for (name, off) in [("a.ini", dir_len), ("B.ini", dir_len + a.len())] {
            pak.extend_from_slice(&(name.len() as u32).to_le_bytes());
            pak.extend_from_slice(name.as_bytes());
            pak.extend_from_slice(&(off as u32).to_le_bytes());
        }
        pak.extend_from_slice(&a);
        pak.extend_from_slice(&b);
        let pak = Pak::parse(pak).unwrap();
        assert_eq!(pak.names().collect::<Vec<_>>(), ["a.ini", "B.ini"]);
        assert_eq!(pak.lines("A.INI").unwrap().unwrap(), [b"[S]".to_vec(), b"k=v".to_vec()]);
        assert_eq!(pak.lines("b.ini").unwrap().unwrap(), [b"x".to_vec()]);
    }
}
