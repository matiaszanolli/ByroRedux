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
use super::misc::{parse_dial, parse_info};
use super::DialRecord;
use anyhow::Result;
use std::collections::HashMap;

pub(super) fn extract_records_with_modl(
    reader: &mut EsmReader,
    end: usize,
    expected_type: &[u8; 4],
    statics: &mut HashMap<u32, StaticObject>,
    f: &mut dyn FnMut(u32, &[SubRecord]),
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub_group = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub_group);
            extract_records_with_modl(reader, sub_end, expected_type, statics, f)?;
            continue;
        }
        let header = reader.read_record_header()?;
        if &header.record_type == expected_type {
            let subs = reader.read_sub_records(&header)?;
            // Cell-side: build the StaticObject from the same subs.
            if let Some(stat) =
                build_static_object_from_subs(header.form_id, &header.record_type, &subs)
            {
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
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub_group = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub_group);
            extract_records(reader, sub_end, expected_type, f)?;
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
    /// Topic Children group_type from the ESM format (TES4 / FO3 /
    /// FNV / Skyrim / FO4 all share the value).
    const GROUP_TYPE_TOPIC_CHILDREN: u32 = 7;

    let mut last_dial_form_id: Option<u32> = None;

    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub_group = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub_group);

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
                walk_info_records(reader, sub_end, target, dialogues)?;
                continue;
            }

            // Any other nested group inside the DIAL tree (rare —
            // shouldn't happen in vanilla content): recurse with the
            // same handler so a stray DIAL or another Topic Children
            // tier still gets walked. Bytes accounting stays sound.
            extract_dial_with_info(reader, sub_end, dialogues)?;
            continue;
        }

        let header = reader.read_record_header()?;
        if &header.record_type == b"DIAL" {
            let subs = reader.read_sub_records(&header)?;
            let dial = parse_dial(header.form_id, &subs);
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
            let info = parse_info(header.form_id, &subs);
            if let Some(dial) = dialogues.get_mut(&parent_dial_form_id) {
                dial.infos.push(info);
            }
        } else {
            reader.skip_record(&header);
        }
    }
    Ok(())
}
