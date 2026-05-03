//! FLST (FormID List) — flat array of FormID references.
//!
//! A FLST is the engine's "named tuple of form IDs" primitive. The
//! Creation Engine references it from places where a single record
//! ID isn't enough but a heavyweight LVLI/LVLN leveled-list would be
//! overkill:
//!
//!   * **PERK entry-point conditions** — `IsInList <flst>` predicates
//!     filter perks like *Action Boy / Action Girl*, *Travel Light*,
//!     *Friend of the Night*, and the FNV trait hooks. Pre-#630 every
//!     `IsInList` lookup hit an empty map and returned "not in list",
//!     silently disabling ~50 vanilla FNV PERKs that gate behaviour
//!     on weapon / armour / location FLSTs.
//!   * **COBJ recipe ingredient lists** — Skyrim+ smithing /
//!     enchanting / alchemy menus filter craftable outputs by FLST
//!     membership of the player's inventory.
//!   * **CCRD / CDCK Caravan deck** — FNV's mini-game carries the
//!     deck composition as FLSTs of card form IDs; an empty list
//!     surfaces as "no cards" at the dealer.
//!   * **Quest objective filters** — radiant story dispatch keys off
//!     FLST membership when an objective allows multiple specific
//!     targets.
//!
//! **Sub-record layout** (per FNV xEdit `wbDefinitionsFNV.pas`,
//! UESP `Fallout3:Mod_File_Format/FLST`):
//!
//! - `EDID` — editor ID (z-string).
//! - `LNAM` × N — each is a `u32` FormID (4 B). Order is preserved
//!   verbatim because the engine evaluates `IsInList` and the
//!   Caravan-deck order both by index, not by membership alone.
//!   `0xFFFFFFFF` entries (null-form sentinels — content the author
//!   left unfilled in the editor) ARE retained: dropping them would
//!   shift downstream indices and break index-keyed consumers.
//!
//! Vanilla `FalloutNV.esm` ships ~340 FLST records spanning the
//! `WeapType*List`, `ArmorType*List`, and `Faction*List` families;
//! Skyrim ships ~1500. See audit `FNV-D2-02` / #630.

use crate::esm::reader::SubRecord;
use crate::esm::records::common::{read_u32_at, read_zstring};

/// Parsed FLST record — flat array of FormID references in authoring order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FlstRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// Entries from successive `LNAM` sub-records, in authoring order.
    /// `0xFFFFFFFF` (null-form sentinel) is retained — see module-level
    /// docs for why downstream index-keyed consumers depend on
    /// preserving slot positions.
    pub entries: Vec<u32>,
}

/// Parse a FLST record from its sub-record list. Unknown sub-records
/// are ignored. A short `LNAM` (< 4 bytes) is silently dropped rather
/// than panicking — mirrors the SCOL / ENCH defensive posture against
/// malformed mod content.
pub fn parse_flst(form_id: u32, subs: &[SubRecord]) -> FlstRecord {
    let mut out = FlstRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"LNAM" => {
                if let Some(id) = read_u32_at(&sub.data, 0) {
                    out.entries.push(id);
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

    fn mk_sub(code: &[u8; 4], data: Vec<u8>) -> SubRecord {
        SubRecord {
            sub_type: *code,
            data,
        }
    }

    fn edid(name: &str) -> SubRecord {
        let mut z = name.as_bytes().to_vec();
        z.push(0);
        mk_sub(b"EDID", z)
    }

    fn lnam(form_id: u32) -> SubRecord {
        mk_sub(b"LNAM", form_id.to_le_bytes().to_vec())
    }

    /// Baseline: a multi-entry FLST round-trips with EDID + every
    /// LNAM in authoring order. Models `WeapTypeAssaultCarbineList`
    /// from FNV — the audit's named regression target.
    #[test]
    fn parse_flst_collects_lnam_entries_in_order() {
        let subs = vec![
            edid("WeapTypeAssaultCarbineList"),
            lnam(0x000F_AA0F),
            lnam(0x001E_4E94),
            lnam(0x0010_0DD9),
        ];
        let rec = parse_flst(0x000F_43DD, &subs);
        assert_eq!(rec.form_id, 0x000F_43DD);
        assert_eq!(rec.editor_id, "WeapTypeAssaultCarbineList");
        assert_eq!(
            rec.entries,
            vec![0x000F_AA0F, 0x001E_4E94, 0x0010_0DD9],
            "LNAM order must be preserved verbatim"
        );
    }

    /// EDID-only FLST: no LNAMs. Vanilla data has zero such records,
    /// but a mod-authored stub must parse without panicking and
    /// surface an empty `entries`.
    #[test]
    fn parse_flst_edid_only_produces_empty_entries() {
        let subs = vec![edid("EmptyList")];
        let rec = parse_flst(0xABCD_0000, &subs);
        assert_eq!(rec.editor_id, "EmptyList");
        assert!(rec.entries.is_empty());
    }

    /// Null-form sentinel `0xFFFFFFFF` is retained — dropping it would
    /// shift downstream indices that the engine evaluates by slot
    /// position (Caravan deck composition, perk entry-point ladders).
    #[test]
    fn parse_flst_keeps_null_form_sentinels_for_slot_alignment() {
        let subs = vec![
            edid("ListWithNulls"),
            lnam(0x0001_2345),
            lnam(0xFFFF_FFFF), // null-form sentinel — author left blank
            lnam(0x0001_2347),
        ];
        let rec = parse_flst(0x0001_2300, &subs);
        assert_eq!(rec.entries.len(), 3);
        assert_eq!(rec.entries[0], 0x0001_2345);
        assert_eq!(rec.entries[1], 0xFFFF_FFFF, "null sentinel must survive");
        assert_eq!(rec.entries[2], 0x0001_2347);
    }

    /// A LNAM payload < 4 bytes is silently dropped rather than
    /// panicking — same defensive posture as SCOL / ENCH against
    /// malformed mod content.
    #[test]
    fn parse_flst_truncated_lnam_drops_silently() {
        let subs = vec![
            edid("Truncated"),
            lnam(0x0001_2345), // valid
            mk_sub(b"LNAM", vec![0u8; 2]), // truncated → dropped
            lnam(0x0001_2347), // valid
        ];
        let rec = parse_flst(0x0001_2400, &subs);
        assert_eq!(rec.entries, vec![0x0001_2345, 0x0001_2347]);
    }

    /// Stray / unknown sub-records (`OBND` or any future addition)
    /// must not corrupt the LNAM walk. Verifies the `_ => {}` arm
    /// keeps parsing past noise.
    #[test]
    fn parse_flst_ignores_unknown_subs() {
        let subs = vec![
            edid("Noisy"),
            mk_sub(b"OBND", vec![0u8; 12]),
            lnam(0x0001_2345),
            mk_sub(b"NONE", vec![0u8; 4]),
            lnam(0x0001_2346),
        ];
        let rec = parse_flst(0x0001_2500, &subs);
        assert_eq!(rec.entries, vec![0x0001_2345, 0x0001_2346]);
    }
}
