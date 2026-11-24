//! `LCTN` location records (Skyrim+) — the hierarchy and keywords the
//! location runtime reads (M47.2): `GetInCurrentLoc`, `LocationHasKeyword`,
//! the location aliases and the editor location of a persistent reference.
//!
//! Layout per xEdit's `wbDefinitionsTES5` `LCTN`: `FULL`, `KSIZ` + `KWDA`
//! keywords, `PNAM` parent location, and three sets of reference tables
//! (`wbDefinitionsTES5.pas:6310-6400`, `wbDefinitionsFO4.pas:8045-8154`,
//! `wbDefinitionsSF1.pas:13401-13490`): the **Master** tables (`LC*`) a
//! location's defining record lists, the **Added** tables (`AC*`) and the
//! **Removed** tables (`RC*`) an override lists against them (#7276):
//!
//! - `LCPR` / `ACPR` persistent location refs `{ref, cell-or-world, grid y,
//!   grid x}` (12 bytes; Starfield appends a `wbUnknown(4)`, 16 bytes), and
//!   `RCPR` the removed refs, bare — editor location: this location.
//! - `LCUN` / `ACUN` unique NPCs `{actor base, ref, location}` (12 bytes) —
//!   editor location: the struct's own location ("points to itself in most
//!   cases"), this one when null — and `RCUN` the removed actor bases.
//! - `LCSR` / `ACSR` special refs `{location ref type, ref, cell-or-world,
//!   grid}` (16 bytes; Starfield 20) — the location's `LCRT`-tagged
//!   references (boss, map marker, …) — and `RCSR` the removed refs.
//!
//! Within one record the effective tables are Master and Added less Removed
//! ([`LctnRecord::editor_location_refs`], [`LctnRecord::ref_type_refs`]). How
//! the game combines the tables of *several* plugins' records is not
//! documented; the fold keeps the winning record's (#7276 stays open for it).
//!
//! **Partial forms (#7275).** Fallout 4 and Starfield overrides of a location
//! can set record flag `0x4000` and carry only the sub-records they change:
//! `SFBGS003.esm`'s `LCTN 00010DFB` carries `EDID` and the Added tables alone.
//! xEdit applies "Partial Form" inheritance (a sub-record the partial form
//! leaves out comes from the previous override, `wbConflict.pas`) to `CELL`,
//! `DIAL`, `QUST` and `WRLD`, but names `LCTN`'s bit `'Unknown 14' //Partial
//! Form` (`wbDefinitionsFO4.pas:8042`, `wbDefinitionsSF1.pas:13382`). That it
//! applies to `LCTN` too is **inferred**: from that comment, and from shipped
//! DLC partials whose whole-record replacement would strip an official
//! location of its name, keywords and parent. [`LctnRecord::overlaid_on`]
//! applies it.
//!
//! Starfield's trailing `wbUnknown(4)` on the population and special structs is
//! `wbDefinitionsSF1.pas:13401-13470`; read at the FO4 stride, every entry
//! after the first was misaligned and its grid / unknown words were remapped
//! as FormIDs (#7118). FO4 and FO76 keep the shorter layout.

use super::super::common::{remap_fid, CommonNamedFields};
use crate::esm::reader::{FormIdRemap, GameKind, SubRecord};
use crate::esm::sub_reader::SubReader;

/// One set of `LCTN` reference tables: the Master (`LC*`), Added (`AC*`) or
/// Removed (`RC*`) set.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LctnRefTables {
    /// `*CPR` persistent location references.
    pub persist: Vec<u32>,
    /// `*CUN` unique NPCs, `(actor base, reference, editor location)`; a
    /// removed entry (`RCUN`) names the actor base alone.
    pub unique: Vec<(u32, u32, u32)>,
    /// `*CSR` special references, `(location ref type, reference)`; a removed
    /// entry (`RCSR`) names the reference alone.
    pub special: Vec<(u32, u32)>,
}

/// Which sub-record groups an `LCTN` record carried, so a partial form
/// ([`LctnRecord::overlaid_on`]) inherits exactly the ones it left out.
pub mod carried {
    pub const FULL: u16 = 1 << 0;
    pub const KEYWORDS: u16 = 1 << 1;
    pub const PARENT: u16 = 1 << 2;
    pub const MASTER_PERSIST: u16 = 1 << 3;
    pub const ADDED_PERSIST: u16 = 1 << 4;
    pub const REMOVED_PERSIST: u16 = 1 << 5;
    pub const MASTER_UNIQUE: u16 = 1 << 6;
    pub const ADDED_UNIQUE: u16 = 1 << 7;
    pub const REMOVED_UNIQUE: u16 = 1 << 8;
    pub const MASTER_SPECIAL: u16 = 1 << 9;
    pub const ADDED_SPECIAL: u16 = 1 << 10;
    pub const REMOVED_SPECIAL: u16 = 1 << 11;
}

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
    /// The Master tables (`LC*`).
    pub master: LctnRefTables,
    /// The Added tables (`AC*`).
    pub added: LctnRefTables,
    /// The Removed tables (`RC*`).
    pub removed: LctnRefTables,
    /// The [`carried`] bits of the sub-record groups this record carried.
    pub carried: u16,
    /// `(reference, editor location)` of the persistent refs and unique NPCs
    /// the effective tables hold: Master and Added, less Removed.
    pub editor_location_refs: Vec<(u32, u32)>,
    /// `(location ref type, reference)` of the effective special refs.
    pub ref_type_refs: Vec<(u32, u32)>,
}

impl LctnRecord {
    /// Recompute [`Self::editor_location_refs`] and [`Self::ref_type_refs`]
    /// from the tables: Master, then Added, each entry Removed names left out.
    fn derive_effective(&mut self) {
        let own = self.form_id;
        let removed = &self.removed;
        let tables = [&self.master, &self.added];
        self.editor_location_refs = tables
            .iter()
            .flat_map(|table| table.persist.iter())
            .filter(|reference| !removed.persist.contains(reference))
            .map(|&reference| (reference, own))
            .chain(
                tables
                    .iter()
                    .flat_map(|table| table.unique.iter())
                    .filter(|(actor, _, _)| !removed.unique.iter().any(|(gone, _, _)| gone == actor))
                    .map(|&(_, reference, location)| (reference, location)),
            )
            .collect();
        self.ref_type_refs = tables
            .iter()
            .flat_map(|table| table.special.iter())
            .filter(|(_, reference)| !removed.special.iter().any(|(_, gone)| gone == reference))
            .copied()
            .collect();
    }

    /// This record as a partial form (record flag `0x4000`, #7275) over
    /// `previous`, the record it overrides: every sub-record group it carried
    /// replaces `previous`'s, every group it left out is inherited. `EDID` is
    /// always carried (xEdit keeps it in a partial form).
    pub fn overlaid_on(&self, previous: &LctnRecord) -> LctnRecord {
        use carried::*;
        let has = |bit: u16| self.carried & bit != 0;
        let pick = |bit: u16, own: &Vec<u32>, earlier: &Vec<u32>| if has(bit) { own.clone() } else { earlier.clone() };
        let mut out = LctnRecord {
            form_id: self.form_id,
            editor_id: self.editor_id.clone(),
            full_name: if has(FULL) { self.full_name.clone() } else { previous.full_name.clone() },
            keywords: pick(KEYWORDS, &self.keywords, &previous.keywords),
            parent: if has(PARENT) { self.parent } else { previous.parent },
            carried: self.carried | previous.carried,
            ..Default::default()
        };
        for (table, earlier, own, [persist, unique, special]) in [
            (&mut out.master, &previous.master, &self.master, [MASTER_PERSIST, MASTER_UNIQUE, MASTER_SPECIAL]),
            (&mut out.added, &previous.added, &self.added, [ADDED_PERSIST, ADDED_UNIQUE, ADDED_SPECIAL]),
            (&mut out.removed, &previous.removed, &self.removed, [REMOVED_PERSIST, REMOVED_UNIQUE, REMOVED_SPECIAL]),
        ] {
            table.persist = pick(persist, &own.persist, &earlier.persist);
            table.unique = if has(unique) { own.unique.clone() } else { earlier.unique.clone() };
            table.special = if has(special) { own.special.clone() } else { earlier.special.clone() };
        }
        out.derive_effective();
        out
    }
}

/// `*CPR` entry size before Starfield: ref, cell-or-world, grid.
const PERSIST_REF_LEN: usize = 12;
/// `*CSR` entry size before Starfield: ref type, ref, cell-or-world, grid.
const SPECIAL_REF_LEN: usize = 16;
/// Starfield's `wbUnknown(4)` tail on both entry kinds.
const STARFIELD_REF_TAIL: usize = 4;

pub fn parse_lctn(
    form_id: u32,
    subs: &[SubRecord],
    remap: &Option<FormIdRemap>,
    game: GameKind,
) -> LctnRecord {
    use carried::*;
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
            b"FULL" => out.carried |= FULL,
            b"KSIZ" => out.carried |= KEYWORDS,
            b"KWDA" => {
                out.carried |= KEYWORDS;
                while r.remaining() >= 4 {
                    out.keywords.push(remap_fid(r.u32_or_default(), remap));
                }
            }
            b"PNAM" => {
                out.carried |= PARENT;
                if sub.data.len() >= 4 {
                    let parent = remap_fid(r.u32_or_default(), remap);
                    out.parent = (parent != 0).then_some(parent);
                }
            }
            // {ref, cell/world, grid y i16, grid x i16}
            kind @ (b"ACPR" | b"LCPR") => {
                let (table, bit) = if kind == b"LCPR" { (&mut out.master, MASTER_PERSIST) } else { (&mut out.added, ADDED_PERSIST) };
                out.carried |= bit;
                while r.remaining() >= PERSIST_REF_LEN + tail {
                    let reference = r.u32_or_default();
                    let _cell = r.u32_or_default();
                    let _grid = r.u32_or_default();
                    r.skip_or_eof(tail);
                    if reference != 0 {
                        table.persist.push(remap_fid(reference, remap));
                    }
                }
            }
            b"RCPR" => {
                out.carried |= REMOVED_PERSIST;
                while r.remaining() >= 4 {
                    let reference = r.u32_or_default();
                    if reference != 0 {
                        out.removed.persist.push(remap_fid(reference, remap));
                    }
                }
            }
            // {actor base, ref, location}
            kind @ (b"ACUN" | b"LCUN") => {
                let (table, bit) = if kind == b"LCUN" { (&mut out.master, MASTER_UNIQUE) } else { (&mut out.added, ADDED_UNIQUE) };
                out.carried |= bit;
                while r.remaining() >= 12 {
                    let actor = r.u32_or_default();
                    let reference = r.u32_or_default();
                    let location = r.u32_or_default();
                    if reference != 0 {
                        let location = match location {
                            0 => form_id,
                            location => remap_fid(location, remap),
                        };
                        table.unique.push((remap_fid(actor, remap), remap_fid(reference, remap), location));
                    }
                }
            }
            b"RCUN" => {
                out.carried |= REMOVED_UNIQUE;
                while r.remaining() >= 4 {
                    let actor = r.u32_or_default();
                    if actor != 0 {
                        out.removed.unique.push((remap_fid(actor, remap), 0, 0));
                    }
                }
            }
            // {location ref type, ref, cell/world, grid y i16, grid x i16}
            kind @ (b"ACSR" | b"LCSR") => {
                let (table, bit) = if kind == b"LCSR" { (&mut out.master, MASTER_SPECIAL) } else { (&mut out.added, ADDED_SPECIAL) };
                out.carried |= bit;
                while r.remaining() >= SPECIAL_REF_LEN + tail {
                    let ref_type = r.u32_or_default();
                    let reference = r.u32_or_default();
                    let _cell = r.u32_or_default();
                    let _grid = r.u32_or_default();
                    r.skip_or_eof(tail);
                    if ref_type != 0 && reference != 0 {
                        table.special.push((remap_fid(ref_type, remap), remap_fid(reference, remap)));
                    }
                }
            }
            b"RCSR" => {
                out.carried |= REMOVED_SPECIAL;
                while r.remaining() >= 4 {
                    let reference = r.u32_or_default();
                    if reference != 0 {
                        out.removed.special.push((0, remap_fid(reference, remap)));
                    }
                }
            }
            _ => {}
        }
    }
    out.derive_effective();
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
            vec![(0xA1, own), (0xA2, own), (0xA4, 0x1_8A56), (0xA5, own)],
            "RCPR names a removed reference, not one this location holds (#7276)"
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

    /// #7276 — the Master, Added and Removed tables decode apart, and the
    /// effective set is Master and Added less Removed: `RCPR` removes a
    /// persistent ref, `RCUN` a unique NPC by actor base, `RCSR` a special ref.
    #[test]
    fn removed_tables_take_entries_out_of_the_effective_set() {
        let subs = vec![
            sub(b"LCPR", words(&[0xA1, 0x3C, 0, 0xA2, 0x3C, 0])),
            sub(b"ACPR", words(&[0xA3, 0x3C, 0])),
            sub(b"RCPR", words(&[0xA2])),
            sub(b"LCUN", words(&[0xB1, 0xC1, 0, 0xB2, 0xC2, 0])),
            sub(b"RCUN", words(&[0xB2])),
            sub(b"LCSR", words(&[0xD1, 0xE1, 0x3C, 0, 0xD2, 0xE2, 0x3C, 0])),
            sub(b"RCSR", words(&[0xE2])),
        ];
        let record = parse_lctn(7, &subs, &None, GameKind::Skyrim);
        assert_eq!(record.master.persist, vec![0xA1, 0xA2]);
        assert_eq!(record.added.persist, vec![0xA3]);
        assert_eq!(record.removed.persist, vec![0xA2]);
        assert_eq!(record.editor_location_refs, vec![(0xA1, 7), (0xA3, 7), (0xC1, 7)]);
        assert_eq!(record.ref_type_refs, vec![(0xD1, 0xE1)]);
    }

    /// #7275 — a partial form inherits every sub-record group it left out:
    /// `SFBGS003.esm`'s `LCTN 00010DFB` shape, `EDID` and an Added table over
    /// a master with a name, keywords, a parent and its own table.
    #[test]
    fn a_partial_location_inherits_what_it_leaves_out() {
        let master = parse_lctn(
            7,
            &[
                edid("CityNewAtlantis"),
                sub(b"FULL", b"New Atlantis\0".to_vec()),
                sub(b"KSIZ", words(&[1])),
                sub(b"KWDA", words(&[0x51])),
                sub(b"PNAM", words(&[0x60])),
                sub(b"LCPR", words(&[0xA1, 0x3C, 0, 0])),
            ],
            &None,
            GameKind::Starfield,
        );
        let partial = parse_lctn(7, &[edid("CityNewAtlantis"), sub(b"ACPR", words(&[0xA9, 0x3C, 0, 0]))], &None, GameKind::Starfield);
        assert!(partial.keywords.is_empty() && partial.parent.is_none(), "on its own the partial form has neither");

        let folded = partial.overlaid_on(&master);
        assert_eq!(folded.full_name, master.full_name);
        assert_eq!(folded.keywords, vec![0x51]);
        assert_eq!(folded.parent, Some(0x60));
        assert_eq!(folded.master.persist, vec![0xA1], "the master's table is inherited");
        assert_eq!(folded.added.persist, vec![0xA9]);
        assert_eq!(folded.editor_location_refs, vec![(0xA1, 7), (0xA9, 7)]);

        // A group the partial form does carry replaces the earlier one, even when empty.
        let renamed = parse_lctn(7, &[edid("CityNewAtlantis"), sub(b"KSIZ", words(&[0]))], &None, GameKind::Starfield);
        assert!(renamed.overlaid_on(&master).keywords.is_empty());
    }
}
