//! `NiControllerSequence` import and `ControlledBlock` string resolution
//! (`anim/sequence.rs`, `anim/controlled_block.rs`).

use super::super::*;

use crate::blocks::interpolator::NiLookAtInterpolator;
use crate::scene::NifScene;
use std::sync::Arc;

use super::dummy_controlled_block;

/// Regression: #402. Oblivion-era `NiControllerSequence` blocks
/// reference their node/controller strings through an
/// `NiStringPalette` + byte offsets rather than the modern header
/// string table. Before the fix, `resolve_cb_string` returned None
/// for palette-backed ControlledBlocks → every `cb.node_name` guard
/// in `import_sequence` short-circuited → zero clips imported on
/// every Oblivion KF. This test builds a minimal scene with a
/// palette-backed transform ControlledBlock and asserts the
/// resolver returns the expected string.
#[test]
fn resolve_cb_string_reads_oblivion_palette() {
    use crate::blocks::properties::NiStringPalette;
    use crate::types::BlockRef;

    let palette = NiStringPalette {
        palette: "Bip01\0NiTransformController\0".to_string(),
    };
    let scene = NifScene {
        blocks: vec![Box::new(palette)],
        ..NifScene::default()
    };
    let mut cb = dummy_controlled_block();
    cb.string_palette_ref = BlockRef(0);
    cb.node_name_offset = 0;
    cb.controller_type_offset = 6;

    let node = resolve_cb_string(&scene, &cb, CbString::NodeName)
        .expect("palette-backed node_name must resolve");
    assert_eq!(&*node, "Bip01");
    let ctrl = resolve_cb_string(&scene, &cb, CbString::ControllerType)
        .expect("palette-backed controller_type must resolve");
    assert_eq!(&*ctrl, "NiTransformController");
}

/// #402 sibling: modern string-table-backed ControlledBlocks (Skyrim+
/// and FNV) still resolve through the inline `Option<Arc<str>>`
/// path. This makes sure the palette fallback doesn't shadow the
/// fast path.
#[test]
fn resolve_cb_string_prefers_inline_when_present() {
    let scene = NifScene::default();
    let mut cb = dummy_controlled_block();
    cb.node_name = Some(Arc::from("Bip01 Head"));
    // Palette offset would point at a completely different string,
    // but the inline field takes precedence.
    cb.node_name_offset = 42;

    let node = resolve_cb_string(&scene, &cb, CbString::NodeName)
        .expect("inline name must win over palette fallback");
    assert_eq!(&*node, "Bip01 Head");
}

/// Regression: LC-D5-03 / #1442. The KF-sequence dispatch must route a
/// controlled block whose controller type resolves to the classic
/// "NiKeyframeController" alias through transform extraction, exactly
/// like "NiTransformController" — not drop it to the `_ =>` arm. The
/// block parser (blocks/mod.rs) and the embedded-animation path
/// (entry.rs) already alias both names; this pins the sequence path to
/// the same behavior.
#[test]
fn import_sequence_dispatches_keyframe_controller_alias() {
    use crate::blocks::controller::NiControllerSequence;
    use crate::types::{BlockRef, NiPoint3, NiQuatTransform};

    // A NiLookAtInterpolator yields a constant-pose transform channel,
    // so any block that reaches extract_transform_channel produces a
    // channel — letting us detect dispatch purely by channel presence.
    let pose = NiQuatTransform {
        translation: NiPoint3 {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        },
        rotation: [1.0, 0.0, 0.0, 0.0],
        scale: 1.0,
    };
    let lookat = NiLookAtInterpolator {
        flags: 0,
        look_at: BlockRef::NULL,
        look_at_name: None,
        transform: pose,
        interp_translation: BlockRef::NULL,
        interp_roll: BlockRef::NULL,
        interp_scale: BlockRef::NULL,
    };
    let scene = NifScene {
        blocks: vec![Box::new(lookat)],
        ..NifScene::default()
    };

    let make_seq = |ctrl_type: &str| {
        let mut cb = dummy_controlled_block();
        cb.interpolator_ref = BlockRef(0);
        cb.node_name = Some(Arc::from("Bip01"));
        cb.controller_type = Some(Arc::from(ctrl_type));
        NiControllerSequence {
            name: Some(Arc::from("seq")),
            controlled_blocks: vec![cb],
            array_grow_by: 0,
            weight: 1.0,
            text_keys_ref: BlockRef::NULL,
            cycle_type: 0,
            frequency: 1.0,
            phase: 0.0,
            start_time: 0.0,
            stop_time: 1.0,
            manager_ref: BlockRef::NULL,
            accum_root_name: None,
            anim_note_refs: Vec::new(),
        }
    };

    // Baseline: the modern controller name resolves to a channel.
    let modern = import_sequence(&scene, &make_seq("NiTransformController"));
    assert!(
        modern.channels.contains_key("Bip01"),
        "NiTransformController must produce a transform channel"
    );
    // Fix under test: the classic alias must ALSO dispatch, not drop.
    let classic = import_sequence(&scene, &make_seq("NiKeyframeController"));
    assert!(
        classic.channels.contains_key("Bip01"),
        "NiKeyframeController alias must dispatch to transform extraction (#1442)"
    );
}

/// #3097 SIBLING gap — `NiControllerSequence.Phase` was parsed since
/// M21 but `import_sequence` never carried it onto the resulting
/// `AnimationClip`. Not the primary bug in #3097 (that's the
/// embedded-controller path hardcoding `Loop`/`1.0`/`duration 0.0`),
/// but the same "parsed and discarded" shape in the sibling import
/// path the issue asked to check.
#[test]
fn import_sequence_carries_authored_phase() {
    use crate::blocks::controller::NiControllerSequence;
    use crate::types::BlockRef;

    let scene = NifScene::default();
    let seq = NiControllerSequence {
        name: Some(Arc::from("seq")),
        controlled_blocks: Vec::new(),
        array_grow_by: 0,
        weight: 1.0,
        text_keys_ref: BlockRef::NULL,
        cycle_type: 0,
        frequency: 1.0,
        phase: 0.6,
        start_time: 0.0,
        stop_time: 1.0,
        manager_ref: BlockRef::NULL,
        accum_root_name: None,
        anim_note_refs: Vec::new(),
    };

    let clip = import_sequence(&scene, &seq);
    assert!((clip.phase - 0.6).abs() < 1e-6);
}
