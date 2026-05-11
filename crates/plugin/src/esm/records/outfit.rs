//! OTFT — outfit record (Skyrim+).
//!
//! Flat list of armor or leveled-item FormIDs that define an NPC's
//! default-equipped set. NPCs reference an outfit via `DOFT`
//! (default outfit) or `SOFT` (sleeping outfit) sub-records on
//! `NPC_`. The Skyrim+ NPC equip pipeline is:
//!
//! ```text
//! NPC_.DOFT  →  OTFT  →  INAM[]  →  ARMO  →  Armature[]  →  ARMA  →  MOD2/MOD3
//! ```
//!
//! The `INAM` sub-record array entries can resolve to either ARMO
//! (direct armor) or LVLI (leveled-item rolling against the actor's
//! level at spawn). The parser captures both as raw FormIDs; the
//! consumer dereferences against `EsmIndex.items` / `leveled_items`
//! and walks accordingly.
//!
//! Record shape sourced from the xEdit project (by ElminsterAU and
//! the xEdit team, MPL-2.0):
//!
//!   <https://github.com/TES5Edit/TES5Edit>
//!
//! Specifically `Core/wbDefinitionsTES5.pas:7749-7752` at tag
//! `dev-4.1.6` (commit valid 2026-05-07). Same shape on Skyrim /
//! Skyrim SE / FO4 / FO76 / Starfield.

use crate::esm::reader::SubRecord;
use crate::esm::records::common::{read_u32_at, read_zstring};

/// Parsed OTFT record — flat array of item FormIDs (ARMO or LVLI).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OtftRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// Items that compose this outfit. Each entry resolves to ARMO
    /// (direct armor instance) or LVLI (leveled item — rolls at
    /// equip time against the actor's level).
    pub items: Vec<u32>,
}

/// Parse an OTFT record from its sub-record list. Unknown sub-records
/// are ignored; short `INAM` entries (< 4 bytes) are silently dropped.
pub fn parse_otft(form_id: u32, subs: &[SubRecord]) -> OtftRecord {
    let mut out = OtftRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"INAM" if sub.data.len() >= 4 => {
                if let Some(id) = read_u32_at(&sub.data, 0) {
                    out.items.push(id);
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

    fn inam(form_id: u32) -> SubRecord {
        mk_sub(b"INAM", form_id.to_le_bytes().to_vec())
    }

    #[test]
    fn parses_outfit_with_multiple_items() {
        // Models a real OTFT shape: `WhiterunGuardOutfit` with helmet,
        // cuirass, gauntlets, boots referenced by INAM.
        let subs = vec![
            edid("WhiterunGuardOutfit"),
            inam(0x0001_3937),
            inam(0x0001_3938),
            inam(0x0001_3939),
            inam(0x0001_393A),
        ];
        let r = parse_otft(0x0008_F09E, &subs);
        assert_eq!(r.editor_id, "WhiterunGuardOutfit");
        assert_eq!(
            r.items,
            vec![0x0001_3937, 0x0001_3938, 0x0001_3939, 0x0001_393A],
            "INAM order must round-trip verbatim"
        );
    }

    #[test]
    fn empty_outfit_round_trips_with_no_items() {
        let r = parse_otft(0x0001_2345, &[edid("EmptyOutfit")]);
        assert_eq!(r.editor_id, "EmptyOutfit");
        assert!(r.items.is_empty());
    }

    #[test]
    fn malformed_inam_short_payload_is_dropped() {
        let subs = vec![
            edid("BadOutfit"),
            mk_sub(b"INAM", vec![0xAA, 0xBB]), // only 2 bytes, not 4
            inam(0x0001_0000),
        ];
        let r = parse_otft(0x0001_0000, &subs);
        assert_eq!(
            r.items,
            vec![0x0001_0000],
            "short INAM must not panic or pollute the list"
        );
    }
}
