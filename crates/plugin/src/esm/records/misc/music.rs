//! `MUSC` — a Fallout 3 / New Vegas music type: one track file and its level.
//!
//! Layout per xEdit's FNV definitions: `EDID`, `FNAM` (the file name, relative to the `music\` folder)
//! and `ANAM` ("dB (positive = Loop)"). Other games' `MUSC` records are laid out differently (Skyrim's
//! carries a track list), so only the Fallout 3 / New Vegas shape is decoded.

use super::super::common::{read_zstring, CommonNamedFields};
use crate::esm::reader::{GameKind, SubRecord};
use crate::esm::sub_reader::SubReader;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MuscRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `FNAM` — the track, relative to the `music\` folder; empty on a game whose `MUSC` is not decoded.
    pub file: String,
    /// `ANAM` — the level in dB; a positive value marks a looping track.
    pub decibels: f32,
}

impl MuscRecord {
    pub fn looping(&self) -> bool {
        self.decibels > 0.0
    }
}

pub fn parse_musc(form_id: u32, subs: &[SubRecord], game: GameKind) -> MuscRecord {
    let common = CommonNamedFields::from_subs_with_remap(subs, &None);
    let mut out = MuscRecord { form_id, editor_id: common.editor_id, ..Default::default() };
    if game != GameKind::Fallout3NV {
        return out;
    }
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
    out
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
}
