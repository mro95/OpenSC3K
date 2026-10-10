//! Network tiling rules in `Res/TilingRules/*.txt` (`SIMNTWRK.DLL`): tile sets, convert sets,
//! protected sets and rule families. See `docs/formats/tiling.md`.
//!
//! The readers mirror the original's quirks: `strtok` tokens, `strtol` numbers, and the
//! empty-slot handling of each reader.

/// A tile and its rotation. The files pack both into one number, `id << 8 | rot`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileId {
    pub id: u32,
    pub rot: u8,
}

impl TileId {
    /// `cTileID::Convert`
    pub fn from_packed(packed: i32) -> Self {
        Self {
            id: packed as u32 >> 8,
            rot: packed as u8,
        }
    }
}

/// `tSTTileConvert`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileConvert {
    pub from: TileId,
    pub to: TileId,
}

/// `cTileIdentity`: a tile at a meta position of the 5×5 neighbourhood.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileIdentity {
    pub tile: TileId,
    pub pos: u8,
}

/// `cTileRule`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TileRule {
    pub conditions: Vec<TileIdentity>,
    pub solutions: Vec<TileIdentity>,
}

/// `cTilingSet`. `key` is 0x100 for a wildcard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TilingSet {
    pub key: u32,
    pub rules: Vec<TileRule>,
}

/// `cTilingFamily`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TilingFamily {
    pub sets: Vec<TilingSet>,
}

/// `GrokFileTileSet` (`SIMNTWRK.DLL 0x1001746E`): occupant instance ids. Each token has its
/// leading non-digits skipped.
pub fn parse_tile_set(buf: &[u8]) -> Vec<u32> {
    let mut out = Vec::new();
    for t in tokens(buf) {
        let Some(start) = t.iter().position(|b| b.is_ascii_digit()) else {
            continue;
        };
        let id = t[start..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .fold(0u32, |acc, &d| {
                acc.wrapping_mul(10).wrapping_add(u32::from(d - b'0'))
            });
        out.push(id);
    }
    out
}

/// `GrokFileProtectedSets` (libSimNtwrk Ghidra 0x9003C): a count, then plain tile ids. Also
/// reads the bridge sets and `Collapse.txt`. The count only has to be non-zero; it does not
/// limit the loop.
/// Unchecked: no Windows address known yet.
pub fn parse_protected_set(buf: &[u8]) -> Vec<u32> {
    let mut out = Vec::new();
    let mut has_count = false;
    for t in tokens(buf) {
        let v = strtol_prefix(t).unwrap_or(0);
        if has_count {
            out.push(v as u32);
        } else {
            has_count = v != 0;
        }
    }
    out
}

/// `GrokFileConvertSets` (libSimNtwrk Ghidra 0x8FBE4): a count, then packed `from to` pairs.
/// A `from` whose id is 0 leaves the slot empty, so the next token is read as `from` again.
/// Unchecked: no Windows address known yet.
pub fn parse_convert_set(buf: &[u8]) -> Vec<TileConvert> {
    let mut out = Vec::new();
    let mut has_count = false;
    let mut from: Option<TileId> = None;
    for t in tokens(buf) {
        let v = strtol_prefix(t).unwrap_or(0);
        if !has_count {
            has_count = v != 0;
        } else if let Some(from) = from.take() {
            out.push(TileConvert {
                from,
                to: TileId::from_packed(v),
            });
        } else {
            let tile = TileId::from_packed(v);
            if tile.id != 0 {
                from = Some(tile);
            }
        }
    }
    out
}

/// `cPersistantTilingFamily::Read` (sc3u_demo 0x0823EEC0) and `parseToken` (0x0823F194).
/// Each rule becomes its own set.
/// Unchecked: no Windows address known yet.
pub fn parse_family(buf: &[u8]) -> TilingFamily {
    let mut sets = Vec::new();
    let mut state = ParserState::Tag;
    // The rule and condition counts are stored by the original but never used.
    let mut key = 0;
    let mut solution_count = 0;
    let mut rule = TileRule::default();

    for t in tokens(buf) {
        if !t[0].is_ascii_digit() {
            continue;
        }
        let Some(v) = strtol_prefix(t) else { continue };

        state = match state {
            ParserState::Tag => match v {
                0 => ParserState::RuleCount,
                1 => ParserState::Key,
                2 => ParserState::ConditionCount,
                3 => ParserState::ConditionPos,
                4 => ParserState::SolutionCount,
                5 => ParserState::SolutionPos,
                _ => ParserState::Tag,
            },
            ParserState::RuleCount | ParserState::ConditionCount => ParserState::Tag,
            ParserState::Key => {
                key = v as u32;
                ParserState::Tag
            }
            ParserState::SolutionCount => {
                solution_count = v;
                ParserState::Tag
            }
            ParserState::ConditionPos => ParserState::ConditionValue { pos: meta_pos(v) },
            ParserState::SolutionPos => ParserState::SolutionValue { pos: meta_pos(v) },
            ParserState::ConditionValue { pos } => {
                rule.conditions.push(TileIdentity {
                    tile: TileId::from_packed(v),
                    pos,
                });
                ParserState::Tag
            }
            ParserState::SolutionValue { pos } => {
                rule.solutions.push(TileIdentity {
                    tile: TileId::from_packed(v),
                    pos,
                });
                if rule.solutions.len() as i32 == solution_count {
                    sets.push(TilingSet {
                        key,
                        rules: vec![std::mem::take(&mut rule)],
                    });
                    key = 0;
                }
                ParserState::Tag
            }
        };
    }

    TilingFamily { sets }
}

/// `parseToken` states 0–8: what the next digit token means.
#[derive(Clone, Copy, Debug)]
enum ParserState {
    Tag,
    RuleCount,
    Key,
    ConditionCount,
    ConditionPos,
    ConditionValue { pos: u8 },
    SolutionCount,
    SolutionPos,
    SolutionValue { pos: u8 },
}

/// A position truncated to a byte; 255 (−1) means the cell itself, 0x1F.
fn meta_pos(v: i32) -> u8 {
    match v as u8 {
        255 => 0x1F,
        pos => pos,
    }
}

/// `strtok` with the delimiters `" ,\n\t"` over a buffer of at most 0x1FFFF bytes, which the
/// original zero-fills, so a NUL ends the text.
fn tokens(buf: &[u8]) -> impl Iterator<Item = &[u8]> {
    let buf = &buf[..buf.len().min(0x1FFFF)];
    let buf = buf.split(|&b| b == 0).next().unwrap_or(&[]);
    buf.split(|b| matches!(b, b' ' | b',' | b'\n' | b'\t'))
        .filter(|t| !t.is_empty())
}

/// `strtol(t, NULL, 10)` with a 32-bit `long`. `None` when no digits are read, where `strtol`
/// returns 0.
fn strtol_prefix(t: &[u8]) -> Option<i32> {
    // isspace: space, \t, \n, \v, \f, \r
    let start = t
        .iter()
        .position(|b| !matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
        .unwrap_or(t.len());
    let t = &t[start..];

    let (neg, t) = match t.first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };

    let digits = t.iter().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }

    // Cap at 2^31 so the i64 never overflows, then clamp like strtol.
    let mag = t[..digits].iter().fold(0i64, |acc, &d| {
        (acc * 10 + i64::from(d - b'0')).min(1 << 31)
    });
    let v = if neg { -mag } else { mag };
    Some(v.clamp(i32::MIN.into(), i32::MAX.into()) as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(buf: &[u8]) -> Vec<&[u8]> {
        tokens(buf).collect()
    }

    #[test]
    fn tokens_split_on_strtok_delimiters() {
        assert_eq!(
            toks(b"0,4\n1,256\t2 3"),
            [&b"0"[..], b"4", b"1", b"256", b"2", b"3"]
        );
    }

    #[test]
    fn tokens_merge_delimiter_runs() {
        assert_eq!(toks(b" ,\n\t a,, \n\tb ,\n"), [&b"a"[..], b"b"]);
        assert!(toks(b" ,\n\t").is_empty());
        assert!(toks(b"").is_empty());
    }

    #[test]
    fn tokens_keep_cr_and_braces() {
        // \r, { and } are not delimiters.
        assert_eq!(toks(b"1,256\r\n2"), [&b"1"[..], b"256\r", b"2"]);
        assert_eq!(toks(b"1\n\r\n2"), [&b"1"[..], b"\r", b"2"]);
        assert_eq!(toks(b"{29, 18070}"), [&b"{29"[..], b"18070}"]);
    }

    #[test]
    fn tokens_stop_at_nul() {
        assert_eq!(toks(b"1 2\0 3 4"), [&b"1"[..], b"2"]);
        assert!(toks(b"\0 1").is_empty());
    }

    #[test]
    fn tokens_read_at_most_0x1ffff_bytes() {
        let mut buf = vec![b'7'; 0x1FFFF];
        buf.extend_from_slice(b" 8");
        let t = toks(&buf);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].len(), 0x1FFFF);

        let mut buf = vec![b' '; 0x1FFFE];
        buf.extend_from_slice(b"12");
        assert_eq!(toks(&buf), [&b"1"[..]]);
    }

    #[test]
    fn strtol_reads_decimal() {
        assert_eq!(strtol_prefix(b"42"), Some(42));
        assert_eq!(strtol_prefix(b"0"), Some(0));
        assert_eq!(strtol_prefix(b"007"), Some(7));
        assert_eq!(strtol_prefix(b"2873603"), Some(2873603));
    }

    #[test]
    fn strtol_stops_at_first_non_digit() {
        assert_eq!(strtol_prefix(b"88}\r"), Some(88));
        assert_eq!(strtol_prefix(b"12a3"), Some(12));
        assert_eq!(strtol_prefix(b"0x10"), Some(0)); // base 10, no hex prefix
    }

    #[test]
    fn strtol_skips_leading_whitespace() {
        assert_eq!(strtol_prefix(b"\r12"), Some(12));
        assert_eq!(strtol_prefix(b"\x0b\x0c\r5"), Some(5));
    }

    #[test]
    fn strtol_takes_one_sign() {
        assert_eq!(strtol_prefix(b"+5"), Some(5));
        assert_eq!(strtol_prefix(b"-1"), Some(-1));
        assert_eq!(strtol_prefix(b"--1"), None);
        assert_eq!(strtol_prefix(b"- 5"), None);
    }

    #[test]
    fn strtol_without_digits_is_none() {
        assert_eq!(strtol_prefix(b""), None);
        assert_eq!(strtol_prefix(b"}"), None);
        assert_eq!(strtol_prefix(b"{29"), None); // tile set skips the `{` itself
        assert_eq!(strtol_prefix(b"-"), None);
        assert_eq!(strtol_prefix(b"\r"), None);
    }

    #[test]
    fn strtol_clamps_to_32_bit_long() {
        assert_eq!(strtol_prefix(b"2147483647"), Some(i32::MAX));
        assert_eq!(strtol_prefix(b"2147483648"), Some(i32::MAX));
        assert_eq!(strtol_prefix(b"99999999999"), Some(i32::MAX));
        assert_eq!(strtol_prefix(b"-2147483648"), Some(i32::MIN));
        assert_eq!(strtol_prefix(b"-99999999999"), Some(i32::MIN));
    }

    #[test]
    fn tile_set_skips_leading_non_digits() {
        let buf = b"{29, 43,\r\n18070}\r\n} \r -5 a1b2 {}\r\n";
        assert_eq!(parse_tile_set(buf), [29, 43, 18070, 5, 1]);
    }

    #[test]
    fn protected_set_reads_count_then_ids() {
        assert_eq!(
            parse_protected_set(b"3\r\n63\r\n64\r\n18070\r\n"),
            [63, 64, 18070]
        );
    }

    #[test]
    fn protected_set_count_does_not_limit() {
        assert_eq!(parse_protected_set(b"1 7 8 9"), [7, 8, 9]);
    }

    #[test]
    fn protected_set_zero_count_takes_next_token() {
        assert_eq!(parse_protected_set(b"0 3 7 9"), [7, 9]);
        assert_eq!(parse_protected_set(b"} 0 2 7"), [7]); // `}` reads as 0 too
    }

    #[test]
    fn protected_set_pushes_zero_ids() {
        // No digit filter: junk tokens read as 0 and are kept.
        assert_eq!(parse_protected_set(b"3 7 } 0 9"), [7, 0, 0, 9]);
        assert_eq!(parse_protected_set(b"2 7 \r 9"), [7, 0, 9]);
    }

    #[test]
    fn protected_set_uses_strtol_prefix() {
        assert_eq!(parse_protected_set(b"2 12} -1"), [12, u32::MAX]);
    }

    #[test]
    fn protected_set_empty() {
        assert!(parse_protected_set(b"").is_empty());
        assert!(parse_protected_set(b"5\r\n").is_empty());
    }

    #[test]
    fn protected_set_road_install() {
        let Some(dir) = crate::data_dir() else { return };
        let buf = std::fs::read(dir.join("Apps/Res/TilingRules/ROAD_GRND_Protected.txt")).unwrap();
        let ids = parse_protected_set(&buf);
        assert_eq!(ids.len(), 66);
        assert_eq!(ids[0], 63);
        assert_eq!(ids[65], 18070);
    }

    fn conv(from: (u32, u8), to: (u32, u8)) -> TileConvert {
        TileConvert {
            from: TileId {
                id: from.0,
                rot: from.1,
            },
            to: TileId {
                id: to.0,
                rot: to.1,
            },
        }
    }

    #[test]
    fn convert_set_reads_pairs() {
        // 17152 = 67 << 8, 7424 = 29 << 8
        let buf = b"2\r\n17152,7424\r\n17153,7425\r\n";
        assert_eq!(
            parse_convert_set(buf),
            [conv((67, 0), (29, 0)), conv((67, 1), (29, 1))]
        );
    }

    #[test]
    fn convert_set_count_does_not_limit() {
        assert_eq!(parse_convert_set(b"1 17152 7424 17153 7425").len(), 2);
    }

    #[test]
    fn convert_set_zero_count_takes_next_token() {
        assert_eq!(
            parse_convert_set(b"0 1 17152 7424"),
            [conv((67, 0), (29, 0))]
        );
        assert_eq!(
            parse_convert_set(b"} 1 17152 7424"),
            [conv((67, 0), (29, 0))]
        );
    }

    #[test]
    fn convert_set_from_with_id_zero_is_overwritten() {
        // 5 is id 0 rot 5, `}` is id 0 rot 0: neither fills the from slot.
        assert_eq!(
            parse_convert_set(b"1 5 } 17152 7424"),
            [conv((67, 0), (29, 0))]
        );
    }

    #[test]
    fn convert_set_to_with_id_zero_is_pushed() {
        assert_eq!(parse_convert_set(b"1 17152 3"), [conv((67, 0), (0, 3))]);
        assert_eq!(parse_convert_set(b"1 17152 }"), [conv((67, 0), (0, 0))]);
    }

    #[test]
    fn convert_set_drops_unpaired_from() {
        assert_eq!(
            parse_convert_set(b"1 17152 7424 17153"),
            [conv((67, 0), (29, 0))]
        );
    }

    #[test]
    fn convert_set_uses_strtol_prefix() {
        assert_eq!(
            parse_convert_set(b"1 17152} 7424\r"),
            [conv((67, 0), (29, 0))]
        );
    }

    #[test]
    fn convert_set_road_install() {
        let Some(dir) = crate::data_dir() else { return };
        let rules = dir.join("Apps/Res/TilingRules");

        let simple =
            parse_convert_set(&std::fs::read(rules.join("ROAD_GRND_Convert.txt")).unwrap());
        assert_eq!(simple.len(), 184);
        assert_eq!(simple[0], conv((67, 0), (29, 0)));

        let complex =
            parse_convert_set(&std::fs::read(rules.join("ROAD_GRND_Complex_Convert.txt")).unwrap());
        assert_eq!(complex.len(), 180);
    }

    fn ident(id: u32, rot: u8, pos: u8) -> TileIdentity {
        TileIdentity {
            tile: TileId { id, rot },
            pos,
        }
    }

    fn set(key: u32, conditions: Vec<TileIdentity>, solutions: Vec<TileIdentity>) -> TilingSet {
        TilingSet {
            key,
            rules: vec![TileRule {
                conditions,
                solutions,
            }],
        }
    }

    // 7424 = 29 << 8, 7425 = 29 << 8 | 1, 11008 = 43 << 8

    #[test]
    fn family_single_rule() {
        let f = parse_family(b"0,1\r\n1,5\r\n2,0\r\n4,1\r\n5,255,7424\r\n");
        assert_eq!(f.sets, [set(5, vec![], vec![ident(29, 0, 31)])]);
    }

    #[test]
    fn family_conditions_and_solutions() {
        let f = parse_family(b"1,8 2,2 3,1,7424 3,3,7425 4,2 5,31,7424 5,0,11008");
        assert_eq!(
            f.sets,
            [set(
                8,
                vec![ident(29, 0, 1), ident(29, 1, 3)],
                vec![ident(29, 0, 31), ident(43, 0, 0)]
            )]
        );
    }

    #[test]
    fn family_each_rule_is_own_set_and_key_resets() {
        // Second rule has no `1,k`: key 0. Solution count 1 carries over.
        let f = parse_family(b"1,7 4,1 5,255,7424 5,255,11008");
        assert_eq!(
            f.sets,
            [
                set(7, vec![], vec![ident(29, 0, 31)]),
                set(0, vec![], vec![ident(43, 0, 31)])
            ]
        );
    }

    #[test]
    fn family_pos_255_is_self_and_truncates() {
        // 287 = 0x11F truncates to 0x1F, 511 truncates to 255 and maps to 0x1F.
        let f = parse_family(b"4,1 1,1 3,287,7424 5,511,7424");
        assert_eq!(
            f.sets,
            [set(1, vec![ident(29, 0, 31)], vec![ident(29, 0, 31)])]
        );
    }

    #[test]
    fn family_skips_non_digit_tokens() {
        // `}`, `{x`, `\r` don't touch the state; `7424}` starts with a digit.
        let f = parse_family(b"} 1,5 {x 4,1 \r 5,255,7424}\r\n");
        assert_eq!(f.sets, [set(5, vec![], vec![ident(29, 0, 31)])]);
    }

    #[test]
    fn family_ignores_unknown_tags() {
        let f = parse_family(b"9 7 1,5 4,1 5,255,7424");
        assert_eq!(f.sets, [set(5, vec![], vec![ident(29, 0, 31)])]);
    }

    #[test]
    fn family_pos_state_does_not_resync() {
        // `3` opens a condition; the following `4` is read as its pos, not as a tag,
        // and `1` as its value (id 0, rot 1).
        let f = parse_family(b"4,1 1,5 3 4,1 5,255,7424");
        assert_eq!(
            f.sets,
            [set(5, vec![ident(0, 1, 4)], vec![ident(29, 0, 31)])]
        );
    }

    #[test]
    fn family_solution_count_zero_never_finishes() {
        assert!(parse_family(b"1,3 4,0 5,255,7424 5,255,7424")
            .sets
            .is_empty());
    }

    #[test]
    fn family_drops_unfinished_rule_at_end() {
        assert!(parse_family(b"1,5 4,2 5,255,7424").sets.is_empty());
    }

    #[test]
    fn family_road_install() {
        let Some(dir) = crate::data_dir() else { return };
        let rules = dir.join("Apps/Res/TilingRules");
        let read = |name: &str| parse_family(&std::fs::read(rules.join(name)).unwrap());

        let fin = read("ROAD_GRND_final.txt");
        assert_eq!(fin.sets.len(), 16);
        let keys: Vec<u32> = fin.sets.iter().map(|s| s.key).collect();
        assert_eq!(keys, [0, 8, 4, 1, 2, 12, 9, 10, 5, 6, 3, 13, 14, 11, 7, 15]);
        let solve = |key| fin.sets.iter().find(|s| s.key == key).unwrap().rules[0].solutions[0];
        assert_eq!(solve(5), ident(29, 0, 31));
        assert_eq!(solve(10), ident(29, 1, 31));
        assert_eq!(solve(15), ident(43, 0, 31));

        let simple = read("ROAD_GRND_SimpleRules.txt");
        assert_eq!(simple.sets.len(), 430);
        assert_eq!(simple.sets[0], set(8, vec![], vec![ident(11225, 0, 31)]));

        let complex = read("ROAD_GRND_ComplexRules.txt");
        assert_eq!(complex.sets.len(), 195);
        assert!(complex.sets.iter().all(|s| s.key == 256));
        assert_eq!(
            complex.sets[0].rules[0].conditions,
            [ident(35, 2, 6), ident(11203, 2, 31)]
        );
    }
}
