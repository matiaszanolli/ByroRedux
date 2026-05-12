//! Tests for `tests` extracted from ../anim.rs (refactor stage A).
//!
//! Same qualified path preserved (`tests::FOO`).

use super::*;

#[test]
fn resolve_morph_target_index_by_name() {
    use crate::blocks::controller::{MorphTarget, NiTimeControllerBase};
    use crate::types::BlockRef;

    // Build a scene with: [0] NiMorphData (3 named targets), [1] NiGeomMorpherController.
    let morph_data = NiMorphData {
        num_vertices: 0,
        relative_targets: 0,
        morphs: vec![
            MorphTarget {
                name: Some(Arc::from("Blink")),
                vectors: vec![],
            },
            MorphTarget {
                name: Some(Arc::from("JawOpen")),
                vectors: vec![],
            },
            MorphTarget {
                name: Some(Arc::from("BrowUp")),
                vectors: vec![],
            },
        ],
    };
    let morpher = NiGeomMorpherController {
        base: NiTimeControllerBase {
            next_controller_ref: BlockRef::NULL,
            flags: 0,
            frequency: 1.0,
            phase: 0.0,
            start_time: 0.0,
            stop_time: 1.0,
            target_ref: BlockRef::NULL,
        },
        morpher_flags: 0,
        data_ref: BlockRef(0),
        always_update: 0,
        interpolator_weights: vec![],
    };
    let scene = NifScene {
        blocks: vec![Box::new(morph_data), Box::new(morpher)],
        ..NifScene::default()
    };

    // Controlled block pointing at the morpher with controller_id = "JawOpen".
    let mut cb = dummy_controlled_block();
    cb.controller_ref = BlockRef(1);
    cb.controller_id = Some(Arc::from("JawOpen"));
    assert_eq!(resolve_morph_target_index(&scene, &cb), Some(1));

    // Case-insensitive match.
    cb.controller_id = Some(Arc::from("blink"));
    assert_eq!(resolve_morph_target_index(&scene, &cb), Some(0));

    // Missing name returns None (caller falls back to 0).
    cb.controller_id = Some(Arc::from("NotARealMorph"));
    assert_eq!(resolve_morph_target_index(&scene, &cb), None);

    // Null controller_ref returns None.
    cb.controller_ref = BlockRef::NULL;
    assert_eq!(resolve_morph_target_index(&scene, &cb), None);
}

fn dummy_controlled_block() -> ControlledBlock {
    ControlledBlock {
        interpolator_ref: crate::types::BlockRef::NULL,
        controller_ref: crate::types::BlockRef::NULL,
        priority: 0,
        node_name: None,
        property_type: None,
        controller_type: None,
        controller_id: None,
        interpolator_id: None,
        string_palette_ref: crate::types::BlockRef::NULL,
        node_name_offset: 0,
        property_type_offset: 0,
        controller_type_offset: 0,
        controller_id_offset: 0,
        interpolator_id_offset: 0,
    }
}

/// Regression: #334 (AR-08). A ControlledBlock pointing at a
/// NiBlendTransformInterpolator must still produce a transform
/// channel — the resolver picks the dominant sub-interpolator
/// (highest normalized_weight) and the extractor recurses into it.
/// Pre-fix the extractor returned None on the blend type and
/// multi-sequence NPC animations silently lost every channel.
#[test]
fn extract_transform_channel_follows_blend_to_dominant_sub_interp() {
    use crate::blocks::interpolator::{
        InterpBlendItem, KeyGroup, NiBlendInterpolator, NiBlendTransformInterpolator,
        NiTransformData, NiTransformInterpolator,
    };
    use crate::types::{BlockRef, NiQuatTransform};

    // Scene layout:
    //   [0] NiTransformData (dominant — carries a single scale key)
    //   [1] NiTransformData (secondary — empty)
    //   [2] NiTransformInterpolator referencing [0]
    //   [3] NiTransformInterpolator referencing [1]
    //   [4] NiBlendTransformInterpolator with items [2]@0.8 + [3]@0.2
    let empty_floats = KeyGroup::<FloatKey> {
        key_type: KeyType::Linear,
        keys: Vec::new(),
    };
    let empty_vec3s = KeyGroup::<Vec3Key> {
        key_type: KeyType::Linear,
        keys: Vec::new(),
    };
    let dominant_data = NiTransformData {
        rotation_type: None,
        rotation_keys: Vec::new(),
        xyz_rotations: None,
        translations: empty_vec3s.clone(),
        scales: KeyGroup {
            key_type: KeyType::Linear,
            keys: vec![FloatKey {
                time: 0.0,
                value: 1.5,
                tangent_forward: 0.0,
                tangent_backward: 0.0,
                tbc: None,
            }],
        },
    };
    let secondary_data = NiTransformData {
        rotation_type: None,
        rotation_keys: Vec::new(),
        xyz_rotations: None,
        translations: empty_vec3s,
        scales: empty_floats,
    };
    let dom_interp = NiTransformInterpolator {
        transform: NiQuatTransform::default(),
        data_ref: BlockRef(0),
    };
    let sec_interp = NiTransformInterpolator {
        transform: NiQuatTransform::default(),
        data_ref: BlockRef(1),
    };
    let blend = NiBlendTransformInterpolator {
        base: NiBlendInterpolator {
            flags: 0, // not manager-controlled, so items is live
            array_size: 2,
            weight_threshold: 0.0,
            manager_controlled: false,
            interp_count: 2,
            single_index: 0,
            items: vec![
                InterpBlendItem {
                    interpolator_ref: BlockRef(2),
                    weight: 0.8,
                    normalized_weight: 0.8,
                    priority: 0,
                    ease_spinner: 0.0,
                },
                InterpBlendItem {
                    interpolator_ref: BlockRef(3),
                    weight: 0.2,
                    normalized_weight: 0.2,
                    priority: 0,
                    ease_spinner: 0.0,
                },
            ],
        },
    };
    let scene = NifScene {
        blocks: vec![
            Box::new(dominant_data),
            Box::new(secondary_data),
            Box::new(dom_interp),
            Box::new(sec_interp),
            Box::new(blend),
        ],
        ..NifScene::default()
    };

    let mut cb = dummy_controlled_block();
    cb.interpolator_ref = BlockRef(4); // point at the blend

    let channel = extract_transform_channel(&scene, &cb)
        .expect("blend transform interpolator must resolve to the dominant sub-interp");
    assert_eq!(
        channel.scale_keys.len(),
        1,
        "must reach dominant data's scales"
    );
    assert!((channel.scale_keys[0].value - 1.5).abs() < 1e-6);
}

/// #604 — NiLookAtInterpolator must produce a constant TransformChannel
/// from its static `transform` pose instead of returning None. Pre-fix
/// the dispatch had no third branch and embedded look-at chains in
/// FNV / SkyrimSE silently dropped every channel.
#[test]
fn extract_transform_channel_emits_constant_pose_for_lookat() {
    use crate::types::{BlockRef, NiPoint3, NiQuatTransform};

    // Static pose with a 90° rotation around Z-up Z (= around Y-up Y
    // after coord conversion). Translation + scale are both
    // non-default so the test catches a coord-handling regression on
    // any field.
    let half = std::f32::consts::FRAC_1_SQRT_2; // sin(45°) = cos(45°)
    let zup_quat = [half, 0.0, 0.0, half]; // (w, x, y, z) = 90° around +Z
    let pose = NiQuatTransform {
        translation: NiPoint3 {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        },
        rotation: zup_quat,
        scale: 0.75,
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

    let mut cb = dummy_controlled_block();
    cb.interpolator_ref = BlockRef(0);

    let channel = extract_transform_channel(&scene, &cb)
        .expect("NiLookAtInterpolator must emit a constant transform channel");
    assert_eq!(channel.translation_keys.len(), 1);
    assert_eq!(channel.rotation_keys.len(), 1);
    assert_eq!(channel.scale_keys.len(), 1);

    // Translation Z-up → Y-up: (1, 2, 3) → (1, 3, -2).
    let t = channel.translation_keys[0].value;
    assert!((t[0] - 1.0).abs() < 1e-6);
    assert!((t[1] - 3.0).abs() < 1e-6);
    assert!((t[2] + 2.0).abs() < 1e-6);

    // Rotation: Z-up (w,x,y,z) = (√2/2, 0, 0, √2/2) → glam (x,y,z,w)
    // via zup_to_yup_quat = (0, √2/2, 0, √2/2).
    let r = channel.rotation_keys[0].value;
    assert!(r[0].abs() < 1e-6);
    assert!((r[1] - half).abs() < 1e-6);
    assert!(r[2].abs() < 1e-6);
    assert!((r[3] - half).abs() < 1e-6);

    // Scale passes through unchanged.
    assert!((channel.scale_keys[0].value - 0.75).abs() < 1e-6);

    // Time stamps default to 0 — single-key constant channel.
    assert_eq!(channel.translation_keys[0].time, 0.0);
    assert_eq!(channel.rotation_keys[0].time, 0.0);
    assert_eq!(channel.scale_keys[0].time, 0.0);
}

/// #772 — FLT_MAX in any TRS axis of an interpolator pose value is
/// Bethesda's "axis inactive" sentinel; the importer must NOT
/// materialise it as a real key, or the apply phase writes infinity
/// to the bone's Transform and skinned vertices fly off-screen
/// (FNV Doc Mitchell finger bones / FO3 TestQAHairM 31→0 vanish).
/// Same FLT_MAX-as-no-value convention as BSShaderPPLighting's
/// rimlight gate at `crates/nif/src/blocks/shader.rs:977-978`.
#[test]
fn extract_transform_channel_drops_flt_max_pose_axes_for_lookat() {
    use crate::types::{BlockRef, NiPoint3, NiQuatTransform};

    // Pose with FLT_MAX on every axis — no static pose value at all.
    // Empirically observed on FNV `mtidle.kf` finger / twist bones
    // when bound through B-spline interpolators with no translation
    // payload; the same NiQuatTransform shape is shared across
    // `NiTransformInterpolator` / `NiBSplineCompTransformInterpolator`
    // / `NiLookAtInterpolator` so the gate must apply uniformly.
    let inactive_pose = NiQuatTransform {
        translation: NiPoint3 {
            x: -f32::MAX,
            y: -f32::MAX,
            z: -f32::MAX,
        },
        rotation: [-f32::MAX, -f32::MAX, -f32::MAX, -f32::MAX],
        scale: -f32::MAX,
    };
    let lookat = NiLookAtInterpolator {
        flags: 0,
        look_at: BlockRef::NULL,
        look_at_name: None,
        transform: inactive_pose,
        interp_translation: BlockRef::NULL,
        interp_roll: BlockRef::NULL,
        interp_scale: BlockRef::NULL,
    };
    let scene = NifScene {
        blocks: vec![Box::new(lookat)],
        ..NifScene::default()
    };
    let mut cb = dummy_controlled_block();
    cb.interpolator_ref = BlockRef(0);

    let channel = extract_transform_channel(&scene, &cb)
        .expect("FLT_MAX pose still produces an empty TransformChannel, not None");
    assert!(
        channel.translation_keys.is_empty(),
        "FLT_MAX translation must not materialise as a key"
    );
    assert!(
        channel.rotation_keys.is_empty(),
        "FLT_MAX rotation must not materialise as a key"
    );
    assert!(
        channel.scale_keys.is_empty(),
        "FLT_MAX scale must not materialise as a key"
    );
}

/// #772 sibling — partial FLT_MAX (translation inactive, rotation
/// authored). The translation axis must drop while rotation passes
/// through. mtidle.kf for finger bones is exactly this shape: no
/// translation payload, real rotation curve.
#[test]
fn extract_transform_channel_keeps_authored_axes_when_translation_is_flt_max() {
    use crate::types::{BlockRef, NiPoint3, NiQuatTransform};

    let half = std::f32::consts::FRAC_1_SQRT_2;
    let mixed_pose = NiQuatTransform {
        translation: NiPoint3 {
            x: -f32::MAX,
            y: -f32::MAX,
            z: -f32::MAX,
        },
        rotation: [half, 0.0, 0.0, half], // 90° around +Z, real
        scale: 1.0,                       // identity scale, real
    };
    let lookat = NiLookAtInterpolator {
        flags: 0,
        look_at: BlockRef::NULL,
        look_at_name: None,
        transform: mixed_pose,
        interp_translation: BlockRef::NULL,
        interp_roll: BlockRef::NULL,
        interp_scale: BlockRef::NULL,
    };
    let scene = NifScene {
        blocks: vec![Box::new(lookat)],
        ..NifScene::default()
    };
    let mut cb = dummy_controlled_block();
    cb.interpolator_ref = BlockRef(0);

    let channel = extract_transform_channel(&scene, &cb).expect("mixed pose channel");
    assert!(channel.translation_keys.is_empty());
    assert_eq!(channel.rotation_keys.len(), 1);
    assert_eq!(channel.scale_keys.len(), 1);
    assert!((channel.scale_keys[0].value - 1.0).abs() < 1e-6);
}

/// #605 — NiPathInterpolator must emit translation keys sampled
/// from its referenced NiPosData (Z-up → Y-up converted, interpolation
/// type preserved). Rotation/scale stay identity matching legacy
/// Gamebryo path-interpolator behavior. Pre-fix the dispatch had no
/// fourth branch and embedded path animations (door swings, moving
/// platforms, dragon flight curves) silently static-posed.
#[test]
fn extract_transform_channel_emits_path_keys_for_path_interpolator() {
    use crate::blocks::interpolator::Vec3Key;
    use crate::types::BlockRef;

    // Three-point path in Z-up: start (0,0,0), midpoint (10,0,5),
    // end (20,0,0) — a simple arch. Times 0, 1, 2 seconds.
    let pos_data = NiPosData {
        keys: KeyGroup::<Vec3Key> {
            key_type: KeyType::Linear,
            keys: vec![
                Vec3Key {
                    time: 0.0,
                    value: [0.0, 0.0, 0.0],
                    tangent_forward: [0.0; 3],
                    tangent_backward: [0.0; 3],
                    tbc: None,
                },
                Vec3Key {
                    time: 1.0,
                    value: [10.0, 0.0, 5.0],
                    tangent_forward: [0.0; 3],
                    tangent_backward: [0.0; 3],
                    tbc: None,
                },
                Vec3Key {
                    time: 2.0,
                    value: [20.0, 0.0, 0.0],
                    tangent_forward: [0.0; 3],
                    tangent_backward: [0.0; 3],
                    tbc: None,
                },
            ],
        },
    };
    let path_interp = NiPathInterpolator {
        flags: 0,
        bank_dir: 0,
        max_bank_angle: 0.0,
        smoothing: 0.0,
        follow_axis: 0,
        path_data_ref: BlockRef(0),
        percent_data_ref: BlockRef::NULL,
    };
    let scene = NifScene {
        blocks: vec![Box::new(pos_data), Box::new(path_interp)],
        ..NifScene::default()
    };

    let mut cb = dummy_controlled_block();
    cb.interpolator_ref = BlockRef(1);

    let channel = extract_transform_channel(&scene, &cb)
        .expect("NiPathInterpolator must emit a translation channel from its NiPosData");

    // Three keys round-tripped from path data, Z-up → Y-up:
    // (x, y, z) → (x, z, -y).  (10, 0, 5) → (10, 5, 0).
    assert_eq!(channel.translation_keys.len(), 3);
    assert_eq!(channel.translation_keys[0].value, [0.0, 0.0, 0.0]);
    assert_eq!(channel.translation_keys[1].value, [10.0, 5.0, 0.0]);
    assert_eq!(channel.translation_keys[2].value, [20.0, 0.0, 0.0]);
    assert_eq!(channel.translation_keys[0].time, 0.0);
    assert_eq!(channel.translation_keys[1].time, 1.0);
    assert_eq!(channel.translation_keys[2].time, 2.0);
    assert_eq!(channel.translation_type, KeyType::Linear);

    // Rotation is identity, single key — Gamebryo's documented
    // path-interp default.
    assert_eq!(channel.rotation_keys.len(), 1);
    assert_eq!(channel.rotation_keys[0].value, [0.0, 0.0, 0.0, 1.0]);

    // Scale identity, single key.
    assert_eq!(channel.scale_keys.len(), 1);
    assert_eq!(channel.scale_keys[0].value, 1.0);
}

/// Edge case: NiPathInterpolator with a null path_data_ref or with
/// referenced NiPosData carrying zero keys returns None — there's no
/// useful animation to emit, and downstream handles None as "skip
/// this channel" via the existing fall-through.
#[test]
fn extract_transform_channel_returns_none_for_empty_path() {
    use crate::types::BlockRef;

    // Case 1 — null path_data_ref.
    let path_interp = NiPathInterpolator {
        flags: 0,
        bank_dir: 0,
        max_bank_angle: 0.0,
        smoothing: 0.0,
        follow_axis: 0,
        path_data_ref: BlockRef::NULL,
        percent_data_ref: BlockRef::NULL,
    };
    let scene = NifScene {
        blocks: vec![Box::new(path_interp)],
        ..NifScene::default()
    };
    let mut cb = dummy_controlled_block();
    cb.interpolator_ref = BlockRef(0);
    assert!(extract_transform_channel(&scene, &cb).is_none());

    // Case 2 — empty NiPosData.
    let empty_pos = NiPosData {
        keys: KeyGroup::<Vec3Key> {
            key_type: KeyType::Linear,
            keys: Vec::new(),
        },
    };
    let path_interp = NiPathInterpolator {
        flags: 0,
        bank_dir: 0,
        max_bank_angle: 0.0,
        smoothing: 0.0,
        follow_axis: 0,
        path_data_ref: BlockRef(0),
        percent_data_ref: BlockRef::NULL,
    };
    let scene = NifScene {
        blocks: vec![Box::new(empty_pos), Box::new(path_interp)],
        ..NifScene::default()
    };
    let mut cb = dummy_controlled_block();
    cb.interpolator_ref = BlockRef(1);
    assert!(extract_transform_channel(&scene, &cb).is_none());
}

/// The resolver picks the item with the HIGHEST normalized_weight.
/// Ties go to either item (we pick deterministically via
/// `max_by` → first-max-wins-in-iteration-order) but the test
/// asserts the non-tied case explicitly.
#[test]
fn resolve_blend_picks_highest_normalized_weight() {
    use crate::blocks::interpolator::{
        InterpBlendItem, NiBlendInterpolator, NiBlendTransformInterpolator,
    };
    use crate::types::BlockRef;

    let blend = NiBlendTransformInterpolator {
        base: NiBlendInterpolator {
            flags: 0,
            array_size: 3,
            weight_threshold: 0.0,
            manager_controlled: false,
            interp_count: 3,
            single_index: 0,
            items: vec![
                InterpBlendItem {
                    interpolator_ref: BlockRef(10),
                    weight: 0.1,
                    normalized_weight: 0.1,
                    priority: 0,
                    ease_spinner: 0.0,
                },
                InterpBlendItem {
                    interpolator_ref: BlockRef(20),
                    weight: 0.9,
                    normalized_weight: 0.9, // dominant
                    priority: 0,
                    ease_spinner: 0.0,
                },
                InterpBlendItem {
                    interpolator_ref: BlockRef(30),
                    weight: 0.3,
                    normalized_weight: 0.3,
                    priority: 0,
                    ease_spinner: 0.0,
                },
            ],
        },
    };
    let scene = NifScene {
        blocks: vec![Box::new(blend)],
        ..NifScene::default()
    };
    assert_eq!(resolve_blend_interpolator_target(&scene, 0), Some(20));
}

/// Manager-controlled blend (flag bit 0) has an empty `items`
/// array — sub-interpolators are driven externally by the
/// NiControllerManager via sibling ControlledBlocks. Resolver
/// returns None so the caller cleanly produces no channel; the
/// manager's other sequences supply the data through their own
/// interpolator_refs.
#[test]
fn resolve_blend_returns_none_for_manager_controlled() {
    use crate::blocks::interpolator::{NiBlendInterpolator, NiBlendTransformInterpolator};

    let blend = NiBlendTransformInterpolator {
        base: NiBlendInterpolator {
            flags: 1, // bit 0 = manager_controlled
            array_size: 0,
            weight_threshold: 0.0,
            manager_controlled: true,
            interp_count: 0,
            single_index: 0,
            items: Vec::new(),
        },
    };
    let scene = NifScene {
        blocks: vec![Box::new(blend)],
        ..NifScene::default()
    };
    assert_eq!(resolve_blend_interpolator_target(&scene, 0), None);
}

/// Non-blend interpolators must not be touched by the resolver —
/// it returns None so the caller falls through to the direct path.
#[test]
fn resolve_blend_returns_none_for_non_blend_interpolator() {
    use crate::blocks::interpolator::NiTransformInterpolator;
    use crate::types::{BlockRef, NiQuatTransform};

    let interp = NiTransformInterpolator {
        transform: NiQuatTransform::default(),
        data_ref: BlockRef::NULL,
    };
    let scene = NifScene {
        blocks: vec![Box::new(interp)],
        ..NifScene::default()
    };
    assert_eq!(resolve_blend_interpolator_target(&scene, 0), None);
}

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

#[test]
fn cycle_type_from_u32() {
    assert_eq!(CycleType::from_u32(0), CycleType::Clamp);
    assert_eq!(CycleType::from_u32(1), CycleType::Loop);
    assert_eq!(CycleType::from_u32(2), CycleType::Reverse);
    assert_eq!(CycleType::from_u32(99), CycleType::Clamp);
}

#[test]
fn zup_to_yup_position() {
    // Gamebryo Z-up (1, 2, 3) → Y-up (1, 3, -2)
    let result = zup_to_yup_pos([1.0, 2.0, 3.0]);
    assert_eq!(result, [1.0, 3.0, -2.0]);
}

#[test]
fn zup_to_yup_identity_quat() {
    // Gamebryo identity (w=1, x=0, y=0, z=0) → glam (x=0, y=0, z=0, w=1)
    let result = zup_to_yup_quat([1.0, 0.0, 0.0, 0.0]);
    assert_eq!(result, [0.0, 0.0, 0.0, 1.0]);
}

#[test]
fn empty_scene_produces_no_clips() {
    let scene = NifScene::default();
    let clips = import_kf(&scene);
    assert!(clips.is_empty());
}

#[test]
fn euler_to_quat_identity() {
    // All angles zero → identity quaternion (w=1, x=0, y=0, z=0)
    let [w, x, y, z] = euler_to_quat_wxyz(0.0, 0.0, 0.0);
    assert!((w - 1.0).abs() < 1e-6);
    assert!(x.abs() < 1e-6);
    assert!(y.abs() < 1e-6);
    assert!(z.abs() < 1e-6);
}

#[test]
fn euler_to_quat_90_deg_x() {
    use std::f32::consts::FRAC_PI_2;
    // 90° around X: quat = (cos(45°), sin(45°), 0, 0) = (~0.707, ~0.707, 0, 0)
    let [w, x, y, z] = euler_to_quat_wxyz(FRAC_PI_2, 0.0, 0.0);
    let s = FRAC_PI_2.sin() * 0.5_f32.sqrt(); // sin(45°)
    let c = FRAC_PI_2.cos() * 0.5_f32.sqrt(); // cos(45°) — but let's just check magnitude
    assert!(
        (w * w + x * x + y * y + z * z - 1.0).abs() < 1e-5,
        "quaternion should be unit"
    );
    assert!(x > 0.5, "x component should be dominant for X rotation");
    assert!(y.abs() < 1e-5);
    assert!(z.abs() < 1e-5);
}

#[test]
fn euler_to_quat_90_deg_y() {
    use std::f32::consts::FRAC_PI_2;
    let [w, x, y, z] = euler_to_quat_wxyz(0.0, FRAC_PI_2, 0.0);
    assert!((w * w + x * x + y * y + z * z - 1.0).abs() < 1e-5);
    assert!(x.abs() < 1e-5);
    assert!(y > 0.5, "y component should be dominant for Y rotation");
    assert!(z.abs() < 1e-5);
}

#[test]
fn euler_to_quat_90_deg_z() {
    use std::f32::consts::FRAC_PI_2;
    let [w, x, y, z] = euler_to_quat_wxyz(0.0, 0.0, FRAC_PI_2);
    assert!((w * w + x * x + y * y + z * z - 1.0).abs() < 1e-5);
    assert!(x.abs() < 1e-5);
    assert!(y.abs() < 1e-5);
    assert!(z > 0.5, "z component should be dominant for Z rotation");
}

#[test]
fn sample_float_key_group_linear() {
    let group = KeyGroup {
        key_type: KeyType::Linear,
        keys: vec![
            FloatKey {
                time: 0.0,
                value: 0.0,
                tangent_forward: 0.0,
                tangent_backward: 0.0,
                tbc: None,
            },
            FloatKey {
                time: 1.0,
                value: 1.0,
                tangent_forward: 0.0,
                tangent_backward: 0.0,
                tbc: None,
            },
        ],
    };
    assert!((sample_float_key_group(&group, 0.5) - 0.5).abs() < 1e-5);
    assert!((sample_float_key_group(&group, 0.0) - 0.0).abs() < 1e-5);
    assert!((sample_float_key_group(&group, 1.0) - 1.0).abs() < 1e-5);
}

#[test]
fn sample_float_key_group_empty() {
    let group = KeyGroup {
        key_type: KeyType::Linear,
        keys: vec![],
    };
    assert_eq!(sample_float_key_group(&group, 0.5), 0.0);
}

#[test]
fn sample_float_key_group_single() {
    let group = KeyGroup {
        key_type: KeyType::Linear,
        keys: vec![FloatKey {
            time: 0.5,
            value: 42.0,
            tangent_forward: 0.0,
            tangent_backward: 0.0,
            tbc: None,
        }],
    };
    assert_eq!(sample_float_key_group(&group, 0.0), 42.0);
    assert_eq!(sample_float_key_group(&group, 1.0), 42.0);
}

#[test]
fn convert_xyz_euler_keys_produces_rotation_keys() {
    use std::f32::consts::FRAC_PI_2;
    // Create NiTransformData with XYZ euler rotation keys:
    // At t=0: all angles 0 (identity)
    // At t=1: 90° around X
    let x_keys = KeyGroup {
        key_type: KeyType::Linear,
        keys: vec![
            FloatKey {
                time: 0.0,
                value: 0.0,
                tangent_forward: 0.0,
                tangent_backward: 0.0,
                tbc: None,
            },
            FloatKey {
                time: 1.0,
                value: FRAC_PI_2,
                tangent_forward: 0.0,
                tangent_backward: 0.0,
                tbc: None,
            },
        ],
    };
    let empty_keys = KeyGroup {
        key_type: KeyType::Linear,
        keys: vec![
            FloatKey {
                time: 0.0,
                value: 0.0,
                tangent_forward: 0.0,
                tangent_backward: 0.0,
                tbc: None,
            },
            FloatKey {
                time: 1.0,
                value: 0.0,
                tangent_forward: 0.0,
                tangent_backward: 0.0,
                tbc: None,
            },
        ],
    };

    let data = NiTransformData {
        rotation_type: Some(KeyType::XyzRotation),
        rotation_keys: Vec::new(),
        xyz_rotations: Some([x_keys, empty_keys.clone(), empty_keys]),
        translations: KeyGroup {
            key_type: KeyType::Linear,
            keys: Vec::new(),
        },
        scales: KeyGroup {
            key_type: KeyType::Linear,
            keys: Vec::new(),
        },
    };

    let (keys, key_type) = convert_xyz_euler_keys(&data);
    assert_eq!(key_type, KeyType::Linear);
    assert_eq!(
        keys.len(),
        2,
        "should have 2 rotation keys (one per unique timestamp)"
    );

    // First key (t=0): identity → after Z-up to Y-up, glam format (x, y, z, w)
    let k0 = &keys[0];
    assert!((k0.time).abs() < 1e-5);
    // Identity quat in glam: (0, 0, 0, 1)
    assert!(
        (k0.value[3] - 1.0).abs() < 1e-4,
        "w should be ~1 for identity: {:?}",
        k0.value
    );

    // Second key (t=1): 90° around X in Z-up, then converted to Y-up
    let k1 = &keys[1];
    assert!((k1.time - 1.0).abs() < 1e-5);
    // Should be a unit quaternion
    let len_sq = k1.value.iter().map(|v| v * v).sum::<f32>();
    assert!(
        (len_sq - 1.0).abs() < 1e-4,
        "quaternion should be unit: {:?}",
        k1.value
    );
}

// ── B-spline evaluator tests (issue #155) ──────────────────────────

#[test]
fn bspline_dequant_midpoint() {
    // raw=0 → offset; raw=32767 → offset + half_range; raw=-32767 → offset - half_range
    assert!((dequant(0, 10.0, 5.0) - 10.0).abs() < 1e-5);
    assert!((dequant(32767, 10.0, 5.0) - 15.0).abs() < 1e-4);
    assert!((dequant(-32767, 10.0, 5.0) - 5.0).abs() < 1e-4);
}

#[test]
fn deboor_cubic_clamped_endpoints() {
    // With 4 control points on a single-scalar channel, the cubic
    // B-spline at u=0 should equal CP[0], at u=1 should equal CP[3]
    // because an open uniform knot vector is fully clamped at both
    // ends for the minimum degree-3 case.
    let cps = vec![1.0, 2.0, 3.0, 10.0];
    let v0 = deboor_cubic(&cps, 4, 1, 0.0);
    let v1 = deboor_cubic(&cps, 4, 1, 1.0);
    assert!(
        (v0[0] - 1.0).abs() < 1e-4,
        "u=0 should give CP[0], got {}",
        v0[0]
    );
    assert!(
        (v1[0] - 10.0).abs() < 1e-4,
        "u=1 should give CP[3], got {}",
        v1[0]
    );
}

#[test]
fn deboor_cubic_monotone_between_endpoints() {
    // With a monotone CP sequence and a monotone knot parameter,
    // the evaluated curve should also be monotone (not strictly,
    // but the sign of successive differences should agree).
    let cps = vec![0.0, 1.0, 2.0, 3.0, 4.0];
    let n = 5;
    let u_max = (n - BSPLINE_DEGREE) as f32;
    let mut prev = f32::NEG_INFINITY;
    for i in 0..=10 {
        let u = u_max * (i as f32 / 10.0);
        let v = deboor_cubic(&cps, n, 1, u)[0];
        assert!(
            v >= prev - 1e-4,
            "non-monotone: v[{}] = {} < prev {}",
            i,
            v,
            prev
        );
        prev = v;
    }
}

#[test]
fn bspline_channel_slice_invalid_handle() {
    let raw: Vec<i16> = vec![0; 100];
    assert!(channel_slice(u32::MAX, &raw, 4, 3, 0.0, 1.0).is_none());
}

#[test]
fn bspline_channel_slice_out_of_bounds() {
    let raw: Vec<i16> = vec![0; 10];
    // Needs 4 * 3 = 12 slots starting at handle 0 → should fail (only 10).
    assert!(channel_slice(0, &raw, 4, 3, 0.0, 1.0).is_none());
}

#[test]
fn bspline_channel_slice_dequantizes() {
    // 4 CPs × stride 1, raw values [0, 32767, -32767, 0]
    // with offset=10, half_range=5 → [10, 15, 5, 10]
    let raw: Vec<i16> = vec![0, 32767, -32767, 0];
    let out = channel_slice(0, &raw, 4, 1, 10.0, 5.0).unwrap();
    assert_eq!(out.len(), 4);
    assert!((out[0] - 10.0).abs() < 1e-4);
    assert!((out[1] - 15.0).abs() < 1e-4);
    assert!((out[2] - 5.0).abs() < 1e-4);
    assert!((out[3] - 10.0).abs() < 1e-4);
}

/// Regression: #261. A NiNode with a `NiTextureTransformController`
/// on `controller_ref` must surface as a looping `AnimationClip`
/// carrying a `FloatTarget::UvOffsetU` channel keyed by the node
/// name. Pre-fix the controller_ref was dropped on the floor during
/// import — water/lava meshes rendered static.
#[test]
fn import_embedded_animations_captures_texture_transform_controller() {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::controller::{NiTextureTransformController, NiTimeControllerBase};
    use crate::blocks::interpolator::{FloatKey, KeyGroup, KeyType};
    use crate::blocks::node::NiNode;
    use crate::types::{BlockRef, NiTransform};
    use std::sync::Arc;

    // Scene layout:
    //   [0] NiFloatData (two linear keys, value 0→0.5 over 1 s)
    //   [1] NiFloatInterpolator → [0]
    //   [2] NiTextureTransformController → interp=[1], operation=0 (UvOffsetU)
    //   [3] NiNode (name="WaterPlane") with controller_ref=[2]
    let data = NiFloatData {
        keys: KeyGroup {
            key_type: KeyType::Linear,
            keys: vec![
                FloatKey {
                    time: 0.0,
                    value: 0.0,
                    tangent_forward: 0.0,
                    tangent_backward: 0.0,
                    tbc: None,
                },
                FloatKey {
                    time: 1.0,
                    value: 0.5,
                    tangent_forward: 0.0,
                    tangent_backward: 0.0,
                    tbc: None,
                },
            ],
        },
    };
    let interp = NiFloatInterpolator {
        value: 0.0,
        data_ref: BlockRef(0),
    };
    let ctrl = NiTextureTransformController {
        base: NiTimeControllerBase {
            next_controller_ref: BlockRef::NULL,
            flags: 0,
            frequency: 1.0,
            phase: 0.0,
            start_time: 0.0,
            stop_time: 1.0,
            target_ref: BlockRef::NULL,
        },
        interpolator_ref: BlockRef(1),
        shader_map: false,
        texture_slot: 0,
        operation: 0, // UvOffsetU
    };
    let node = NiNode {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(Arc::from("WaterPlane")),
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef(2),
            },
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        children: Vec::new(),
        effects: Vec::new(),
    };
    let scene = NifScene {
        blocks: vec![
            Box::new(data),
            Box::new(interp),
            Box::new(ctrl),
            Box::new(node),
        ],
        ..NifScene::default()
    };

    let clip = import_embedded_animations(&scene).expect("expected embedded clip");
    assert_eq!(clip.cycle_type, CycleType::Loop);
    assert!((clip.frequency - 1.0).abs() < 1e-6);
    assert!((clip.duration - 1.0).abs() < 1e-6);
    assert_eq!(
        clip.float_channels.len(),
        1,
        "exactly one float channel expected"
    );
    let (node_name, ch) = &clip.float_channels[0];
    assert_eq!(&**node_name, "WaterPlane");
    assert!(
        matches!(ch.target, FloatTarget::UvOffsetU),
        "expected UvOffsetU, got {:?}",
        ch.target
    );
    assert_eq!(ch.keys.len(), 2);
    assert!((ch.keys[1].value - 0.5).abs() < 1e-6);
}

/// Regression: #545. A NiTriShape with a `NiFlipController` on
/// `controller_ref` must surface as a looping `AnimationClip`
/// carrying a `texture_flip_channels` entry whose `source_paths`
/// list resolves the controller's `sources` BlockRefs against the
/// underlying `NiSourceTexture.filename` strings, in order. Pre-fix
/// the controller_ref walked into `_ => debug!("Skipping unsupported
/// embedded controller type")` and Oblivion / FO3 / FNV fire / smoke /
/// torch flame meshes rendered with a frozen first frame.
#[test]
fn import_embedded_animations_captures_flip_controller() {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::controller::{
        NiFlipController, NiSingleInterpController, NiTimeControllerBase,
    };
    use crate::blocks::interpolator::{FloatKey, KeyGroup, KeyType};
    use crate::blocks::texture::NiSourceTexture;
    use crate::blocks::tri_shape::NiTriShape;
    use crate::types::{BlockRef, NiTransform};
    use std::sync::Arc;

    // Scene layout:
    //   [0] NiFloatData (two linear keys, 0→1 over 1 s — flipbook ramp)
    //   [1] NiFloatInterpolator → [0]
    //   [2] NiSourceTexture (filename = "fire_a.dds")
    //   [3] NiSourceTexture (filename = "fire_b.dds")
    //   [4] NiFlipController → interp=[1], texture_slot=0,
    //       sources=[[2], [3]]
    //   [5] NiTriShape (name="HearthFire") with controller_ref=[4]
    let data = NiFloatData {
        keys: KeyGroup {
            key_type: KeyType::Linear,
            keys: vec![
                FloatKey {
                    time: 0.0,
                    value: 0.0,
                    tangent_forward: 0.0,
                    tangent_backward: 0.0,
                    tbc: None,
                },
                FloatKey {
                    time: 1.0,
                    value: 2.0,
                    tangent_forward: 0.0,
                    tangent_backward: 0.0,
                    tbc: None,
                },
            ],
        },
    };
    let interp = NiFloatInterpolator {
        value: 0.0,
        data_ref: BlockRef(0),
    };
    let make_src = |name: &'static str| NiSourceTexture {
        net: NiObjectNETData {
            name: None,
            extra_data_refs: Vec::new(),
            controller_ref: BlockRef::NULL,
        },
        use_external: true,
        filename: Some(Arc::from(name)),
        pixel_data_ref: BlockRef::NULL,
        pixel_layout: 0,
        use_mipmaps: 0,
        alpha_format: 0,
        is_static: true,
    };
    let src_a = make_src("fire_a.dds");
    let src_b = make_src("fire_b.dds");
    let ctrl = NiFlipController {
        base: NiSingleInterpController {
            base: NiTimeControllerBase {
                next_controller_ref: BlockRef::NULL,
                flags: 0,
                frequency: 1.0,
                phase: 0.0,
                start_time: 0.0,
                stop_time: 1.0,
                target_ref: BlockRef::NULL,
            },
            interpolator_ref: BlockRef(1),
        },
        texture_slot: 0,
        sources: vec![BlockRef(2), BlockRef(3)],
    };
    let node = NiTriShape {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(Arc::from("HearthFire")),
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef(4),
            },
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        data_ref: BlockRef::NULL,
        skin_instance_ref: BlockRef::NULL,
        shader_property_ref: BlockRef::NULL,
        alpha_property_ref: BlockRef::NULL,
        num_materials: 0,
        active_material_index: 0,
    };
    let scene = NifScene {
        blocks: vec![
            Box::new(data),
            Box::new(interp),
            Box::new(src_a),
            Box::new(src_b),
            Box::new(ctrl),
            Box::new(node),
        ],
        ..NifScene::default()
    };

    let clip = import_embedded_animations(&scene).expect("expected embedded clip");
    assert_eq!(clip.cycle_type, CycleType::Loop);
    assert_eq!(
        clip.texture_flip_channels.len(),
        1,
        "exactly one flipbook channel expected"
    );
    let (node_name, ch) = &clip.texture_flip_channels[0];
    assert_eq!(&**node_name, "HearthFire");
    assert_eq!(ch.texture_slot, 0);
    assert_eq!(
        ch.source_paths.iter().map(|s| &**s).collect::<Vec<_>>(),
        vec!["fire_a.dds", "fire_b.dds"]
    );
    assert_eq!(ch.keys.len(), 2);
    assert!((ch.keys[1].value - 2.0).abs() < 1e-6);
}

/// Regression: #261. A NiNode with no `controller_ref` must
/// produce no clip — import_embedded_animations returns None and
/// the scene loader skips the AnimationPlayer spawn.
#[test]
fn import_embedded_animations_returns_none_when_no_controllers() {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::node::NiNode;
    use crate::types::{BlockRef, NiTransform};
    use std::sync::Arc;

    let node = NiNode {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(Arc::from("StaticCrate")),
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        children: Vec::new(),
        effects: Vec::new(),
    };
    let scene = NifScene {
        blocks: vec![Box::new(node)],
        ..NifScene::default()
    };

    assert!(
        import_embedded_animations(&scene).is_none(),
        "no-controller scene must yield no clip"
    );
}

// ── #936 / NIF-D5-NEW-01 — compact-spline float / Point3 emitters ──

/// `extract_float_channel_at` must fall back to the
/// NiBSplineCompFloatInterpolator path when the interp at `interp_idx`
/// isn't an `NiFloatInterpolator`. Builds a 4-CP scalar spline
/// (clamped open-uniform) and pins the endpoint values from the
/// generated keys. Pre-#936 the channel was dropped at dispatch time;
/// the new fallback samples it at BSPLINE_SAMPLE_HZ.
#[test]
fn extract_float_channel_at_samples_bspline_comp_float() {
    use crate::blocks::interpolator::{
        NiBSplineBasisData, NiBSplineCompFloatInterpolator, NiBSplineData,
    };
    use crate::types::BlockRef;

    // 4 CPs encoded with offset=0, half_range=10 so the quantization
    // maps raw [0, 32767, -32767, 0] → [0, 10, -10, 0]. With degree 3
    // and a 4-CP basis the curve is clamped at the endpoints: u=0
    // evaluates to CP[0] (0.0) and u=1 (= n - degree) to CP[3] (0.0).
    let data = NiBSplineData {
        float_control_points: Vec::new(),
        compact_control_points: vec![0, 32767, -32767, 0],
    };
    let basis = NiBSplineBasisData {
        num_control_points: 4,
    };
    let interp = NiBSplineCompFloatInterpolator {
        start_time: 0.0,
        stop_time: 1.0,
        spline_data_ref: BlockRef(0),
        basis_data_ref: BlockRef(1),
        value: 0.0,
        handle: 0,
        float_offset: 0.0,
        float_half_range: 10.0,
    };
    let scene = NifScene {
        blocks: vec![Box::new(data), Box::new(basis), Box::new(interp)],
        ..NifScene::default()
    };

    let ch = extract_float_channel_at(&scene, 2, FloatTarget::Alpha)
        .expect("BSpline-comp float channel must surface keys");
    assert!(
        ch.keys.len() >= 2,
        "must emit at least start + end keys, got {}",
        ch.keys.len()
    );
    let first = ch.keys.first().unwrap();
    let last = ch.keys.last().unwrap();
    assert!(
        (first.value - 0.0).abs() < 1e-3,
        "u=0 evaluates to CP[0] = 0.0, got {}",
        first.value
    );
    assert!(
        (last.value - 0.0).abs() < 1e-3,
        "u=1 evaluates to CP[3] = 0.0, got {}",
        last.value
    );
    assert!(matches!(ch.target, FloatTarget::Alpha));
}

/// Static-handle case: when the interpolator's `handle == u32::MAX`
/// the emitter falls back to a single-key channel at `start_time`
/// carrying the static `value`. Pre-#936 the channel was dropped
/// entirely.
#[test]
fn extract_float_channel_at_emits_static_key_for_invalid_handle() {
    use crate::blocks::interpolator::NiBSplineCompFloatInterpolator;
    use crate::types::BlockRef;

    let interp = NiBSplineCompFloatInterpolator {
        start_time: 0.5,
        stop_time: 1.0,
        spline_data_ref: BlockRef::NULL,
        basis_data_ref: BlockRef::NULL,
        value: 0.42,
        handle: u32::MAX,
        float_offset: 0.0,
        float_half_range: 0.0,
    };
    let scene = NifScene {
        blocks: vec![Box::new(interp)],
        ..NifScene::default()
    };

    let ch = extract_float_channel_at(&scene, 0, FloatTarget::Alpha)
        .expect("static-handle BSpline-comp float must surface a single-key channel");
    assert_eq!(ch.keys.len(), 1, "exactly one static key");
    assert_eq!(ch.keys[0].time, 0.5);
    assert!((ch.keys[0].value - 0.42).abs() < 1e-6);
}

/// `resolve_color_keys_at` must fall back to the
/// NiBSplineCompPoint3Interpolator path. Same recipe as the float
/// test, but with stride 3 and a populated Vec3 spline payload.
#[test]
fn resolve_color_keys_at_samples_bspline_comp_point3() {
    use crate::blocks::interpolator::{
        NiBSplineBasisData, NiBSplineCompPoint3Interpolator, NiBSplineData,
    };
    use crate::types::BlockRef;

    // 4 CPs × stride 3 = 12 i16 slots. Pack [r,g,b] tuples
    // [(0,0,0), (32767,32767,32767), (-32767,-32767,-32767), (0,0,0)]
    // with offset=0.5, half_range=0.5 → [0.5; 3], [1; 3], [0; 3], [0.5; 3].
    let mut cps: Vec<i16> = Vec::with_capacity(12);
    cps.extend([0, 0, 0]);
    cps.extend([32767, 32767, 32767]);
    cps.extend([-32767, -32767, -32767]);
    cps.extend([0, 0, 0]);

    let data = NiBSplineData {
        float_control_points: Vec::new(),
        compact_control_points: cps,
    };
    let basis = NiBSplineBasisData {
        num_control_points: 4,
    };
    let interp = NiBSplineCompPoint3Interpolator {
        start_time: 0.0,
        stop_time: 1.0,
        spline_data_ref: BlockRef(0),
        basis_data_ref: BlockRef(1),
        value: [0.0, 0.0, 0.0],
        handle: 0,
        position_offset: 0.5,
        position_half_range: 0.5,
    };
    let scene = NifScene {
        blocks: vec![Box::new(data), Box::new(basis), Box::new(interp)],
        ..NifScene::default()
    };

    let keys = resolve_color_keys_at(&scene, 2);
    assert!(
        keys.len() >= 2,
        "BSpline-comp Point3 must surface sampled color keys, got {}",
        keys.len()
    );
    let first = keys.first().unwrap();
    let last = keys.last().unwrap();
    // u=0 → CP[0] = [0.5; 3]; u=1 → CP[3] = [0.5; 3] (open-uniform
    // clamps at both endpoints).
    for &c in &first.value {
        assert!((c - 0.5).abs() < 1e-3, "first key channel = 0.5, got {c}");
    }
    for &c in &last.value {
        assert!((c - 0.5).abs() < 1e-3, "last key channel = 0.5, got {c}");
    }
}

/// Regression: #983. A `NiPointLight` with all four `NiLight*Controller`
/// types chained off its `controller_ref` must surface as an
/// `AnimationClip` carrying:
///   - one `ColorTarget::LightDiffuse` channel (NiLightColorController,
///     target_color=0)
///   - one `FloatTarget::LightDimmer` channel (NiLightDimmerController)
///   - one `FloatTarget::LightIntensity` channel (NiLightIntensityController)
///   - one `FloatTarget::LightRadius` channel (NiLightRadiusController)
///
/// All four channels are keyed by the NiPointLight's NiObjectNET name
/// (`"Torch01"`) so the runtime animation system writes into the
/// matching `LightSource` ECS entity. Pre-fix the four controller
/// dispatch arms were missing entirely and lanterns/campfires/plasma
/// weapons emitted constant light.
#[test]
fn import_embedded_animations_captures_nilight_controllers() {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::controller::{
        NiLightColorController, NiLightFloatController, NiSingleInterpController,
        NiTimeControllerBase,
    };
    use crate::blocks::interpolator::{
        FloatKey, KeyGroup, KeyType, NiPoint3Interpolator, NiPosData, Vec3Key,
    };
    use crate::blocks::light::{NiLightBase, NiPointLight};
    use crate::types::{BlockRef, NiColor, NiTransform};
    use std::sync::Arc;

    // Block layout:
    //   [0] NiFloatData (dimmer keys 0→1 over 1s)
    //   [1] NiFloatInterpolator → [0]
    //   [2] NiFloatData (intensity keys 0→2)
    //   [3] NiFloatInterpolator → [2]
    //   [4] NiFloatData (radius keys 100→200)
    //   [5] NiFloatInterpolator → [4]
    //   [6] NiPosData ([1,0,0] → [0,1,0])
    //   [7] NiPoint3Interpolator → [6]
    //   [8] NiLightColorController → interp [7] (Diffuse, target_color=0)
    //   [9] NiLightFloatController("NiLightRadiusController") → interp [5], next=[8]
    //  [10] NiLightFloatController("NiLightIntensityController") → interp [3], next=[9]
    //  [11] NiLightFloatController("NiLightDimmerController") → interp [1], next=[10]
    //  [12] NiPointLight (name="Torch01") with controller_ref=[11]
    fn float_data(v0: f32, v1: f32) -> NiFloatData {
        NiFloatData {
            keys: KeyGroup {
                key_type: KeyType::Linear,
                keys: vec![
                    FloatKey {
                        time: 0.0,
                        value: v0,
                        tangent_forward: 0.0,
                        tangent_backward: 0.0,
                        tbc: None,
                    },
                    FloatKey {
                        time: 1.0,
                        value: v1,
                        tangent_forward: 0.0,
                        tangent_backward: 0.0,
                        tbc: None,
                    },
                ],
            },
        }
    }
    fn float_interp(data_idx: u32) -> NiFloatInterpolator {
        NiFloatInterpolator {
            value: 0.0,
            data_ref: BlockRef(data_idx),
        }
    }
    let tc_base = |next: BlockRef| NiTimeControllerBase {
        next_controller_ref: next,
        flags: 0,
        frequency: 1.0,
        phase: 0.0,
        start_time: 0.0,
        stop_time: 1.0,
        target_ref: BlockRef::NULL,
    };
    let single_interp = |next: BlockRef, interp: u32| NiSingleInterpController {
        base: tc_base(next),
        interpolator_ref: BlockRef(interp),
    };

    let pos_data = NiPosData {
        keys: KeyGroup {
            key_type: KeyType::Linear,
            keys: vec![
                Vec3Key {
                    time: 0.0,
                    value: [1.0, 0.0, 0.0],
                    tangent_forward: [0.0; 3],
                    tangent_backward: [0.0; 3],
                    tbc: None,
                },
                Vec3Key {
                    time: 1.0,
                    value: [0.0, 1.0, 0.0],
                    tangent_forward: [0.0; 3],
                    tangent_backward: [0.0; 3],
                    tbc: None,
                },
            ],
        },
    };
    let p3_interp = NiPoint3Interpolator {
        value: [0.0; 3],
        data_ref: BlockRef(6),
    };
    let color_ctrl = NiLightColorController {
        base: tc_base(BlockRef::NULL), // tail of chain
        interpolator_ref: BlockRef(7),
        target_color: 0, // Diffuse
    };
    let radius_ctrl = NiLightFloatController {
        type_name: "NiLightRadiusController",
        base: single_interp(BlockRef(8), 5),
    };
    let intensity_ctrl = NiLightFloatController {
        type_name: "NiLightIntensityController",
        base: single_interp(BlockRef(9), 3),
    };
    let dimmer_ctrl = NiLightFloatController {
        type_name: "NiLightDimmerController",
        base: single_interp(BlockRef(10), 1),
    };

    let light = NiPointLight {
        base: NiLightBase {
            av: NiAVObjectData {
                net: NiObjectNETData {
                    name: Some(Arc::from("Torch01")),
                    extra_data_refs: Vec::new(),
                    controller_ref: BlockRef(11_u32), // head = dimmer_ctrl
                },
                flags: 0,
                transform: NiTransform::default(),
                properties: Vec::new(),
                collision_ref: BlockRef::NULL,
            },
            switch_state: true,
            affected_nodes: Vec::new(),
            dimmer: 1.0,
            ambient_color: NiColor {
                r: 0.0,
                g: 0.0,
                b: 0.0,
            },
            diffuse_color: NiColor {
                r: 1.0,
                g: 1.0,
                b: 1.0,
            },
            specular_color: NiColor {
                r: 0.0,
                g: 0.0,
                b: 0.0,
            },
        },
        constant_attenuation: 1.0,
        linear_attenuation: 0.0,
        quadratic_attenuation: 0.0,
    };

    let scene = NifScene {
        blocks: vec![
            Box::new(float_data(0.0, 1.0)),  // [0] dimmer data
            Box::new(float_interp(0)),       // [1] dimmer interp
            Box::new(float_data(0.0, 2.0)),  // [2] intensity data
            Box::new(float_interp(2)),       // [3] intensity interp
            Box::new(float_data(100.0, 200.0)), // [4] radius data
            Box::new(float_interp(4)),       // [5] radius interp
            Box::new(pos_data),              // [6] color data
            Box::new(p3_interp),             // [7] color interp
            Box::new(color_ctrl),            // [8] color ctrl
            Box::new(radius_ctrl),           // [9] radius ctrl
            Box::new(intensity_ctrl),        // [10] intensity ctrl
            Box::new(dimmer_ctrl),           // [11] dimmer ctrl (chain head)
            Box::new(light),                 // [12] NiPointLight
        ],
        ..NifScene::default()
    };

    let clip = import_embedded_animations(&scene).expect("expected embedded clip");
    // Three float channels (Dimmer + Intensity + Radius) + one color
    // channel (LightDiffuse). Order doesn't matter — we assert by
    // target and node name.
    assert_eq!(
        clip.float_channels.len(),
        3,
        "expected 3 NiLightFloatController channels"
    );
    assert_eq!(
        clip.color_channels.len(),
        1,
        "expected 1 NiLightColorController channel"
    );
    let mut seen_targets = std::collections::HashSet::new();
    for (name, ch) in &clip.float_channels {
        assert_eq!(&**name, "Torch01");
        seen_targets.insert(ch.target);
    }
    assert!(seen_targets.contains(&FloatTarget::LightDimmer));
    assert!(seen_targets.contains(&FloatTarget::LightIntensity));
    assert!(seen_targets.contains(&FloatTarget::LightRadius));

    let (cname, cch) = &clip.color_channels[0];
    assert_eq!(&**cname, "Torch01");
    assert_eq!(cch.target, ColorTarget::LightDiffuse);
    assert_eq!(cch.keys.len(), 2);
}
