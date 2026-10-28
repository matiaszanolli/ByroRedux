//! `MSET` — a Fallout: New Vegas media set: the music container REGN `RDSI`/`RDSB` point at (#3816).
//!
//! Layout per xEdit's `wbDefinitionsFNV.pas` `MSET` (no other game defines the record): `EDID`,
//! `FULL`, `NAM1` type, six track paths `NAM2`–`NAM7`, six levels `NAM8`/`NAM9`/`NAM0`/`ANAM`/
//! `BNAM`/`CNAM`, six layer boundaries `JNAM`–`ONAM`, `PNAM` enable flags, four timing floats
//! `DNAM`/`ENAM`/`FNAM`/`GNAM`, and two `SOUN` FormIDs `HNAM`/`INAM`. Most slots mean different
//! things per type — xEdit labels them `(B)`attle / `(D)`ungeon / `(L)`ocation / `(I)`ncidental —
//! so they are kept positionally, and only the incidental reading has a typed view
//! ([`MediaSetRecord::incidental`]), because that is the shape REGN `RDSI` authors.
//!
//! Census of `FalloutNV.esm` (2026-10-28): 140 sets — 32 battle, 81 location, 23 dungeon,
//! 4 incidental. 10 of the 11 `RDSI` fields name one of those 4 incidental sets (the 11th,
//! `0x010276B2`, dangles); every incidental set's day/night `SOUN` is a folder-form `FNAM`
//! (`fx\mus\inc\day\` …).

use super::super::common::{read_zstring, remap_fid, CommonNamedFields};
use crate::esm::reader::{FormIdRemap, SubRecord};
use crate::esm::sub_reader::SubReader;

/// `NAM1` — xEdit's enum, with `-1` = "No Set".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MediaSetKind {
    Battle,
    Location,
    Dungeon,
    Incidental,
    #[default]
    NoSet,
    /// A value outside xEdit's enum, kept rather than folded into `NoSet`.
    Other(u32),
}

impl MediaSetKind {
    fn from_wire(v: u32) -> Self {
        match v {
            0 => Self::Battle,
            1 => Self::Location,
            2 => Self::Dungeon,
            3 => Self::Incidental,
            u32::MAX => Self::NoSet,
            other => Self::Other(other),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MediaSetRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub kind: MediaSetKind,
    /// `NAM2`–`NAM7` — track paths; empty where not authored.
    pub tracks: [String; 6],
    /// `NAM8`, `NAM9`, `NAM0`, `ANAM`, `BNAM`, `CNAM` — the level of the matching `tracks` slot, in dB.
    pub track_decibels: [f32; 6],
    /// `JNAM`–`ONAM` — location-set layer boundary percentages.
    pub boundaries: [f32; 6],
    /// `PNAM` — location-set layer enable bits (day outer … night inner).
    pub enable_flags: u8,
    /// `DNAM`, `ENAM`, `FNAM`, `GNAM` — type-dependent timings; see [`MediaSetRecord::incidental`].
    pub timings: [f32; 4],
    /// `HNAM` — intro `SOUN` (battle/dungeon) or the daytime `SOUN` (incidental).
    pub sound_a: Option<u32>,
    /// `INAM` — outro `SOUN` (battle/dungeon) or the nighttime `SOUN` (incidental).
    pub sound_b: Option<u32>,
}

/// An incidental set read through xEdit's `(I)` labels. The timings are carried in the
/// record's own unit, which no available source names (#3301).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IncidentalMediaSet {
    pub day_sound: Option<u32>,
    pub night_sound: Option<u32>,
    pub day_min: f32,
    pub day_max: f32,
    pub night_min: f32,
    pub night_max: f32,
}

impl MediaSetRecord {
    /// The `(I)` reading of `DNAM`/`ENAM`/`FNAM`/`GNAM`/`HNAM`/`INAM`; `None` for any other type.
    pub fn incidental(&self) -> Option<IncidentalMediaSet> {
        (self.kind == MediaSetKind::Incidental).then(|| IncidentalMediaSet {
            day_sound: self.sound_a,
            night_sound: self.sound_b,
            day_min: self.timings[0],
            night_min: self.timings[1],
            day_max: self.timings[2],
            night_max: self.timings[3],
        })
    }
}

const TRACK_SUBS: [&[u8; 4]; 6] = [b"NAM2", b"NAM3", b"NAM4", b"NAM5", b"NAM6", b"NAM7"];
const DECIBEL_SUBS: [&[u8; 4]; 6] = [b"NAM8", b"NAM9", b"NAM0", b"ANAM", b"BNAM", b"CNAM"];
const BOUNDARY_SUBS: [&[u8; 4]; 6] = [b"JNAM", b"KNAM", b"LNAM", b"MNAM", b"NNAM", b"ONAM"];
const TIMING_SUBS: [&[u8; 4]; 4] = [b"DNAM", b"ENAM", b"FNAM", b"GNAM"];

fn slot(subs: &[&[u8; 4]], sub_type: &[u8; 4]) -> Option<usize> {
    subs.iter().position(|s| *s == sub_type)
}

pub fn parse_mset(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> MediaSetRecord {
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    let mut out = MediaSetRecord { form_id, editor_id: common.editor_id, ..Default::default() };
    for sub in subs {
        let t = &sub.sub_type;
        // A short scalar is skipped, not read as zero — the field keeps its default.
        let f32_field = || SubReader::new(&sub.data).f32().ok();
        let fid_field = || SubReader::new(&sub.data).u32().ok().map(|v| remap_fid(v, remap));
        if let Some(i) = slot(&TRACK_SUBS, t) {
            out.tracks[i] = read_zstring(&sub.data);
        } else if let Some(i) = slot(&DECIBEL_SUBS, t) {
            out.track_decibels[i] = f32_field().unwrap_or_default();
        } else if let Some(i) = slot(&BOUNDARY_SUBS, t) {
            out.boundaries[i] = f32_field().unwrap_or_default();
        } else if let Some(i) = slot(&TIMING_SUBS, t) {
            out.timings[i] = f32_field().unwrap_or_default();
        } else {
            match t {
                b"NAM1" => {
                    if let Ok(v) = SubReader::new(&sub.data).u32() {
                        out.kind = MediaSetKind::from_wire(v);
                    }
                }
                b"PNAM" => out.enable_flags = sub.data.first().copied().unwrap_or_default(),
                b"HNAM" => out.sound_a = fid_field().filter(|&v| v != 0),
                b"INAM" => out.sound_b = fid_field().filter(|&v| v != 0),
                _ => {}
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::records::test_support::sub;

    /// `musSetINCDesert` (`0x0014E860`) as authored in `FalloutNV.esm`.
    #[test]
    fn an_incidental_set_reads_its_day_and_night_sounds_and_waits() {
        let subs = vec![
            sub(b"EDID", b"musSetINCDesert\0"),
            sub(b"NAM1", 3u32.to_le_bytes()),
            sub(b"DNAM", 4.0f32.to_le_bytes()),
            sub(b"ENAM", 6.0f32.to_le_bytes()),
            sub(b"FNAM", 16.0f32.to_le_bytes()),
            sub(b"GNAM", 12.0f32.to_le_bytes()),
            sub(b"HNAM", 0x0014_E85Du32.to_le_bytes()),
            sub(b"INAM", 0x0014_E85Eu32.to_le_bytes()),
        ];
        let set = parse_mset(0x0014_E860, &subs, &None);
        assert_eq!(set.editor_id, "musSetINCDesert");
        assert_eq!(set.kind, MediaSetKind::Incidental);
        assert_eq!(
            set.incidental(),
            Some(IncidentalMediaSet {
                day_sound: Some(0x0014_E85D),
                night_sound: Some(0x0014_E85E),
                day_min: 4.0,
                day_max: 16.0,
                night_min: 6.0,
                night_max: 12.0,
            })
        );
    }

    #[test]
    fn a_location_set_keeps_its_slots_positionally_and_has_no_incidental_view() {
        let subs = vec![
            sub(b"NAM1", 1u32.to_le_bytes()),
            sub(b"NAM2", b"loc\\a\\day_outer.mp3\0"),
            sub(b"NAM7", b"loc\\a\\night_inner.mp3\0"),
            sub(b"CNAM", (-6.0f32).to_le_bytes()),
            sub(b"ONAM", 25.0f32.to_le_bytes()),
            sub(b"PNAM", [0b0010_0001]),
        ];
        let set = parse_mset(1, &subs, &None);
        assert_eq!(set.kind, MediaSetKind::Location);
        assert_eq!(set.tracks[0], "loc\\a\\day_outer.mp3");
        assert_eq!(set.tracks[5], "loc\\a\\night_inner.mp3");
        assert_eq!(set.track_decibels[5], -6.0);
        assert_eq!(set.boundaries[5], 25.0);
        assert_eq!(set.enable_flags, 0b0010_0001);
        assert_eq!(set.incidental(), None);
    }

    #[test]
    fn no_set_unknown_types_and_truncated_scalars_decode_without_fabrication() {
        assert_eq!(parse_mset(1, &[sub(b"NAM1", u32::MAX.to_le_bytes())], &None).kind, MediaSetKind::NoSet);
        assert_eq!(parse_mset(1, &[sub(b"NAM1", 9u32.to_le_bytes())], &None).kind, MediaSetKind::Other(9));
        let truncated = parse_mset(1, &[sub(b"DNAM", [0u8, 0, 0x80]), sub(b"HNAM", [1u8, 2])], &None);
        assert_eq!(truncated.timings[0], 0.0);
        assert_eq!(truncated.sound_a, None);
        assert_eq!(parse_mset(1, &[sub(b"HNAM", 0u32.to_le_bytes())], &None).sound_a, None, "a null FormID is not a sound");
    }
}
