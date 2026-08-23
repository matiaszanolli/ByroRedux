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
//! **Deliberately NOT decoded here**: `SNDD`/`SNDX` (legacy Oblivion /
//! FO3+ attenuation-curve sound data — min/max distance, frequency
//! adjustment, static attenuation, priority) and `CNAM`/`SDSC` (random-
//! selection / descriptor FormID links, FO3+). None of that is needed to
//! resolve a FormID to a playable archive path, which is the actual EX-16
//! item 1 blocker, and this project's no-guessing policy requires a real
//! spec or corpus to pin an exact byte layout before decoding binary
//! attenuation data — none was available this session. A future consumer
//! that needs playback tuning (volume falloff, priority) should decode
//! those as their own addition, not bundled speculatively here.

use crate::esm::reader::SubRecord;
use crate::esm::records::common::{read_string_sub, CommonNamedFields};

/// Parsed `SOUN` record — just enough to resolve a sound FormID to an
/// archive-relative file path.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SounRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `FNAM` — file path relative to `Data\Sound\` (e.g.
    /// `fx\explosion.wav`). Empty when the record omits the sub-record
    /// (rare; only seen on placeholder/dev records).
    pub sound_path: String,
}

/// Parse a SOUN record from its sub-record list. Unknown sub-records are
/// ignored — see the module doc for what's deliberately deferred.
pub fn parse_soun(form_id: u32, subs: &[SubRecord]) -> SounRecord {
    let common = CommonNamedFields::from_subs(subs);
    SounRecord {
        form_id,
        editor_id: common.editor_id,
        sound_path: read_string_sub(subs, b"FNAM").unwrap_or_default(),
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
        // SNDX/CNAM/SDSC are deliberately not decoded (see module doc) —
        // their presence must not perturb the fields that ARE decoded.
        let subs = vec![
            edid("AMBWind"),
            zstring_sub(b"FNAM", "amb\\wind_loop.wav"),
            mk_sub(b"SNDX", vec![0u8; 16]),
            mk_sub(b"CNAM", 0x0001_0000u32.to_le_bytes().to_vec()),
        ];
        let s = parse_soun(0x0005_0000, &subs);
        assert_eq!(s.sound_path, "amb\\wind_loop.wav");
    }
}
