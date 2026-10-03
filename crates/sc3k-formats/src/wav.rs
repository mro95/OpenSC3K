//! RIFF WAVE files with PCM samples (`Res/Sound/MUSIC/3KLOOP.WAV`, the `.WAV` effects).
//! See `docs/formats/audio.md`.

use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    NotRiffWave,
    MissingChunk(&'static str),
    Unsupported { format: u16, bits: u16 },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotRiffWave => write!(f, "not a RIFF WAVE file"),
            Error::MissingChunk(c) => write!(f, "missing '{c}' chunk"),
            Error::Unsupported { format, bits } => {
                write!(f, "unsupported WAVE encoding (format tag {format}, {bits} bits)")
            }
        }
    }
}

impl std::error::Error for Error {}

/// Decoded PCM audio: interleaved signed 16-bit samples.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wav {
    pub channels: u16,
    pub sample_rate: u32,
    pub samples: Vec<i16>,
}

impl Wav {
    pub fn parse(data: &[u8]) -> Result<Wav, Error> {
        if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
            return Err(Error::NotRiffWave);
        }
        let mut fmt = None;
        let mut pcm = None;
        let mut pos = 12;
        while pos + 8 <= data.len() {
            let id = &data[pos..pos + 4];
            let len = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().unwrap()) as usize;
            let body = &data[pos + 8..(pos + 8 + len).min(data.len())];
            match id {
                b"fmt " if body.len() >= 16 => fmt = Some(body),
                b"data" => pcm = Some(body),
                _ => {}
            }
            // Chunks are padded to even sizes.
            pos += 8 + len + (len & 1);
        }
        let fmt = fmt.ok_or(Error::MissingChunk("fmt "))?;
        let pcm = pcm.ok_or(Error::MissingChunk("data"))?;
        let u16_at = |i: usize| u16::from_le_bytes([fmt[i], fmt[i + 1]]);
        let (format, channels, bits) = (u16_at(0), u16_at(2), u16_at(14));
        let sample_rate = u32::from_le_bytes(fmt[4..8].try_into().unwrap());
        let samples = match (format, bits) {
            (1, 16) => pcm.chunks_exact(2).map(|s| i16::from_le_bytes([s[0], s[1]])).collect(),
            // 8-bit PCM is unsigned.
            (1, 8) => pcm.iter().map(|&s| (s as i16 - 128) << 8).collect(),
            _ => return Err(Error::Unsupported { format, bits }),
        };
        Ok(Wav { channels, sample_rate, samples })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(bits: u16, pcm: &[u8]) -> Vec<u8> {
        let mut fmt = Vec::new();
        for v in [1u16, 2] {
            fmt.extend_from_slice(&v.to_le_bytes());
        }
        fmt.extend_from_slice(&22050u32.to_le_bytes());
        fmt.extend_from_slice(&(22050u32 * 2 * bits as u32 / 8).to_le_bytes());
        fmt.extend_from_slice(&(2 * bits / 8).to_le_bytes());
        fmt.extend_from_slice(&bits.to_le_bytes());
        let mut d = b"RIFF\0\0\0\0WAVE".to_vec();
        d.extend_from_slice(b"fmt ");
        d.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
        d.extend_from_slice(&fmt);
        d.extend_from_slice(b"LIST\x01\0\0\0x\0"); // odd-sized chunk, padded
        d.extend_from_slice(b"data");
        d.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
        d.extend_from_slice(pcm);
        d
    }

    #[test]
    fn decodes_pcm() {
        let w = Wav::parse(&wav(16, &[0x01, 0x00, 0xFF, 0xFF])).unwrap();
        assert_eq!((w.channels, w.sample_rate, w.samples), (2, 22050, vec![1, -1]));
        let w = Wav::parse(&wav(8, &[0x80, 0xFF])).unwrap();
        assert_eq!(w.samples, vec![0, 127 << 8]);
        assert_eq!(Wav::parse(b"RIFFxxxxAVI "), Err(Error::NotRiffWave));
    }
}
