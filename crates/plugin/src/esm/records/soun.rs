//! `SOUN` — sound descriptor. EX-16 item 1 (#2372) prerequisite: a REGN
//! `RegionDataKind::Sound` entry carries a `sound_form: u32` pointing at a
//! SOUN record, but SOUN was dispatched through the long-tail
//! `parse_minimal_esm_record` (EDID + optional FULL only, #810), so there
//! was no path from that FormID to an actual archive audio file.
//!
//! **Sub-record decoded**: `FNAM` — the sound's file path, relative to
//! `Data\Sound\` (e.g. `fx\explosion.wav`). This codebase's own MUSC test
//! fixture already documents the same FNAM-as-filename convention for the
//! sibling `MUSC` record
//! (`crates/plugin/src/esm/records/misc/equipment.rs`,
//! `parse_minimal_record_picks_edid_full`: `FNAM = "music\base\maintitle.mp3"`),
//! and it is a load-bearing convention throughout this crate — `MODL`,
//! `ICON`, and every other authored-path sub-record already decode via the
//! same `read_zstring` z-string reader (`common.rs`).
//!
//! **`SNDD`/`SNDX`**: only the `Loop` playback flag is decoded (#3775 /
//! AUD-2026-08-30-D4-01) — see [`SounRecord::looping`]'s doc for the exact
//! bit and why no era/sub-record-type branch is needed to read it. The
//! rest of the attenuation-curve data (min/max distance, frequency
//! adjustment, static attenuation, priority) stays **NOT decoded**: none
//! of it is needed to resolve a FormID to a playable archive path (the
//! original EX-16 item 1 blocker) or to answer the Loop question, and
//! this project's no-guessing policy requires a real spec or corpus to
//! pin an exact byte layout before decoding it — a future consumer that
//! needs playback tuning (volume falloff, priority) should decode those
//! as their own addition. `CNAM`/`SDSC` (random-selection / descriptor
//! FormID links, FO3+) are likewise still not decoded.

use crate::esm::reader::SubRecord;
use crate::esm::records::common::{read_string_sub, CommonNamedFields};

/// Bit `0x0010` of `SNDD`/`SNDX`'s `Flags` word — "Loop". Confirmed
/// identical across Oblivion, FO3, and FNV via xEdit's
/// `wbDefinitionsTES4.pas`/`wbDefinitionsFO3.pas` record definitions
/// (#3775): the bit sits at the same byte offset in every variant of
/// this sub-record (the 8-byte Oblivion `SNDD`, the 12-byte `SNDX`
/// shared by both eras, and the 36-byte FO3+ `SNDD`) — only the field's
/// total width changed (`u16` → `u32`), never the position of this bit,
/// so a single byte-offset read works for every era without branching.
const SNDD_SNDX_FLAGS_OFFSET: usize = 4;
const SNDD_SNDX_LOOP_BIT: u8 = 0x10;

/// Parsed `SOUN` record — just enough to resolve a sound FormID to an
/// archive-relative file path, plus whether the engine should loop it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SounRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `FNAM` — file path relative to `Data\Sound\` (e.g.
    /// `fx\explosion.wav`). Empty when the record omits the sub-record
    /// (rare; only seen on placeholder/dev records).
    pub sound_path: String,
    /// `SNDD`/`SNDX` `Flags` bit `0x0010` ("Loop") — Oblivion/FO3/FNV
    /// only (see [`SNDD_SNDX_FLAGS_OFFSET`]'s doc for why no era branch
    /// is needed). `false` when neither sub-record is present, when it's
    /// too short to hold the flags byte, or when the bit is unset.
    ///
    /// Skyrim's `SOUN` carries neither sub-record meaningfully — both are
    /// `cpIgnore`d "leftover, unused" fields in xEdit's own Skyrim
    /// definition, superseded by a separate `SNDR` record's `LNAM.Looping`
    /// **enum** (`0x08`, not a composable bit, and at an unrelated offset
    /// in an unrelated record) — so this field is always `false` on
    /// Skyrim content; decoding `SNDR` is tracked separately (#3816-
    /// adjacent scope, not attempted here).
    pub looping: bool,
}

/// Parse a SOUN record from its sub-record list. Unknown sub-records are
/// ignored — see the module doc for what's deliberately deferred.
pub fn parse_soun(form_id: u32, subs: &[SubRecord]) -> SounRecord {
    let common = CommonNamedFields::from_subs(subs);
    let looping = subs
        .iter()
        .find(|s| s.sub_type == *b"SNDD" || s.sub_type == *b"SNDX")
        .and_then(|s| s.data.get(SNDD_SNDX_FLAGS_OFFSET))
        .is_some_and(|&flags_low_byte| flags_low_byte & SNDD_SNDX_LOOP_BIT != 0);
    SounRecord {
        form_id,
        editor_id: common.editor_id,
        sound_path: read_string_sub(subs, b"FNAM").unwrap_or_default(),
        looping,
    }
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

    fn zstring_sub(code: &[u8; 4], s: &str) -> SubRecord {
        let mut z = s.as_bytes().to_vec();
        z.push(0);
        mk_sub(code, z)
    }

    #[test]
    fn parse_soun_picks_edid_and_fnam_path() {
        let subs = vec![
            edid("FXExplosion01"),
            zstring_sub(b"FNAM", "fx\\explosion01.wav"),
        ];
        let s = parse_soun(0x0001_2345, &subs);
        assert_eq!(s.form_id, 0x0001_2345);
        assert_eq!(s.editor_id, "FXExplosion01");
        assert_eq!(s.sound_path, "fx\\explosion01.wav");
    }

    #[test]
    fn parse_soun_without_fnam_yields_empty_path_not_panic() {
        let subs = vec![edid("PlaceholderSound")];
        let s = parse_soun(0xDEAD_BEEF, &subs);
        assert_eq!(s.editor_id, "PlaceholderSound");
        assert_eq!(s.sound_path, "");
    }

    #[test]
    fn parse_soun_ignores_unrelated_sub_records() {
        // CNAM/SDSC are deliberately not decoded (see module doc), and
        // SNDX's attenuation-curve fields beyond the Loop bit likewise —
        // their presence must not perturb the fields that ARE decoded.
        // The all-zero SNDX here has the Loop bit unset.
        let subs = vec![
            edid("AMBWind"),
            zstring_sub(b"FNAM", "amb\\wind_loop.wav"),
            mk_sub(b"SNDX", vec![0u8; 16]),
            mk_sub(b"CNAM", 0x0001_0000u32.to_le_bytes().to_vec()),
        ];
        let s = parse_soun(0x0005_0000, &subs);
        assert_eq!(s.sound_path, "amb\\wind_loop.wav");
        assert!(!s.looping);
    }

    /// #3775 — the actual Loop flag, on the Oblivion-shaped 8-byte `SNDD`:
    /// bytes 0-3 are min/max atten + freq adj + unused, bytes 4-5 are the
    /// `Flags` `u16` with bit `0x0010` set.
    #[test]
    fn parse_soun_decodes_loop_bit_from_oblivion_sndd() {
        let mut sndd = vec![0u8; 8];
        sndd[4] = 0x10; // Flags low byte: Loop bit set
        let subs = vec![
            edid("AMBWindLoop"),
            zstring_sub(b"FNAM", "amb\\wind_loop.wav"),
            mk_sub(b"SNDD", sndd),
        ];
        let s = parse_soun(0x0006_0000, &subs);
        assert!(s.looping, "bit 0x0010 in SNDD's Flags byte must decode as looping");
    }

    /// #3775 sibling — the FO3+ 36-byte `SNDD`, where `Flags` widened to a
    /// `u32` (bytes 4-7) but the Loop bit stays at the same low-byte
    /// position, so the same byte-offset read applies unchanged.
    #[test]
    fn parse_soun_decodes_loop_bit_from_fo3_wide_sndd() {
        let mut sndd = vec![0u8; 36];
        sndd[4] = 0x10;
        let subs = vec![edid("FNVAmbLoop"), mk_sub(b"SNDD", sndd)];
        let s = parse_soun(0x0007_0000, &subs);
        assert!(s.looping);
    }

    /// Other bits in the same Flags byte (e.g. `Play At Random`, `0x02`)
    /// must not be mistaken for Loop.
    #[test]
    fn parse_soun_other_flag_bits_do_not_set_looping() {
        let mut sndd = vec![0u8; 8];
        sndd[4] = 0x02; // Play At Random, not Loop
        let subs = vec![mk_sub(b"SNDD", sndd)];
        let s = parse_soun(0x0008_0000, &subs);
        assert!(!s.looping);
    }

    /// A record with neither `SNDD` nor `SNDX` (or one too short to hold
    /// the flags byte) must not panic and must default to non-looping.
    #[test]
    fn parse_soun_without_sndd_or_sndx_is_not_looping() {
        let s = parse_soun(0x0009_0000, &[edid("NoAttenData")]);
        assert!(!s.looping);

        let truncated = vec![mk_sub(b"SNDD", vec![0u8; 3])]; // shorter than offset 4
        let s2 = parse_soun(0x0009_0001, &truncated);
        assert!(!s2.looping);
    }
}
