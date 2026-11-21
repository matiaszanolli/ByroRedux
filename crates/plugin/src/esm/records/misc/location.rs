//! `LCTN` location records (Skyrim+) — the hierarchy and keywords the
//! location runtime reads (M47.2): `GetInCurrentLoc`, `LocationHasKeyword`,
//! the location aliases and the editor location of a persistent reference.
//!
//! Layout per xEdit's `wbDefinitionsTES5` `LCTN`: `FULL`, `KSIZ` + `KWDA`
//! keywords, `PNAM` parent location, and the reference tables. The `A*` and
//! `L*` spellings carry the same content (UESP *Skyrim Mod:Mod File
//! Format/LCTN*: the names changed with Dawnguard):
//!
//! - `ACPR` / `LCPR` population refs `{ref, cell-or-world, grid y, grid x}`
//!   (12 bytes; Starfield appends a `wbUnknown(4)`, 16 bytes) and `RCPR` bare
//!   refs — editor location: this location.
//! - `ACUN` / `LCUN` unique actors `{actor base, ref, location}` (12 bytes) —
//!   editor location: the struct's own location ("points to itself in most
//!   cases"), this one when null.
//! - `ACSR` / `LCSR` static refs `{location ref type, ref, cell-or-world,
//!   grid}` (16 bytes; Starfield 20) — the location's `LCRT`-tagged
//!   references (boss, map marker, …).
//!
//! Starfield's trailing `wbUnknown(4)` on both structs is
//! `wbDefinitionsSF1.pas:13401-13470`; read at the FO4 stride, every entry
//! after the first was misaligned and its grid / unknown words were remapped
//! as FormIDs (#7118). FO4 and FO76 keep the shorter layout.

use super::super::common::{remap_fid, CommonNamedFields};
use crate::esm::reader::{FormIdRemap, GameKind, SubRecord};
use crate::esm::sub_reader::SubReader;

/// One parsed `LCTN`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LctnRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// `KWDA` keyword FormIDs (`LocTypeDungeon`, `LocTypeCity`, …).
    pub keywords: Vec<u32>,
    /// `PNAM` parent location, `None` at the top of the hierarchy.
    pub parent: Option<u32>,
    /// `(reference, editor location)` for the `ACPR` / `LCPR` / `RCPR`
    /// population refs and the `ACUN` / `LCUN` unique actors.
    pub editor_location_refs: Vec<(u32, u32)>,
    /// `(location ref type, reference)` from `ACSR` / `LCSR`.
    pub ref_type_refs: Vec<(u32, u32)>,
}

/// `ACPR` / `LCPR` entry size before Starfield: ref, cell-or-world, grid.
const PERSIST_REF_LEN: usize = 12;
/// `ACSR` / `LCSR` entry size before Starfield: ref type, ref, cell-or-world, grid.
const SPECIAL_REF_LEN: usize = 16;
/// Starfield's `wbUnknown(4)` tail on both entry kinds.
const STARFIELD_REF_TAIL: usize = 4;

pub fn parse_lctn(
    form_id: u32,
    subs: &[SubRecord],
    remap: &Option<FormIdRemap>,
    game: GameKind,
) -> LctnRecord {
    let tail = if game == GameKind::Starfield { STARFIELD_REF_TAIL } else { 0 };
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    let mut out = LctnRecord {
        form_id,
        editor_id: common.editor_id,
        full_name: common.full_name,
        ..Default::default()
    };
    for sub in subs {
        let mut r = SubReader::new(&sub.data);
        match &sub.sub_type {
            b"KWDA" => {
                while r.remaining() >= 4 {
                    out.keywords.push(remap_fid(r.u32_or_default(), remap));
                }
            }
            b"PNAM" if sub.data.len() >= 4 => {
                let parent = remap_fid(r.u32_or_default(), remap);
                out.parent = (parent != 0).then_some(parent);
            }
            // {ref, cell/world, grid y i16, grid x i16}
            b"ACPR" | b"LCPR" => {
                while r.remaining() >= PERSIST_REF_LEN + tail {
                    let reference = r.u32_or_default();
                    let _cell = r.u32_or_default();
                    let _grid = r.u32_or_default();
                    r.skip_or_eof(tail);
                    if reference != 0 {
                        out.editor_location_refs
                            .push((remap_fid(reference, remap), form_id));
                    }
                }
            }
            b"RCPR" => {
                while r.remaining() >= 4 {
                    let reference = r.u32_or_default();
                    if reference != 0 {
                        out.editor_location_refs
                            .push((remap_fid(reference, remap), form_id));
                    }
                }
            }
            // {actor base, ref, location}
            b"ACUN" | b"LCUN" => {
                while r.remaining() >= 12 {
                    let _actor = r.u32_or_default();
                    let reference = r.u32_or_default();
                    let location = r.u32_or_default();
                    if reference != 0 {
                        let location = match location {
                            0 => form_id,
                            location => remap_fid(location, remap),
                        };
                        out.editor_location_refs
                            .push((remap_fid(reference, remap), location));
                    }
                }
            }
            // {location ref type, ref, cell/world, grid y i16, grid x i16}
            b"ACSR" | b"LCSR" => {
                while r.remaining() >= SPECIAL_REF_LEN + tail {
                    let ref_type = r.u32_or_default();
                    let reference = r.u32_or_default();
                    let _cell = r.u32_or_default();
                    let _grid = r.u32_or_default();
                    r.skip_or_eof(tail);
                    if ref_type != 0 && reference != 0 {
                        out.ref_type_refs
                            .push((remap_fid(ref_type, remap), remap_fid(reference, remap)));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::records::test_support::{edid, sub};

    fn words(values: &[u32]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    #[test]
    fn parses_keywords_parent_and_editor_location_refs() {
        let subs = vec![
            edid("WhiterunLocation"),
            sub(b"LCPR", words(&[0xA1, 0x3C, 0x0002_FFFC, 0xA2, 0x3C, 0])),
            sub(b"RCPR", words(&[0xA3])),
            sub(b"LCUN", words(&[0x0001_3BBF, 0xA4, 0x1_8A56, 0x0001_3BC0, 0xA5, 0])),
            sub(b"LCSR", words(&[0x0001_30F8, 0xA6, 0x3C, 0x0002_FFFC])),
            sub(b"KSIZ", words(&[2])),
            sub(b"KWDA", words(&[0x13168, 0x13166])),
            sub(b"PNAM", words(&[0x1_6772])),
        ];
        let record = parse_lctn(0x1_8A56, &subs, &None, GameKind::Skyrim);
        assert_eq!(record.editor_id, "WhiterunLocation");
        assert_eq!(record.keywords, vec![0x13168, 0x13166]);
        assert_eq!(record.parent, Some(0x1_6772));
        let own = 0x1_8A56;
        assert_eq!(
            record.editor_location_refs,
            vec![(0xA1, own), (0xA2, own), (0xA3, own), (0xA4, 0x1_8A56), (0xA5, own)]
        );
        assert_eq!(record.ref_type_refs, vec![(0x0001_30F8, 0xA6)]);
    }

    #[test]
    fn a_null_parent_is_none_and_short_tables_stop_cleanly() {
        let subs = vec![
            sub(b"PNAM", words(&[0])),
            sub(b"LCPR", [1, 2, 3]),
        ];
        let record = parse_lctn(1, &subs, &None, GameKind::Skyrim);
        assert_eq!(record.parent, None);
        assert!(record.editor_location_refs.is_empty());
    }

    /// Starfield appends a `wbUnknown(4)` to both entry kinds (16 / 20 bytes,
    /// `wbDefinitionsSF1.pas:13401-13470`): two entries each must decode
    /// their own refs, not the previous entry's tail words (#7118).
    #[test]
    fn starfield_ref_entries_step_over_their_unknown_tail() {
        let subs = vec![
            sub(b"LCPR", words(&[0xA1, 0x3C, 0x7FFF_7FFF, 0xFFFF_FFFF, 0xA2, 0x3C, 0, 0x40A0_0000])),
            sub(
                b"LCSR",
                words(&[0xB1, 0xC1, 0x3C, 0x7FFF_7FFF, 0xFFFF_FFFF, 0xB2, 0xC2, 0x3C, 0, 0x42C8_0000]),
            ),
        ];
        let record = parse_lctn(7, &subs, &None, GameKind::Starfield);
        assert_eq!(record.editor_location_refs, vec![(0xA1, 7), (0xA2, 7)]);
        assert_eq!(record.ref_type_refs, vec![(0xB1, 0xC1), (0xB2, 0xC2)]);

        // The same bytes at FO4's stride misread the tail words as entries.
        let fo4 = parse_lctn(7, &subs, &None, GameKind::Fallout4);
        assert_ne!(fo4.ref_type_refs, record.ref_type_refs);
    }
}
