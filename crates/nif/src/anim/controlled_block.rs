//! Controlled-block string + target resolution.
//!
//! `CbString` enum + helpers for resolving node names / property kinds /
//! blend targets across version-dependent string layouts.

use crate::blocks::controller::ControlledBlock;
use crate::blocks::extra_data::{AnimNoteType, BsAnimNote};
use crate::blocks::interpolator::{
    NiBlendBoolInterpolator, NiBlendFloatInterpolator, NiBlendInterpolator,
    NiBlendPoint3Interpolator, NiBlendTransformInterpolator,
};
use crate::blocks::properties::NiStringPalette;
use crate::scene::NifScene;
use std::sync::Arc;

pub enum CbString {
    NodeName,
    ControllerType,
}

/// Resolve a `ControlledBlock` string field across both on-disk layouts.
///
/// NIFs from Oblivion / pre-FNV Bethesda titles (`10.2.0.0 ≤ v < 20.1.0.1`)
/// store the five per-block strings as byte offsets into a sibling
/// `NiStringPalette`; newer files inline them via the header's string
/// table and the parser pre-resolves them into `cb.node_name` et al.
/// Before #402 the importer only checked the string-table fields, so
/// every Oblivion `ControlledBlock` short-circuited at the `node_name`
/// guard and `import_kf` returned zero clips on all 1843 Oblivion KF
/// files. Falling through to the palette lookup fixes the whole range
/// of pre-Skyrim animations (Oblivion / Morrowind BBBB-era content)
/// without changing modern-path semantics.
pub fn resolve_cb_string(scene: &NifScene, cb: &ControlledBlock, which: CbString) -> Option<Arc<str>> {
    let (inline, offset) = match which {
        CbString::NodeName => (cb.node_name.as_ref(), cb.node_name_offset),
        CbString::ControllerType => (cb.controller_type.as_ref(), cb.controller_type_offset),
    };
    if let Some(s) = inline {
        return Some(Arc::clone(s));
    }
    let pal_idx = cb.string_palette_ref.index()?;
    let palette = scene.get_as::<NiStringPalette>(pal_idx)?;
    let s = palette.get_string(offset)?;
    if s.is_empty() {
        return None;
    }
    Some(Arc::from(s))
}

/// Serialize a `BSAnimNote` into a label suitable for the `text_keys`
/// channel. Downstream consumers filter on the `animnote:` prefix to
/// pick up IK hints specifically and ignore gameplay text events. See
/// the `BSAnimNote` type for field semantics.
pub fn format_anim_note_label(note: &BsAnimNote) -> String {
    match note.kind {
        AnimNoteType::GrabIk => {
            format!("animnote:grabik:arm={}", note.arm.unwrap_or(0))
        }
        AnimNoteType::LookIk => {
            format!(
                "animnote:lookik:gain={};state={}",
                note.gain.unwrap_or(0.0),
                note.state.unwrap_or(0)
            )
        }
        AnimNoteType::Invalid => "animnote:invalid".to_string(),
        AnimNoteType::Unknown(raw) => format!("animnote:unknown={raw}"),
    }
}

/// Follow a `NiBlend*Interpolator` indirection to its dominant sub-
/// interpolator. Returns the picked sub-interpolator's block index, or
/// `None` when `interp_idx` is not a blend variant or has no usable
/// weighted items (e.g. the common "manager-controlled" case where the
/// manager supplies the sub-interpolator externally via sibling
/// sequences — those are driven through their own `ControlledBlock`s
/// and this extractor has nothing to pull off the blend block itself).
///
/// "Dominant" = the item with the highest `normalized_weight` that has
/// a non-null interpolator_ref. This is a single-layer resolution —
/// the AnimationStack performs layer-based blending at the ECS level,
/// so picking one representative interpolator here gets the data
/// through the bottleneck without faking a runtime blend at import
/// time. See #334 (AR-08).
pub fn resolve_blend_interpolator_target(scene: &NifScene, interp_idx: usize) -> Option<usize> {
    let base: &NiBlendInterpolator =
        if let Some(b) = scene.get_as::<NiBlendTransformInterpolator>(interp_idx) {
            &b.base
        } else if let Some(b) = scene.get_as::<NiBlendFloatInterpolator>(interp_idx) {
            &b.base
        } else if let Some(b) = scene.get_as::<NiBlendPoint3Interpolator>(interp_idx) {
            &b.base
        } else if let Some(b) = scene.get_as::<NiBlendBoolInterpolator>(interp_idx) {
            &b.base
        } else {
            return None;
        };

    // Manager-controlled blends carry an empty `items` array — the
    // NiControllerManager drives the sub-interpolators externally via
    // sibling ControlledBlocks. Fall through to None so the caller
    // logs nothing; those sequences import cleanly through their own
    // interpolator_refs.
    base.items
        .iter()
        .filter_map(|it| {
            it.interpolator_ref
                .index()
                .map(|i| (i, it.normalized_weight))
        })
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(idx, _)| idx)
}
