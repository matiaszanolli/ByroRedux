//! `SOUN` — sound descriptor. EX-16 item 1 (#2372) prerequisite: a REGN
//! `RegionDataKind::Sound` entry carries a `sound_form: u32` pointing at a
//! SOUN record, but SOUN was dispatched through the long-tail
//! `parse_minimal_esm_record` (EDID + optional FULL only, #810), so there
//! was no path from that FormID to an actual archive audio file.
//!
//! **Sub-record decoded**: `FNAM` — the sound's path relative to
//! `Data\Sound\`, which is **either a file or a folder** (#3914):
//!
//! - file form, e.g. `fx\explosion.wav` — one archive entry;
//! - folder form, e.g. `fx\amb\ceilingcrumble\` (trailing separator) — a
//!   directory of variant `.wav`s the retail engine picks from at random
//!   on each play.
//!
//! The folder form is the *majority* convention on the reference title,
//! not a data typo: an ESM census of `FalloutNV.esm` (2026-09-06) found
//! 1,620 of the 3,189 FNAM-bearing `SOUN` records (50.8 %) end in `\`,
//! 103 of them footstep sets (`FSTConcSolidJump` →
//! `fx\fst\conc_broken\jump\`); `Fallout3.esm` is 573 of 1,556 (36.8 %).
//! Every folder-form value ends in a backslash — none use `/`, and no
//! extension-less value names a folder without the trailing separator.
//! [`SounRecord::is_folder`] carries the distinction so a consumer never
//! hands a folder to an archive `extract` as if it were an entry key
//! (a guaranteed miss indistinguishable from genuine missing content).
//! Picking a variant needs a policy decision (deterministic-per-emitter
//! vs. per-play) that no consumer has made yet, so this crate exposes the
//! distinction and nothing more.
//!
//! The sibling `MUSC` record's `FNAM` is file-form only
//! (`crates/plugin/src/esm/records/misc/equipment.rs`,
//! `parse_minimal_record_picks_edid_full`: `FNAM = "music\base\maintitle.mp3"`);
//! `MODL`/`ICON` are always files. All decode via the same `read_zstring`
//! z-string reader (`common.rs`) — only the *meaning* of `SOUN.FNAM`
//! differs.
//!
//! **`SNDD`/`SNDX`**: the `Loop` playback flag (#3775 /
//! AUD-2026-08-30-D4-01), the min/max attenuation distance and the
//! `2D` / `Menu Sound` flags (#6281) are decoded — see
//! [`SounRecord::looping`], [`SounRecord::attenuation_units`] and
//! [`SounRecord::non_positional`] for the exact bytes and why no
//! era/sub-record-type branch is needed to read them. The rest
//! (frequency adjustment, static attenuation, the attenuation curve,
//! priority, loop points) stays **NOT decoded**: none of it has a
//! consumer, and this project's no-guessing policy requires a real spec
//! or corpus to pin an exact byte layout before decoding it. `CNAM`/`SDSC`
//! (random-selection / descriptor FormID links, FO3+) are likewise still
//! not decoded.

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
/// `Menu Sound` (`0x20`) and `2D` (`0x40`) — same flags word, same low
/// byte (xEdit `wbDefinitionsTES4/FO3/FNV.pas`).
const SNDD_SNDX_NON_POSITIONAL_BITS: u8 = 0x20 | 0x40;
/// `SNDD`/`SNDX` bytes 0 and 1: minimum / maximum attenuation distance,
/// stored as `u8` and multiplied by 5 and 100 game units respectively
/// (xEdit `wbMul(5)` / `wbMul(100)`, identical across Oblivion, FO3 and
/// FNV).
const SNDD_SNDX_MIN_ATTENUATION_OFFSET: usize = 0;
const SNDD_SNDX_MAX_ATTENUATION_OFFSET: usize = 1;
const SNDD_MIN_ATTENUATION_UNIT: f32 = 5.0;
const SNDD_MAX_ATTENUATION_UNIT: f32 = 100.0;

/// Parsed `SOUN` record — just enough to resolve a sound FormID to an
/// archive-relative path (file or folder — see [`Self::is_folder`]), plus
/// whether the engine should loop it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SounRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `FNAM` — path relative to `Data\Sound\`, verbatim. Either a file
    /// (`fx\explosion.wav`) or, when it ends in a path separator, a folder
    /// of variant files (`fx\amb\ceilingcrumble\`) — see the module doc
    /// and [`Self::is_folder`] (#3914). Empty when the record omits the
    /// sub-record (rare; only seen on placeholder/dev records).
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
    /// `SNDD`/`SNDX` minimum / maximum attenuation distance in game
    /// (Bethesda) units: bytes 0 and 1 scaled ×5 and ×100. `None` when
    /// the sub-record is absent or too short, and when the maximum byte is
    /// zero (a sound that is audible nowhere is an unauthored default, not
    /// a request to be silent). Skyrim `SNDR` attenuation lives in a
    /// separate output-model record and is not decoded: always `None`.
    pub attenuation_units: Option<(f32, f32)>,
    /// `SNDD`/`SNDX` `Flags` bit `0x20` ("Menu Sound") or `0x40` ("2D"):
    /// the sound is not placed in the world. Same byte as [`Self::looping`].
    pub non_positional: bool,
    /// Every `ANAM` file of a Skyrim+ `SNDR` sound descriptor, in authored
    /// order (CK: the descriptor's sounds; the engine plays one of them). Empty
    /// for a `SOUN`, whose single path is [`Self::sound_path`]. For a `SNDR`,
    /// `sound_path` is the first of these.
    pub variants: Vec<String>,
}

impl SounRecord {
    /// `true` when `FNAM` names a folder of variant files rather than one
    /// file — the value ends in a path separator (#3914). A folder is not
    /// an archive entry key: passing it to an archive `extract` is a
    /// guaranteed miss, so consumers must branch on this before resolving.
    /// Vanilla data only ever uses `\`, but `/` is accepted too, matching
    /// the separator normalisation every path consumer already applies.
    /// `false` for the empty (no-`FNAM`) path.
    pub fn is_folder(&self) -> bool {
        self.sound_path.ends_with('\\') || self.sound_path.ends_with('/')
    }
}

/// Parse a SOUN record from its sub-record list. Unknown sub-records are
/// ignored — see the module doc for what's deliberately deferred.
pub fn parse_soun(form_id: u32, subs: &[SubRecord]) -> SounRecord {
    let common = CommonNamedFields::from_subs_with_remap(subs, &None);
    let sndd = subs
        .iter()
        .find(|s| s.sub_type == *b"SNDD" || s.sub_type == *b"SNDX")
        .map(|s| s.data.as_slice());
    let flags_low_byte = sndd.and_then(|d| d.get(SNDD_SNDX_FLAGS_OFFSET)).copied();
    let attenuation_units = sndd.and_then(|d| {
        let min = *d.get(SNDD_SNDX_MIN_ATTENUATION_OFFSET)?;
        let max = *d.get(SNDD_SNDX_MAX_ATTENUATION_OFFSET)?;
        (max != 0).then(|| {
            (
                f32::from(min) * SNDD_MIN_ATTENUATION_UNIT,
                f32::from(max) * SNDD_MAX_ATTENUATION_UNIT,
            )
        })
    });
    SounRecord {
        form_id,
        editor_id: common.editor_id,
        sound_path: read_string_sub(subs, b"FNAM").unwrap_or_default(),
        looping: flags_low_byte.is_some_and(|f| f & SNDD_SNDX_LOOP_BIT != 0),
        attenuation_units,
        non_positional: flags_low_byte.is_some_and(|f| f & SNDD_SNDX_NON_POSITIONAL_BITS != 0),
        variants: Vec::new(),
    }
}

/// `LNAM.Looping` value for "Loop" in a `SNDR` (xEdit `wbDefinitionsTES5.pas`:
/// 0 none, 8 loop, 16 envelope fast, 32 envelope slow).
const SNDR_LOOPING_LOOP: u8 = 8;

/// Parse a Skyrim+ `SNDR` sound descriptor into the same [`SounRecord`] a
/// `SOUN` yields: its repeated `ANAM` sound files (xEdit's
/// `wbSoundDescriptorSounds`) and the `LNAM` looping byte. The first file is
/// the record's `sound_path`; the rest are [`SounRecord::variants`]. Output
/// model, category, alternate-sound and conditions are not decoded.
pub fn parse_sndr(form_id: u32, subs: &[SubRecord]) -> SounRecord {
    let common = CommonNamedFields::from_subs_with_remap(subs, &None);
    let variants: Vec<String> = subs
        .iter()
        .filter(|s| s.sub_type == *b"ANAM")
        .filter_map(|s| {
            let end = s.data.iter().position(|&b| b == 0).unwrap_or(s.data.len());
            let text = String::from_utf8_lossy(&s.data[..end]).into_owned();
            (!text.is_empty()).then_some(text)
        })
        .collect();
    // `LNAM` (Values): unknown byte, looping byte, unknown, rumble.
    let looping = subs
        .iter()
        .find(|s| s.sub_type == *b"LNAM")
        .and_then(|s| s.data.get(1))
        .is_some_and(|&byte| byte == SNDR_LOOPING_LOOP);
    SounRecord {
        form_id,
        editor_id: common.editor_id,
        sound_path: variants.first().cloned().unwrap_or_default(),
        looping,
        attenuation_units: None,
        non_positional: false,
        variants,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::records::test_support::{edid, sub, zstring};

    #[test]
    fn parse_soun_picks_edid_and_fnam_path() {
        let subs = vec![
            edid("FXExplosion01"),
            zstring(b"FNAM", "fx\\explosion01.wav"),
        ];
        let s = parse_soun(0x0001_2345, &subs);
        assert_eq!(s.form_id, 0x0001_2345);
        assert_eq!(s.editor_id, "FXExplosion01");
        assert_eq!(s.sound_path, "fx\\explosion01.wav");
    }

    #[test]
    fn parse_sndr_collects_the_anam_files_and_the_lnam_loop() {
        let subs = vec![
            edid("UIHammer"),
            zstring(b"ANAM", "fx\\ui\\hammer_a.wav"),
            zstring(b"ANAM", "fx\\ui\\hammer_b.wav"),
            crate::esm::records::test_support::sub(b"LNAM", [0, 8, 0, 0]),
        ];
        let s = parse_sndr(0x0010_0001, &subs);
        assert_eq!(s.editor_id, "UIHammer");
        assert_eq!(s.sound_path, "fx\\ui\\hammer_a.wav");
        assert_eq!(s.variants, ["fx\\ui\\hammer_a.wav", "fx\\ui\\hammer_b.wav"]);
        assert!(s.looping);
        assert!(!parse_sndr(1, &[edid("Quiet")]).looping);
    }

    #[test]
    fn parse_soun_without_fnam_yields_empty_path_not_panic() {
        let subs = vec![edid("PlaceholderSound")];
        let s = parse_soun(0xDEAD_BEEF, &subs);
        assert_eq!(s.editor_id, "PlaceholderSound");
        assert_eq!(s.sound_path, "");
        assert!(!s.is_folder(), "an absent FNAM is neither file nor folder");
    }

    /// #3914 — the folder form (`FNAM` ending in `\`) is how 50.8 % of
    /// FNV's `SOUN` records are authored (`EMTCeilingCrumble` →
    /// `fx\amb\ceilingcrumble\`). The path is kept verbatim and the
    /// record reports it as a folder so no consumer hands it to an
    /// archive `extract` as if it were an entry key.
    #[test]
    fn parse_soun_folder_form_fnam_is_flagged_as_folder() {
        let subs = vec![
            edid("EMTCeilingCrumble"),
            zstring(b"FNAM", "fx\\amb\\ceilingcrumble\\"),
        ];
        let s = parse_soun(0x000A_0000, &subs);
        assert_eq!(s.sound_path, "fx\\amb\\ceilingcrumble\\");
        assert!(s.is_folder());
    }

    /// #3914 sibling — a forward-slash trailing separator (non-vanilla
    /// authoring; every vanilla folder-form value uses `\`) is a folder
    /// too, consistent with the `/`→`\` normalisation consumers apply.
    #[test]
    fn parse_soun_forward_slash_folder_form_is_flagged_as_folder() {
        let subs = vec![zstring(b"FNAM", "fx/amb/ceilingcrumble/")];
        assert!(parse_soun(0x000A_0001, &subs).is_folder());
    }

    /// #3914 — the file form (an extension, no trailing separator) is not
    /// a folder, even when its parent directory is one that other records
    /// reference in folder form.
    #[test]
    fn parse_soun_file_form_fnam_is_not_a_folder() {
        let subs = vec![zstring(b"FNAM", "fx\\amb\\ceilingcrumble\\amb_ceilingcrumble_01.wav")];
        assert!(!parse_soun(0x000A_0002, &subs).is_folder());
    }

    #[test]
    fn parse_soun_ignores_unrelated_sub_records() {
        // CNAM/SDSC are deliberately not decoded (see module doc), and
        // SNDX's attenuation-curve fields beyond the Loop bit likewise —
        // their presence must not perturb the fields that ARE decoded.
        // The all-zero SNDX here has the Loop bit unset.
        let subs = vec![
            edid("AMBWind"),
            zstring(b"FNAM", "amb\\wind_loop.wav"),
            sub(b"SNDX", vec![0u8; 16]),
            sub(b"CNAM", 0x0001_0000u32.to_le_bytes()),
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
            zstring(b"FNAM", "amb\\wind_loop.wav"),
            sub(b"SNDD", sndd),
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
        let subs = vec![edid("FNVAmbLoop"), sub(b"SNDD", sndd)];
        let s = parse_soun(0x0007_0000, &subs);
        assert!(s.looping);
    }

    /// Other bits in the same Flags byte (e.g. `Play At Random`, `0x02`)
    /// must not be mistaken for Loop.
    #[test]
    fn parse_soun_other_flag_bits_do_not_set_looping() {
        let mut sndd = vec![0u8; 8];
        sndd[4] = 0x02; // Play At Random, not Loop
        let subs = vec![sub(b"SNDD", sndd)];
        let s = parse_soun(0x0008_0000, &subs);
        assert!(!s.looping);
    }

    /// #6281 — min/max attenuation are bytes 0/1 scaled ×5 / ×100 on the
    /// Oblivion 8-byte and the FO3/FNV 36-byte layouts alike, and `Menu
    /// Sound` / `2D` are bits `0x20` / `0x40` of the same flags byte as Loop.
    #[test]
    fn parse_soun_decodes_attenuation_distances_and_non_positional_flags() {
        for len in [8usize, 36] {
            let mut sndd = vec![0u8; len];
            sndd[0] = 4; // min ×5  = 20
            sndd[1] = 25; // max ×100 = 2500
            let s = parse_soun(1, &[sub(b"SNDD", sndd.clone())]);
            assert_eq!(s.attenuation_units, Some((20.0, 2500.0)), "len {len}");
            assert!(!s.non_positional && !s.looping);

            for bit in [0x20u8, 0x40] {
                sndd[4] = bit;
                assert!(parse_soun(1, &[sub(b"SNDD", sndd.clone())]).non_positional);
            }
            // Environment Ignored (0x04) is neither.
            sndd[4] = 0x04;
            assert!(!parse_soun(1, &[sub(b"SNDD", sndd)]).non_positional);
        }
        // The 12-byte shared SNDX shape reads the same bytes.
        let mut sndx = vec![0u8; 12];
        sndx[0] = 1;
        sndx[1] = 1;
        assert_eq!(
            parse_soun(1, &[sub(b"SNDX", sndx)]).attenuation_units,
            Some((5.0, 100.0))
        );
    }

    /// An absent / truncated sub-record, or a zero maximum, is "not
    /// authored" — never a zero-metre audible radius.
    #[test]
    fn parse_soun_without_usable_attenuation_is_none() {
        assert_eq!(parse_soun(1, &[edid("x")]).attenuation_units, None);
        assert_eq!(parse_soun(1, &[sub(b"SNDD", vec![7u8])]).attenuation_units, None);
        assert_eq!(parse_soun(1, &[sub(b"SNDD", vec![7u8, 0, 0, 0, 0])]).attenuation_units, None);
        assert_eq!(parse_sndr(1, &[edid("y")]).attenuation_units, None);
    }

    /// A record with neither `SNDD` nor `SNDX` (or one too short to hold
    /// the flags byte) must not panic and must default to non-looping.
    #[test]
    fn parse_soun_without_sndd_or_sndx_is_not_looping() {
        let s = parse_soun(0x0009_0000, &[edid("NoAttenData")]);
        assert!(!s.looping);

        let truncated = vec![sub(b"SNDD", vec![0u8; 3])]; // shorter than offset 4
        let s2 = parse_soun(0x0009_0001, &truncated);
        assert!(!s2.looping);
    }
}
