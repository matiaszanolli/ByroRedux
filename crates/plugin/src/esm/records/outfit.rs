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

use crate::esm::reader::{FormIdRemap, SubRecord};
use crate::esm::records::common::CommonNamedFields;

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

/// Remap a raw plugin-local FormID to global space, leaving 0 (no
/// FormID / null ref) untouched. Same convention as `actor.rs` / `misc/
/// ai.rs`'s `remap_fid` — kept local rather than shared since neither
/// module depends on the other's record types.
fn remap_fid(raw: u32, remap: &Option<FormIdRemap>) -> u32 {
    if raw == 0 {
        return 0;
    }
    remap.as_ref().map_or(raw, |r| r.remap(raw))
}

/// Parse an OTFT record from its sub-record list. Unknown sub-records
/// are ignored; short `INAM` entries (< 4 bytes) are silently dropped.
///
/// `remap` promotes each `INAM` entry from plugin-local to global
/// FormID space, matching how `EsmIndex.items` / `.leveled_items` are
/// keyed (#1996 / FNV-D4-01 / #2079) — without it, an outfit defined in
/// a non-base plugin whose `INAM` entries reference content in that same
/// plugin resolves against the wrong (global-keyed) map and the NPC
/// spawns with no equipped outfit.
pub fn parse_otft(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> OtftRecord {
    let mut out = OtftRecord {
        form_id,
        ..Default::default()
    };
    // #2414 / TD2-117 SIBLING — same swap as the `misc/` family; these two
    // sites sit outside it and were not named in the issue.
    let common = CommonNamedFields::from_subs(subs);
    out.editor_id = common.editor_id;
    for sub in subs {
        match &sub.sub_type {
            // #3356 — `INAM` is ONE sub-record holding an ARRAY of 4-byte
            // FormIDs (xEdit `wbDefinitionsTES5.pas`: `wbArray(INAM,
            // 'Items', wbFormIDCk('Item', [ARMO, LVLI]))`), not one FormID
            // per sub-record. Reading a single `u32` consumed the first
            // entry and discarded bytes 4..N, so every outfit yielded
            // exactly one item however many it authored — 765 of 1,246
            // Skyrim.esm items (61%) dropped, 387 of 481 outfits truncated.
            // The old `len() >= 4` guard made it invisible: a 20-byte
            // 5-item array passes it and contributes one item.
            //
            // `chunks_exact(4)` keeps the existing "short / trailing bytes
            // are dropped" contract that `malformed_inam_short_payload_is_dropped`
            // pins.
            b"INAM" => {
                for chunk in sub.data.chunks_exact(4) {
                    let id = u32::from_le_bytes(chunk.try_into().expect("chunks_exact(4)"));
                    out.items.push(remap_fid(id, remap));
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

    /// One `INAM` sub-record holding N FormIDs — the shape the games
    /// actually emit.
    fn inam_array(form_ids: &[u32]) -> SubRecord {
        let mut data = Vec::with_capacity(form_ids.len() * 4);
        for id in form_ids {
            data.extend_from_slice(&id.to_le_bytes());
        }
        mk_sub(b"INAM", data)
    }

    /// #3356 — the real wire shape: a SINGLE `INAM` carrying an array.
    /// This replaces a fixture that modelled four separate 4-byte `INAM`
    /// sub-records under a comment asserting it was "a real OTFT shape"
    /// for `WhiterunGuardOutfit` — there is no such EditorID and no
    /// FormID `0008F09E` in `Skyrim.esm`, and every one of its 481
    /// outfits carries exactly one `INAM`. That fabricated shape is
    /// precisely why the array bug survived: it exercised the one-FormID-
    /// per-sub-record path the parser implemented.
    ///
    /// `ArmorBandedIronAllOutfit` (`000B1FAE`) is real, and its 16-byte
    /// `INAM` is transcribed verbatim from `Skyrim.esm`.
    #[test]
    fn parses_a_single_inam_array_of_items() {
        let subs = vec![
            edid("ArmorBandedIronAllOutfit"),
            inam_array(&[0x0001_3948, 0x0001_2E46, 0x0001_2E4B, 0x0001_2E4D]),
        ];
        let r = parse_otft(0x000B_1FAE, &subs, &None);
        assert_eq!(r.editor_id, "ArmorBandedIronAllOutfit");
        assert_eq!(
            r.items,
            vec![0x0001_3948, 0x0001_2E46, 0x0001_2E4B, 0x0001_2E4D],
            "all four array entries must survive, in authored order (#3356)"
        );
    }

    /// The Bannered Mare outfits, verbatim from `Skyrim.esm` — the bench
    /// cell this defect was traced on. Each is one `INAM` array.
    #[test]
    fn parses_the_bannered_mare_outfit_arrays() {
        for (edid_name, fid, items) in [
            (
                "FarmClothesOutfit02",
                0x0002_D75E_u32,
                vec![0x0002_09A5, 0x0002_09A6],
            ),
            (
                "BarkeepClothes01",
                0x0005_FB81,
                vec![0x0005_B6A1, 0x0005_B6A0],
            ),
            (
                "BeggarWithHatOutfit",
                0x0002_8B61,
                vec![0x0001_3105, 0x0001_3106, 0x0001_3104],
            ),
            (
                "FineClothesOutfit02",
                0x000E_40DD,
                vec![0x000C_EE82, 0x000C_EE80],
            ),
        ] {
            let subs = vec![edid(edid_name), inam_array(&items)];
            let r = parse_otft(fid, &subs, &None);
            assert_eq!(r.items, items, "{edid_name} lost items (#3356)");
        }
    }

    /// Several `INAM` sub-records, each an array, concatenate. Not a
    /// shape vanilla emits, but the parser must not lose data if a mod
    /// authors it.
    #[test]
    fn multiple_inam_sub_records_concatenate() {
        let subs = vec![
            edid("ModdedOutfit"),
            inam_array(&[0x0000_0001, 0x0000_0002]),
            inam(0x0000_0003),
        ];
        let r = parse_otft(0x0000_00FF, &subs, &None);
        assert_eq!(r.items, vec![1, 2, 3]);
    }

    #[test]
    fn empty_outfit_round_trips_with_no_items() {
        let r = parse_otft(0x0001_2345, &[edid("EmptyOutfit")], &None);
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
        let r = parse_otft(0x0001_0000, &subs, &None);
        assert_eq!(
            r.items,
            vec![0x0001_0000],
            "short INAM must not panic or pollute the list"
        );
    }

    /// FNV-D4-01 / #2079 — every `INAM` entry must land in global
    /// load-order space, matching how `EsmIndex.items` / `.leveled_items`
    /// are keyed. Pre-fix, `parse_otft` never threaded a remap at all, so
    /// an outfit defined in a non-base plugin whose `INAM` entries
    /// reference armor/leveled-list content in that same plugin resolved
    /// against the wrong (global-keyed) map — the NPC spawns gearless.
    #[test]
    fn otft_embedded_form_ids_remap_to_global_space() {
        // Plugin slot 2, one master at slot 0.
        let remap = crate::esm::reader::FormIdRemap::regular(2, vec![0]);
        // mod_index 1 == master_slots.len() → self-reference (armor this
        // override plugin defines itself).
        let self_ref = (1u32 << 24) | 0x0000_1234;
        // mod_index 0 → the master's slot (a base-game ARMO).
        let master_ref: u32 = 0x0000_5678;

        let subs = vec![
            edid("OverridePluginOutfit"),
            inam(master_ref),
            inam(self_ref),
        ];
        let r = parse_otft(0x000A_0001, &subs, &Some(remap));

        assert_eq!(
            r.items[0], master_ref,
            "master-slot reference (mod_index 0) stays at slot 0's byte"
        );
        assert_eq!(
            r.items[1],
            (2u32 << 24) | 0x0000_1234,
            "self-reference (mod_index == master count) remaps to this plugin's own slot 2"
        );
    }
}
