//! `KYWD` keywords, `AORU` attraction rules and `CNDF` condition forms.


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
