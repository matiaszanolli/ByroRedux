//! `KYWD` keywords, `AORU` attraction rules, `CNDF` condition forms and `EQUP` equip slots.


use super::super::super::common::CommonNamedFields;
use super::super::super::condition::{push_ctda, ConditionList};
use crate::esm::reader::{FormIdRemap, SubRecord};
use crate::esm::sub_reader::SubReader;

/// Starfield condition form (`CNDF`): a named, reusable condition list that
/// Papyrus evaluates with `ConditionForm.IsTrue(akRefObject, akTargetObject)`
/// and records reference by FormID (xEdit `wbRecord(CNDF)`: `EDID` then the
/// `CTDA` / `CIS*` list).
#[derive(Debug, Clone, Default)]
pub struct CndfRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub conditions: ConditionList,
}

pub fn parse_cndf(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> CndfRecord {
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    let mut out = CndfRecord {
        form_id,
        editor_id: common.editor_id,
        conditions: ConditionList::new(),
    };
    for sub in subs {
        if matches!(&sub.sub_type, b"CTDA" | b"CTDT" | b"CIS1" | b"CIS2") {
            push_ctda(sub, remap, &mut out.conditions);
        }
    }
    out
}

/// Keyword (`KYWD`): the form Papyrus keyword properties and `KWDA` lists
/// point at. Only the editor id is read — keywords carry no data of their own,
/// and the name is what tells the animation-archetype keywords
/// (`AnimArchetypeConfident`, …) from the rest.
#[derive(Debug, Clone, Default)]
pub struct KywdRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// Fallout 4 `DATA` — the `AORU` attraction rule this keyword carries ("Attraction Type"), or `0`.
    pub attraction_rule: u32,
}

/// Fallout 4 `AORU` attraction rule (xEdit `wbAORU`, `AOR2`: radius, min delay, max delay, requires line of
/// sight, combat target). CK *AttractionRule*: "a game system that causes Actors to react to specific events
/// that occur around them", specified on a keyword's *Attraction Type*.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AttractionRuleRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub radius: f32,
    /// Seconds; an actor reacts to the same object no sooner than the minimum and no later than the maximum.
    pub min_delay: f32,
    pub max_delay: f32,
    pub requires_line_of_sight: bool,
    pub combat_target: bool,
}

/// `AOR2` through the combat-target byte: three f32s and two bool bytes (two unused bytes follow, xEdit
/// `wbAORU`).
const AOR2_LEN: usize = 14;

pub fn parse_aoru(form_id: u32, subs: &[SubRecord]) -> AttractionRuleRecord {
    let common = CommonNamedFields::from_subs_with_remap(subs, &None);
    let mut out = AttractionRuleRecord { form_id, editor_id: common.editor_id, ..Default::default() };
    if let Some(sub) = subs.iter().find(|sub| &sub.sub_type == b"AOR2" && sub.data.len() >= AOR2_LEN) {
        let mut r = SubReader::new(&sub.data);
        out.radius = r.f32_or_default();
        out.min_delay = r.f32_or_default();
        out.max_delay = r.f32_or_default();
        out.requires_line_of_sight = r.u8_or_default() != 0;
        out.combat_target = r.u8_or_default() != 0;
    }
    out
}

pub fn parse_kywd(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> KywdRecord {
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    // Fallout 4's `DATA` is the attraction rule (xEdit `wbKYWD`); the other games author none.
    let attraction_rule = subs
        .iter()
        .find(|sub| &sub.sub_type == b"DATA" && sub.data.len() >= 4)
        .map(|sub| super::super::super::common::remap_fid(u32::from_le_bytes(sub.data[..4].try_into().unwrap()), remap))
        .unwrap_or(0);
    KywdRecord {
        form_id,
        editor_id: common.editor_id,
        attraction_rule,
    }
}

/// Equip slot (`EQUP`, Skyrim and later): a body position an item equips to (UESP *Skyrim Mod:Mod File
/// Format/EQUP*): its parent slots (`PNAM`, FormIDs) and whether it takes all of them (`DATA`). The CK
/// *Equip Slot*: `BothHands` has the parents `LeftHand` and `RightHand` and takes both; `EitherHand` has
/// the same parents and takes one. A slot with no parents is a leaf (Skyrim.esm: `LeftHand`,
/// `RightHand`, `Voice`, `Potion`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EqupRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub parents: Vec<u32>,
    pub use_all_parents: bool,
}

pub fn parse_equp(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> EqupRecord {
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    let mut out = EqupRecord { form_id, editor_id: common.editor_id, ..Default::default() };
    for sub in subs {
        match &sub.sub_type {
            b"PNAM" => {
                let mut r = SubReader::new(&sub.data);
                while let Ok(parent) = r.u32() {
                    out.parents.push(crate::esm::records::common::remap_fid(parent, remap));
                }
            }
            b"DATA" => out.use_all_parents = sub.data.first().is_some_and(|&flag| flag != 0),
            _ => {}
        }
    }
    out
}

/// The hands an equip slot takes (Hand Equipment PRD, HND-SLOT-002).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotHands {
    /// No hand (`Voice`, `Potion`, an unknown slot).
    None,
    /// The left hand only (`LeftHand`, and `Shield`, whose one parent it is).
    Left,
    /// The right hand only (`RightHand`).
    Right,
    /// One of the two (`EitherHand`): a slot naming both hands without taking all its parents.
    Either,
    /// Both (`BothHands`): a slot naming both hands and taking all its parents.
    Both,
}

/// The hands `slot` takes, from the equip-slot records: the leaves it reaches are the hands (a leaf
/// named `LeftHand` or `RightHand`, as Skyrim.esm's are); naming both, it takes both when it uses all
/// its parents and one of them when it does not (CK *Equip Slot*).
pub fn slot_hands(slots: &std::collections::HashMap<u32, EqupRecord>, slot: u32) -> SlotHands {
    fn leaves(slots: &std::collections::HashMap<u32, EqupRecord>, slot: u32, depth: usize, left: &mut bool, right: &mut bool) {
        let Some(record) = slots.get(&slot).filter(|_| depth < 8) else { return };
        if record.parents.is_empty() {
            *left |= record.editor_id.eq_ignore_ascii_case("LeftHand");
            *right |= record.editor_id.eq_ignore_ascii_case("RightHand");
            return;
        }
        for &parent in &record.parents {
            leaves(slots, parent, depth + 1, left, right);
        }
    }
    let (mut left, mut right) = (false, false);
    leaves(slots, slot, 0, &mut left, &mut right);
    match (left, right) {
        (true, true) if slots.get(&slot).is_some_and(|record| record.use_all_parents) => SlotHands::Both,
        (true, true) => SlotHands::Either,
        (true, false) => SlotHands::Left,
        (false, true) => SlotHands::Right,
        (false, false) => SlotHands::None,
    }
}

#[cfg(test)]
mod equip_slot_tests {
    use super::*;

    /// Skyrim.esm's hand slots (FormIDs and editor ids from the master), as the CK *Equip Slot* page
    /// describes them.
    #[test]
    fn the_skyrim_hand_slots_take_the_hands_the_ck_describes() {
        let slot = |form_id: u32, editor_id: &str, parents: Vec<u32>, use_all_parents: bool| {
            (form_id, EqupRecord { form_id, editor_id: editor_id.into(), parents, use_all_parents })
        };
        let slots: std::collections::HashMap<u32, EqupRecord> = [
            slot(0x13F42, "RightHand", vec![], false),
            slot(0x13F43, "LeftHand", vec![], false),
            slot(0x13F44, "EitherHand", vec![0x13F43, 0x13F42], false),
            slot(0x13F45, "BothHands", vec![0x13F43, 0x13F42], true),
            slot(0x141E8, "Shield", vec![0x13F43], true),
            slot(0x25BEE, "Voice", vec![], false),
        ]
        .into();
        assert_eq!(slot_hands(&slots, 0x13F42), SlotHands::Right);
        assert_eq!(slot_hands(&slots, 0x13F43), SlotHands::Left);
        assert_eq!(slot_hands(&slots, 0x13F44), SlotHands::Either);
        assert_eq!(slot_hands(&slots, 0x13F45), SlotHands::Both);
        assert_eq!(slot_hands(&slots, 0x141E8), SlotHands::Left);
        assert_eq!(slot_hands(&slots, 0x25BEE), SlotHands::None);
        assert_eq!(slot_hands(&slots, 0xDEAD), SlotHands::None);
    }

    #[test]
    fn an_equp_record_reads_its_parents_and_flag() {
        let sub = |tag: &[u8; 4], data: Vec<u8>| SubRecord { sub_type: *tag, data };
        let mut pnam = 0x13F43u32.to_le_bytes().to_vec();
        pnam.extend(0x13F42u32.to_le_bytes());
        let record = parse_equp(0x13F45, &[sub(b"EDID", b"BothHands\0".to_vec()), sub(b"PNAM", pnam), sub(b"DATA", vec![1, 0, 0, 0])], &None);
        assert_eq!(record.editor_id, "BothHands");
        assert_eq!(record.parents, [0x13F43, 0x13F42]);
        assert!(record.use_all_parents);
    }
}
