//! `MUSC` — a music type. Two unrelated layouts share the FourCC:
//!
//! * **Fallout 3 / New Vegas** (xEdit `wbDefinitionsFNV.pas`): `EDID`, `FNAM` (the file name, relative to
//!   the `music\` folder) and `ANAM` ("dB (positive = Loop)").
//! * **Skyrim** (xEdit `wbDefinitionsTES5.pas`): `EDID`, `FNAM` playback flags (u32), `PNAM` priority +
//!   ducking, `WNAM` fade duration and `TNAM`, an array of [`MustRecord`] FormIDs. The music type holds no
//!   audio path of its own; REGN `RDMO` names one of these (#3816).
//!
//! Other games' layouts are not decoded. [`MustRecord`] is Skyrim's `MUST` music track, the record that
//! carries the file name and loop points.

use super::super::common::{read_zstring, remap_fid, CommonNamedFields};
use crate::esm::reader::{FormIdRemap, GameKind, SubRecord};
use crate::esm::sub_reader::SubReader;
use std::collections::HashMap;

/// Skyrim `MUSC.FNAM` playback flags (xEdit `wbDefinitionsTES5.pas`).
pub const MUSC_PLAYS_ONE_SELECTION: u32 = 0x01;
pub const MUSC_ABRUPT_TRANSITION: u32 = 0x02;
pub const MUSC_CYCLE_TRACKS: u32 = 0x04;
pub const MUSC_MAINTAIN_TRACK_ORDER: u32 = 0x08;
pub const MUSC_DUCKS_CURRENT_TRACK: u32 = 0x20;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MuscRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `FNAM` — the track, relative to the `music\` folder; empty on a game whose `MUSC` is not decoded.
    pub file: String,
    /// `ANAM` — the level in dB; a positive value marks a looping track.
    pub decibels: f32,
    /// Skyrim `FNAM` — playback flags (`MUSC_*`); `0` on a game whose layout is not Skyrim's.
    pub flags: u32,
    /// Skyrim `PNAM` priority.
    pub priority: u16,
    /// Skyrim `PNAM` ducking in dB (xEdit stores it ×100).
    pub ducking_db: f32,
    /// Skyrim `WNAM` — fade duration in seconds.
    pub fade_secs: f32,
    /// Skyrim `TNAM` — the [`MustRecord`] tracks, in authored order.
    pub tracks: Vec<u32>,
}

impl MuscRecord {
    pub fn looping(&self) -> bool {
        self.decibels > 0.0
    }
}

pub fn parse_musc(form_id: u32, subs: &[SubRecord], game: GameKind) -> MuscRecord {
    parse_musc_remapped(form_id, subs, game, &None)
}

/// [`parse_musc`] with the plugin's FormID remap applied to Skyrim's `TNAM` track references.
pub fn parse_musc_remapped(
    form_id: u32,
    subs: &[SubRecord],
    game: GameKind,
    remap: &Option<FormIdRemap>,
) -> MuscRecord {
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    let mut out = MuscRecord { form_id, editor_id: common.editor_id, ..Default::default() };
    match game {
        GameKind::Fallout3NV => {
            for sub in subs {
                match &sub.sub_type {
                    b"FNAM" => out.file = read_zstring(&sub.data),
                    b"ANAM" => {
                        if let Ok(decibels) = SubReader::new(&sub.data).f32() {
                            out.decibels = decibels;
                        }
                    }
                    _ => {}
                }
            }
        }
        GameKind::Skyrim => {
            for sub in subs {
                let mut reader = SubReader::new(&sub.data);
                match &sub.sub_type {
                    b"FNAM" => out.flags = reader.u32().unwrap_or(0),
                    b"PNAM" => {
                        if let (Ok(priority), Ok(ducking)) = (reader.u16(), reader.u16()) {
                            out.priority = priority;
                            out.ducking_db = f32::from(ducking) / 100.0;
                        }
                    }
                    b"WNAM" => out.fade_secs = reader.f32().unwrap_or(0.0),
                    b"TNAM" => {
                        out.tracks = sub
                            .data
                            .chunks_exact(4)
                            .map(|chunk| remap_fid(u32::from_le_bytes(chunk.try_into().unwrap()), remap))
                            .filter(|track| *track != 0)
                            .collect();
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
    out
}

/// `MUST.CNAM` — a Skyrim music track's kind (xEdit's hashed enum).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MustKind {
    /// `0x23F678C3` — a container: [`MustRecord::sub_tracks`] holds the playable tracks.
    Palette,
    /// `0x6ED7E048` — one audio file, [`MustRecord::file`].
    SingleTrack,
    /// `0xA1A9C4D5` — deliberate silence between pieces; nothing to play.
    SilentTrack,
    #[default]
    Unknown,
    /// A value outside xEdit's enum, kept rather than folded into `Unknown`.
    Other(u32),
}

impl MustKind {
    fn from_wire(v: u32) -> Self {
        match v {
            0x23F6_78C3 => Self::Palette,
            0x6ED7_E048 => Self::SingleTrack,
            0xA1A9_C4D5 => Self::SilentTrack,
            other => Self::Other(other),
        }
    }
}

/// `MUST.LNAM` — loop begin / end (seconds) and repeat count. Decoded, not yet played: the audio
/// layer has no loop-region wiring beyond whole-track looping (#3775).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MustLoop {
    pub begin: f32,
    pub end: f32,
    pub count: u32,
}

/// Skyrim `MUST` — a music track: the file `MUSC.TNAM` ends up pointing at.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MustRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub kind: MustKind,
    /// `FLTV` — duration in seconds.
    pub duration: f32,
    /// `DNAM` — fade-out in seconds.
    pub fade_out: f32,
    /// `ANAM` — the track file; empty on a palette or silent track.
    pub file: String,
    /// `BNAM` — the finale file played when the music type ends.
    pub finale_file: String,
    /// `LNAM`.
    pub loop_data: Option<MustLoop>,
    /// `SNAM` — a palette's member tracks.
    pub sub_tracks: Vec<u32>,
}

pub fn parse_must(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> MustRecord {
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    let mut out = MustRecord { form_id, editor_id: common.editor_id, ..Default::default() };
    for sub in subs {
        let mut reader = SubReader::new(&sub.data);
        match &sub.sub_type {
            b"CNAM" => out.kind = reader.u32().map_or(MustKind::Unknown, MustKind::from_wire),
            b"FLTV" => out.duration = reader.f32().unwrap_or(0.0),
            b"DNAM" => out.fade_out = reader.f32().unwrap_or(0.0),
            b"ANAM" => out.file = read_zstring(&sub.data),
            b"BNAM" => out.finale_file = read_zstring(&sub.data),
            b"LNAM" => {
                if let (Ok(begin), Ok(end), Ok(count)) = (reader.f32(), reader.f32(), reader.u32()) {
                    out.loop_data = Some(MustLoop { begin, end, count });
                }
            }
            b"SNAM" => {
                out.sub_tracks = sub
                    .data
                    .chunks_exact(4)
                    .map(|chunk| remap_fid(u32::from_le_bytes(chunk.try_into().unwrap()), remap))
                    .filter(|track| *track != 0)
                    .collect();
            }
            _ => {}
        }
    }
    out
}

/// How deep a palette may nest before the walk gives up (vanilla nests one level).
const MUST_PALETTE_DEPTH_CAP: u32 = 4;

/// The track a Skyrim `MUSC` plays, in the one deterministic case this engine models: the first
/// `SingleTrack` with a file, in `TNAM` order, descending into a palette's `SNAM` members in order.
///
/// This is *a valid pick* under every flag combination, not the whole policy: `Plays One Selection`
/// and an ordered `Cycle Tracks` both start at the first track, and an unordered cycle may start
/// anywhere. Cycling, random order, finale tracks and loop regions are not modelled (#3816).
pub fn first_playable_track<'a>(
    musc: &MuscRecord,
    tracks: &'a HashMap<u32, MustRecord>,
) -> Option<&'a MustRecord> {
    fn walk(form_id: u32, tracks: &HashMap<u32, MustRecord>, depth: u32) -> Option<&MustRecord> {
        let track = tracks.get(&form_id)?;
        match track.kind {
            MustKind::SingleTrack if !track.file.is_empty() => Some(track),
            MustKind::Palette if depth < MUST_PALETTE_DEPTH_CAP => {
                track.sub_tracks.iter().find_map(|&member| walk(member, tracks, depth + 1))
            }
            _ => None,
        }
    }
    musc.tracks.iter().find_map(|&form_id| walk(form_id, tracks, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::records::test_support::sub;

    #[test]
    fn a_new_vegas_music_type_decodes_its_file_and_loop_flag() {
        let subs = vec![
            sub(b"EDID", b"musDocMitchellHouse\0"),
            sub(b"FNAM", b"loc\\corporateruins\\mus_loc_corpruins_day_3high.mp3\0"),
            sub(b"ANAM", 6.0f32.to_le_bytes()),
        ];
        let musc = parse_musc(0x171b4a, &subs, GameKind::Fallout3NV);
        assert_eq!(musc.editor_id, "musDocMitchellHouse");
        assert_eq!(musc.file, "loc\\corporateruins\\mus_loc_corpruins_day_3high.mp3");
        assert!(musc.looping());
        let stinger = parse_musc(1, &[sub(b"ANAM", (-0.0001f32).to_le_bytes()), sub(b"FNAM", b"a.mp3\0")], GameKind::Fallout3NV);
        assert!(!stinger.looping());
        assert_eq!(parse_musc(2, &subs, GameKind::Skyrim).file, "", "another game's layout is not read");
    }

    #[test]
    fn a_truncated_decibel_field_is_skipped_not_indexed() {
        let truncated = parse_musc(3, &[sub(b"ANAM", [0u8, 0, 0x80])], GameKind::Fallout3NV);
        assert_eq!(truncated.decibels, 0.0);
    }

    fn musc_with(tracks: Vec<u32>) -> MuscRecord {
        MuscRecord { tracks, ..Default::default() }
    }

    fn must(form_id: u32, kind: MustKind, file: &str, sub_tracks: Vec<u32>) -> (u32, MustRecord) {
        (form_id, MustRecord { form_id, kind, file: file.into(), sub_tracks, ..Default::default() })
    }

    #[test]
    fn a_skyrim_music_type_decodes_flags_ducking_fade_and_track_list() {
        let mut tnam = Vec::new();
        tnam.extend_from_slice(&0x0010_0001u32.to_le_bytes());
        tnam.extend_from_slice(&0x0010_0002u32.to_le_bytes());
        let mut pnam = Vec::new();
        pnam.extend_from_slice(&40u16.to_le_bytes());
        pnam.extend_from_slice(&1200u16.to_le_bytes());
        let subs = vec![
            sub(b"EDID", b"MUSExploreTundra\0"),
            sub(b"FNAM", (MUSC_CYCLE_TRACKS | MUSC_MAINTAIN_TRACK_ORDER).to_le_bytes()),
            sub(b"PNAM", pnam),
            sub(b"WNAM", 4.5f32.to_le_bytes()),
            sub(b"TNAM", tnam),
        ];
        let musc = parse_musc(0x1234, &subs, GameKind::Skyrim);
        assert_eq!(musc.editor_id, "MUSExploreTundra");
        assert_eq!(musc.flags, 0x0C);
        assert_eq!((musc.priority, musc.ducking_db, musc.fade_secs), (40, 12.0, 4.5));
        assert_eq!(musc.tracks, vec![0x0010_0001, 0x0010_0002]);
        assert_eq!(musc.file, "", "Skyrim FNAM is flags, never a path");
    }

    #[test]
    fn a_skyrim_music_track_decodes_kind_file_and_loop_points() {
        let mut lnam = Vec::new();
        lnam.extend_from_slice(&1.5f32.to_le_bytes());
        lnam.extend_from_slice(&90.0f32.to_le_bytes());
        lnam.extend_from_slice(&2u32.to_le_bytes());
        let subs = vec![
            sub(b"EDID", b"MUSTundraA\0"),
            sub(b"CNAM", 0x6ED7_E048u32.to_le_bytes()),
            sub(b"FLTV", 120.0f32.to_le_bytes()),
            sub(b"ANAM", b"music\\explore\\tundra_a.xwm\0"),
            sub(b"LNAM", lnam),
        ];
        let track = parse_must(0x2000, &subs, &None);
        assert_eq!(track.kind, MustKind::SingleTrack);
        assert_eq!(track.file, "music\\explore\\tundra_a.xwm");
        assert_eq!(track.loop_data, Some(MustLoop { begin: 1.5, end: 90.0, count: 2 }));
        let palette = parse_must(1, &[sub(b"CNAM", 0x23F6_78C3u32.to_le_bytes())], &None);
        assert_eq!(palette.kind, MustKind::Palette);
        assert_eq!(parse_must(2, &[sub(b"CNAM", 7u32.to_le_bytes())], &None).kind, MustKind::Other(7));
    }

    #[test]
    fn a_truncated_skyrim_field_is_skipped_not_indexed() {
        let musc = parse_musc(1, &[sub(b"PNAM", [1u8, 0]), sub(b"WNAM", [0u8, 0])], GameKind::Skyrim);
        assert_eq!((musc.priority, musc.fade_secs), (0, 0.0));
        assert_eq!(parse_must(2, &[sub(b"LNAM", [0u8; 8])], &None).loop_data, None);
    }

    #[test]
    fn the_first_playable_track_descends_a_palette_and_skips_silence() {
        let tracks: HashMap<u32, MustRecord> = [
            must(1, MustKind::SilentTrack, "", vec![]),
            must(2, MustKind::Palette, "", vec![4, 3]),
            must(3, MustKind::SingleTrack, "music\\b.xwm", vec![]),
            must(4, MustKind::SingleTrack, "", vec![]), // single track with no file: not playable
        ]
        .into_iter()
        .collect();
        let picked = first_playable_track(&musc_with(vec![1, 2]), &tracks).expect("palette member");
        assert_eq!(picked.form_id, 3, "silence skipped, empty-file member skipped, next member taken");
        assert!(first_playable_track(&musc_with(vec![1, 99]), &tracks).is_none());
        assert!(first_playable_track(&musc_with(vec![]), &tracks).is_none());
    }

    #[test]
    fn a_palette_cycle_terminates() {
        let tracks: HashMap<u32, MustRecord> = [must(1, MustKind::Palette, "", vec![2]), must(2, MustKind::Palette, "", vec![1])]
            .into_iter()
            .collect();
        assert!(first_playable_track(&musc_with(vec![1]), &tracks).is_none());
    }
}
