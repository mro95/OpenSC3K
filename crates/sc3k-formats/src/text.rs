//! String table records (`Res/Text/<LANG>/*.IXF`): one string per IXF entry, addressed
//! by (table type, string id). See `docs/formats/text.md`.

/// A string record: `u32` byte length, then that many bytes of Windows-1252 text.
/// Returns the raw bytes, which is what the bitmap fonts index by.
pub fn parse_string(data: &[u8]) -> Option<&[u8]> {
    let len = u32::from_le_bytes(data.get(..4)?.try_into().ok()?) as usize;
    data.get(4..4 + len)
}

/// Decode Windows-1252 bytes for display outside the game's own fonts.
pub fn decode_cp1252(bytes: &[u8]) -> String {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}',
        '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
    ];
    bytes
        .iter()
        .map(|&b| match b {
            0x80..=0x9F => HIGH[(b - 0x80) as usize],
            _ => b as char,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_length_prefixed_string() {
        let rec = [4, 0, 0, 0, b'E', b'x', b'i', b't', 0xFF];
        assert_eq!(parse_string(&rec), Some(&b"Exit"[..]));
        assert_eq!(parse_string(&[9, 0, 0, 0, b'x']), None);
        assert_eq!(decode_cp1252(&[0xA9, b' ', 0x99]), "© ™");
    }
}
