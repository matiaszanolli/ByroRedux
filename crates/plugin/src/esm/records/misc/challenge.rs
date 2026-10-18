//! `CHAL` — a New Vegas challenge (the Pip-Boy's challenges screen).
//!
//! Layout per xEdit's FNV definitions (`wbRecord(CHAL ...)`): `EDID`, `FULL`, `ICON`, `SCRI` (the reward
//! script, "the script that runs upon completion of the challenge" — GECK *Challenge*), `DESC`, a 24-byte
//! `DATA` (type u32, threshold u32, flags u32, interval u32, then two u16 and a u32 whose meaning
//! depends on the type), and the type-dependent `SNAM` / `XNAM` FormIDs.

use super::super::common::{remap_fid, CommonNamedFields};
use crate::esm::reader::SubRecord;
use crate::esm::sub_reader::SubReader;

/// The challenge types (`DATA` type), in the GECK's order.
pub mod kind {
    pub const KILL_FROM_LIST: u32 = 0;
    pub const KILL_SPECIFIC: u32 = 1;
    pub const KILL_CATEGORY: u32 = 2;
    pub const HIT_ENEMY: u32 = 3;
    pub const DISCOVER_MARKER: u32 = 4;
    pub const USE_ITEM: u32 = 5;
    pub const ACQUIRE_ITEM: u32 = 6;
    pub const USE_SKILL: u32 = 7;
    pub const DO_DAMAGE: u32 = 8;
    pub const USE_ITEM_FROM_LIST: u32 = 9;
    pub const ACQUIRE_FROM_LIST: u32 = 10;
    pub const MISC_STAT: u32 = 11;
    pub const CRAFT_USING_ITEM: u32 = 12;
    pub const SCRIPTED: u32 = 13;
}

/// `DATA` flag: "the challenge will not be visible and will not progress until enabled by script".
pub const FLAG_START_DISABLED: u32 = 1;
/// `DATA` flag: "the challenge will start over upon completion".
pub const FLAG_RECURRING: u32 = 2;
/// `DATA` flag: "the challenge will appear in the Pip-Boy despite having no progress".
pub const FLAG_SHOW_ZERO_PROGRESS: u32 = 4;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChalRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub description: String,
    /// `SCRI` — the reward script.
    pub script_form_id: u32,
    /// `DATA` type, see [`kind`].
    pub challenge_type: u32,
    /// "The number of times an action must be performed for the challenge to complete."
    pub threshold: u32,
    pub flags: u32,
    /// "The challenge will show progress notifications at the interval specified here."
    pub interval: u32,
    /// The two type-dependent halves and the word after them (a misc stat index, an actor value, a
    /// weapon category ...).
    pub value1: u16,
    pub value2: u16,
    pub value3: u32,
    /// `SNAM` — the form the type names (the item, the marker, the form list, the kill target).
    pub form1: u32,
    /// `XNAM` — the second form the type names (a weapon list, a slot list).
    pub form2: u32,
}

impl ChalRecord {
    pub fn starts_disabled(&self) -> bool {
        self.flags & FLAG_START_DISABLED != 0
    }

    pub fn is_recurring(&self) -> bool {
        self.flags & FLAG_RECURRING != 0
    }
}

pub fn parse_chal(form_id: u32, subs: &[SubRecord], remap: &Option<crate::esm::reader::FormIdRemap>) -> ChalRecord {
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    let mut out = ChalRecord {
        form_id,
        editor_id: common.editor_id,
        full_name: common.full_name,
        script_form_id: common.script_form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"DESC" => out.description = super::super::common::read_lstring_or_zstring(&sub.data),
            b"DATA" if sub.data.len() >= 24 => {
                let mut r = SubReader::new(&sub.data);
                out.challenge_type = r.u32_or_default();
                out.threshold = r.u32_or_default();
                out.flags = r.u32_or_default();
                out.interval = r.u32_or_default();
                out.value1 = r.u16_or_default();
                out.value2 = r.u16_or_default();
                out.value3 = r.u32_or_default();
            }
            b"SNAM" if sub.data.len() >= 4 => out.form1 = remap_fid(u32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]), remap),
            b"XNAM" if sub.data.len() >= 4 => out.form2 = remap_fid(u32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]), remap),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(kind: &[u8; 4], data: Vec<u8>) -> SubRecord {
        SubRecord { sub_type: *kind, data }
    }

    /// `NVChallengeCaravanWin` as it sits in FalloutNV.esm: a misc-stat challenge (stat 37, Caravan Games
    /// Won) for 3 wins, notifying every 1.
    #[test]
    fn a_misc_stat_challenge_decodes_its_data() {
        let mut data = Vec::new();
        for word in [11u32, 3, 0, 1] {
            data.extend(word.to_le_bytes());
        }
        data.extend(37u16.to_le_bytes());
        data.extend(0u16.to_le_bytes());
        data.extend(0u32.to_le_bytes());
        let subs = vec![
            sub(b"EDID", b"NVChallengeCaravanWin\0".to_vec()),
            sub(b"SCRI", 0x0016_577fu32.to_le_bytes().to_vec()),
            sub(b"DATA", data),
            sub(b"SNAM", 0x0012_0d81u32.to_le_bytes().to_vec()),
        ];
        let chal = parse_chal(0x165777, &subs, &None);
        assert_eq!(
            (chal.editor_id.as_str(), chal.challenge_type, chal.threshold, chal.interval, chal.value1),
            ("NVChallengeCaravanWin", kind::MISC_STAT, 3, 1, 37)
        );
        assert_eq!((chal.script_form_id, chal.form1), (0x0016_577f, 0x0012_0d81));
        assert!(!chal.starts_disabled() && !chal.is_recurring());
    }

    #[test]
    fn the_flags_are_start_disabled_recurring_and_show_zero_progress() {
        let chal = ChalRecord { flags: FLAG_START_DISABLED | FLAG_RECURRING, ..Default::default() };
        assert!(chal.starts_disabled() && chal.is_recurring());
        assert_eq!(FLAG_SHOW_ZERO_PROGRESS, 4);
    }
}
