//! GRUP-tree walkers shared by the top-level record dispatch in
//! `plugin_loader::parse_esm_with_load_order`.
//!
//! Lifted out of the pre-#1118 monolithic `records/mod.rs` (TD9-003).
//! The walker signatures and bodies are byte-identical; only their
//! module location changed. Visibility is `pub(super)` so the parent
//! `records::mod` can dispatch into them without leaking these
//! internal walkers outside the crate.

use super::super::cell::{build_static_object_from_subs, StaticObject};
use super::super::reader::{EsmReader, SubRecord};
use super::misc::{parse_dial, parse_info, parse_qust, parse_scen};
use super::{DialRecord, QustRecord, ScenRecord};
use anyhow::Result;
use std::collections::HashMap;

pub(super) fn extract_records_with_modl(
    reader: &mut EsmReader,
    end: usize,
    expected_type: &[u8; 4],
    statics: &mut HashMap<u32, StaticObject>,
    f: &mut dyn FnMut(u32, &[SubRecord]),
) -> Result<()> {
    extract_records_with_modl_inner(reader, end, expected_type, statics, f, 0)
}

fn extract_records_with_modl_inner(
    reader: &mut EsmReader,
    end: usize,
    expected_type: &[u8; 4],
    statics: &mut HashMap<u32, StaticObject>,
    f: &mut dyn FnMut(u32, &[SubRecord]),
    depth: u32,
) -> Result<()> {
    let remap = reader.get_form_id_remap();
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub_group = reader.read_group_header()?;
            let Some(sub_end) =
                reader.bounded_group_content_end(&sub_group, depth, "extract_records_with_modl")
            else {
                continue;
            };
            extract_records_with_modl_inner(reader, sub_end, expected_type, statics, f, depth + 1)?;
            continue;
        }
        let header = reader.read_record_header()?;
        if &header.record_type == expected_type {
            let subs = reader.read_sub_records(&header)?;
            // Cell-side: build the StaticObject from the same subs.
            if let Some(stat) = build_static_object_from_subs(
                header.form_id,
                &header.record_type,
                header.is_visible_when_distant(),
                &subs,
                &remap,
            ) {
                statics.insert(header.form_id, stat);
            }
            // Records-side: typed parser.
            f(header.form_id, &subs);
        } else {
            reader.skip_record(&header);
        }
    }
    Ok(())
}

/// Walk a top-level group and call `f(form_id, subs)` for every record
/// matching `expected_type`. Recurses into nested groups so worldspace
/// children and persistent/temporary cell children are handled too.
///
/// `f` takes a closure rather than returning a parsed value so the caller
/// can route the record into a type-specific HashMap without an extra
/// boxing/erasure layer.
pub(super) fn extract_records(
    reader: &mut EsmReader,
    end: usize,
    expected_type: &[u8; 4],
    f: &mut dyn FnMut(u32, &[SubRecord]),
) -> Result<()> {
    extract_records_inner(reader, end, expected_type, f, 0)
}

fn extract_records_inner(
    reader: &mut EsmReader,
    end: usize,
    expected_type: &[u8; 4],
    f: &mut dyn FnMut(u32, &[SubRecord]),
    depth: u32,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub_group = reader.read_group_header()?;
            let Some(sub_end) =
                reader.bounded_group_content_end(&sub_group, depth, "extract_records")
            else {
                continue;
            };
            extract_records_inner(reader, sub_end, expected_type, f, depth + 1)?;
            continue;
        }
        let header = reader.read_record_header()?;
        if &header.record_type == expected_type {
            let subs = reader.read_sub_records(&header)?;
            f(header.form_id, &subs);
        } else {
            reader.skip_record(&header);
        }
    }
    Ok(())
}

/// Walk a top-level DIAL group, parsing each DIAL record and its
/// child INFO sub-group (group_type == 7 Topic Children). Each
/// sub-GRUP's `label` field carries the parent DIAL's form_id u32 —
/// the walker matches it against the most recent DIAL it parsed and
/// pushes decoded INFOs onto `DialRecord.infos`.
///
/// Layout:
/// ```text
/// GRUP type=0 label="DIAL"  (top-level — caller already entered)
///   DIAL record (form_id=A)
///   GRUP type=7 label=A     (Topic Children for DIAL A)
///     INFO record
///     INFO record
///     ...
///   DIAL record (form_id=B)
///   GRUP type=7 label=B
///     INFO record
///   ...
/// ```
///
/// Pre-#631 the generic `extract_records` walker ignored INFO bytes
/// because it filtered on `expected_type == "DIAL"`. Dedicated walker
/// stays SSE-correct and avoids parameterising the generic walker
/// with a multi-type closure map (the only record with this shape
/// today). See audit `AUDIT_FNV_2026-04-24.md` D2-03.
pub(super) fn extract_dial_with_info(
    reader: &mut EsmReader,
    end: usize,
    dialogues: &mut HashMap<u32, DialRecord>,
) -> Result<()> {
    extract_dial_with_info_inner(reader, end, dialogues, 0)
}

fn extract_dial_with_info_inner(
    reader: &mut EsmReader,
    end: usize,
    dialogues: &mut HashMap<u32, DialRecord>,
    depth: u32,
) -> Result<()> {
    /// Topic Children group_type from the ESM format (TES4 / FO3 /
    /// FNV / Skyrim / FO4 all share the value).
    const GROUP_TYPE_TOPIC_CHILDREN: u32 = 7;

    let remap = reader.get_form_id_remap();
    let mut last_dial_form_id: Option<u32> = None;

    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub_group = reader.read_group_header()?;
            let Some(sub_end) =
                reader.bounded_group_content_end(&sub_group, depth, "extract_dial_with_info")
            else {
                continue;
            };

            if sub_group.group_type == GROUP_TYPE_TOPIC_CHILDREN {
                // Sub-group label is the parent DIAL's form_id u32.
                let parent_form_id = u32::from_le_bytes(sub_group.label);
                // Tolerate sub-group / last-DIAL label drift —
                // shipped content has been observed with off-by-one
                // dispositions across patches. We accept the most-
                // recent DIAL as parent when the labels disagree, and
                // log at debug; mismatch is rare enough to warrant
                // visibility but never bytes-throwing.
                let target = last_dial_form_id.unwrap_or(parent_form_id);
                if Some(parent_form_id) != last_dial_form_id {
                    log::debug!(
                        "DIAL Topic Children sub-group label {:#x} doesn't match \
                         most-recent DIAL form_id {:?}; routing INFOs to \
                         most-recent DIAL — see #631",
                        parent_form_id,
                        last_dial_form_id,
                    );
                }
                walk_info_records(reader, sub_end, target, dialogues, &remap)?;
                continue;
            }

            // Any other nested group inside the DIAL tree (rare —
            // shouldn't happen in vanilla content): recurse with the
            // same handler so a stray DIAL or another Topic Children
            // tier still gets walked. Bytes accounting stays sound.
            extract_dial_with_info_inner(reader, sub_end, dialogues, depth + 1)?;
            continue;
        }

        let header = reader.read_record_header()?;
        if &header.record_type == b"DIAL" {
            let subs = reader.read_sub_records(&header)?;
            let dial = parse_dial(header.form_id, &subs, &remap);
            dialogues.insert(header.form_id, dial);
            last_dial_form_id = Some(header.form_id);
        } else {
            // Non-DIAL record at this tier — skip and keep walking.
            reader.skip_record(&header);
        }
    }
    Ok(())
}

/// Inner helper for `extract_dial_with_info` — walks a Topic Children
/// sub-GRUP, decoding each INFO record onto the parent DIAL's
/// `infos` vec. Skips non-INFO records (defensive — shipped content
/// may include nested QSTR / NAVI tiers in some patches).
fn walk_info_records(
    reader: &mut EsmReader,
    end: usize,
    parent_dial_form_id: u32,
    dialogues: &mut HashMap<u32, DialRecord>,
    remap: &Option<crate::esm::reader::FormIdRemap>,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            // Nested group inside a Topic Children sub-GRUP —
            // unusual but tolerated. Skip wholesale rather than
            // recursing further; the runtime consumer doesn't need
            // the deeper tiers today.
            let inner = reader.read_group_header()?;
            reader.skip_group(&inner);
            continue;
        }
        let header = reader.read_record_header()?;
        if &header.record_type == b"INFO" {
            let subs = reader.read_sub_records(&header)?;
            let info = parse_info(header.form_id, &subs, remap);
            if let Some(dial) = dialogues.get_mut(&parent_dial_form_id) {
                dial.infos.push(info);
            }
        } else {
            reader.skip_record(&header);
        }
    }
    Ok(())
}

/// Walk the top-level `QUST` group, routing `QUST` / `DIAL` / `INFO` /
/// `SCEN` records into their typed maps regardless of nesting depth.
///
/// #2908 / ESM-D4-02 — Oblivion→Skyrim ship `DIAL` and `SCEN` as their
/// OWN top-level GRUPs, so the generic single-type [`extract_records`]
/// worked for them. FO4 nests the entire dialogue/scene tree as
/// children of the `QUST` GRUP instead (FO4 ships no top-level
/// `DIAL`/`SCEN` label at all; Starfield ships both, but empty — the
/// real content is under `QUST` there too), and `extract_records`
/// filtering on a single `expected_type` silently `skip_record`s
/// every non-`QUST` record it finds — 117,230 / 202,193 records on
/// FO4 / Starfield respectively.
///
/// The exact nesting shape under `QUST` (which `group_type` wraps a
/// quest's `DIAL`/`SCEN` children, and how deep `INFO` sits under its
/// parent `DIAL`) isn't independently confirmed against a spec here,
/// so — like [`extract_dial_with_info`] already does for its own
/// group_type-drift case — this walker doesn't gate on a specific
/// `group_type` at all: it recurses into every nested group
/// unconditionally (same posture as [`extract_records`]) and tracks
/// "the most recently parsed `DIAL`" as plain walk-order state,
/// threaded through the recursion via `last_dial_form_id` so an
/// `INFO` attaches correctly no matter how many group levels separate
/// it from its parent `DIAL`.
///
/// `DLBR` (Dialog Branch — also present in the audit's evidence table)
/// is deliberately NOT parsed here: no byte layout for it exists
/// anywhere in this tree or the project's reference sources, and
/// guessing one would violate the project's no-guessing policy. It
/// falls through to the same `skip_record` every other unhandled type
/// at this tier already gets — a stated gap, not a silent one.
pub(super) fn extract_quest_dialogue_scene_tree(
    reader: &mut EsmReader,
    end: usize,
    quests: &mut HashMap<u32, QustRecord>,
    dialogues: &mut HashMap<u32, DialRecord>,
    scenes: &mut HashMap<u32, ScenRecord>,
) -> Result<()> {
    let remap = reader.get_form_id_remap();
    let mut last_dial_form_id: Option<u32> = None;
    extract_quest_dialogue_scene_tree_inner(
        reader,
        end,
        quests,
        dialogues,
        scenes,
        &remap,
        &mut last_dial_form_id,
        0,
    )
}

fn extract_quest_dialogue_scene_tree_inner(
    reader: &mut EsmReader,
    end: usize,
    quests: &mut HashMap<u32, QustRecord>,
    dialogues: &mut HashMap<u32, DialRecord>,
    scenes: &mut HashMap<u32, ScenRecord>,
    remap: &Option<crate::esm::reader::FormIdRemap>,
    last_dial_form_id: &mut Option<u32>,
    depth: u32,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub_group = reader.read_group_header()?;
            let Some(sub_end) = reader.bounded_group_content_end(
                &sub_group,
                depth,
                "extract_quest_dialogue_scene_tree_inner",
            ) else {
                continue;
            };
            extract_quest_dialogue_scene_tree_inner(
                reader,
                sub_end,
                quests,
                dialogues,
                scenes,
                remap,
                last_dial_form_id,
                depth + 1,
            )?;
            continue;
        }
        let header = reader.read_record_header()?;
        match &header.record_type {
            b"QUST" => {
                let subs = reader.read_sub_records(&header)?;
                quests.insert(header.form_id, parse_qust(header.form_id, &subs, remap));
            }
            b"DIAL" => {
                let subs = reader.read_sub_records(&header)?;
                dialogues.insert(header.form_id, parse_dial(header.form_id, &subs, remap));
                *last_dial_form_id = Some(header.form_id);
            }
            b"INFO" => {
                let subs = reader.read_sub_records(&header)?;
                let info = parse_info(header.form_id, &subs, remap);
                if let Some(parent) = last_dial_form_id.and_then(|fid| dialogues.get_mut(&fid)) {
                    parent.infos.push(info);
                }
                // No `last_dial_form_id` yet (malformed/reordered
                // content) — the INFO has nowhere to attach. Same
                // silent-drop posture `walk_info_records` already has
                // for its unresolvable-parent case.
            }
            b"SCEN" => {
                let subs = reader.read_sub_records(&header)?;
                scenes.insert(header.form_id, parse_scen(header.form_id, &subs, remap));
            }
            _ => reader.skip_record(&header),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::reader::MAX_GRUP_NESTING_DEPTH;

    fn tes5_record(record_type: &[u8; 4], form_id: u32) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(24);
        bytes.extend_from_slice(record_type);
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&form_id.to_le_bytes());
        bytes.extend_from_slice(&[0; 8]);
        bytes
    }

    fn tes5_group(payload: Vec<u8>) -> Vec<u8> {
        let total_size = 24u32 + u32::try_from(payload.len()).expect("synthetic group fits u32");
        let mut bytes = Vec::with_capacity(total_size as usize);
        bytes.extend_from_slice(b"GRUP");
        bytes.extend_from_slice(&total_size.to_le_bytes());
        bytes.extend_from_slice(b"TEST");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&[0; 8]);
        bytes.extend_from_slice(&payload);
        bytes
    }

    #[test]
    fn deeply_nested_grup_is_skipped_at_shared_limit() {
        let mut bytes = tes5_record(b"GMST", 0x1234);
        for _ in 0..(MAX_GRUP_NESTING_DEPTH + 128) {
            bytes = tes5_group(bytes);
        }

        let mut reader = EsmReader::new(&bytes);
        let mut seen = Vec::new();
        extract_records(&mut reader, bytes.len(), b"GMST", &mut |form_id, _| {
            seen.push(form_id)
        })
        .expect("over-depth GRUP input must be skipped without aborting the parse");

        assert!(
            seen.is_empty(),
            "records below the depth cap must be skipped"
        );
        assert_eq!(
            reader.position(),
            bytes.len(),
            "skipping the over-depth group must preserve outer byte accounting"
        );
    }
}
