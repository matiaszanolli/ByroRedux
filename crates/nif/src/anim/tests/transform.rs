//! Transform-channel extraction and blend-interpolator resolution
//! (`anim/transform.rs`).

use super::super::*;

use crate::blocks::controller::{NiGeomMorpherController, NiMorphData};
use crate::blocks::interpolator::{
    FloatKey, KeyGroup, KeyType, NiLookAtInterpolator, NiPathInterpolator, NiPosData, Vec3Key,
};
use crate::scene::NifScene;
use std::sync::Arc;

use super::dummy_controlled_block;

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
