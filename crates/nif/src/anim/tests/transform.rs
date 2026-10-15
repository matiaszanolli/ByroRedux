//! Transform-channel extraction and blend-interpolator resolution
//! (`anim/transform.rs`).

use super::super::*;

use crate::blocks::controller::{NiGeomMorpherController, NiMorphData};
use crate::blocks::interpolator::{
    FloatKey, KeyGroup, KeyType, NiLookAtInterpolator, NiPathInterpolator, NiPosData, Vec3Key,
};
use crate::scene::NifScene;
use byroredux_core::animation::types::KeyType as CoreKeyType;
use byroredux_core::math::Vec3;
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

    let channel = extract_transform_channel(&scene, &cb, &mut super::super::bspline::BsplineSampling::new())
        .expect("blend transform interpolator must resolve to the dominant sub-interp");
    assert_eq!(
        channel.scale_keys.len(),
        1,
        "must reach dominant data's scales"
    );
    assert!((channel.scale_keys[0].value - 1.5).abs() < 1e-6);
}

/// #3316 / FNV-D6-01 — a `NiTransformInterpolator` whose `data_ref` is null
/// carries its pose in the interpolator's own `transform` field (nif.xml
/// documents the sibling `NiPoint3Interpolator.Value` as "Pose value if
/// lacking NiPosData"; Gamebryo v2.6/v3.2 call it "an unchanging pose").
/// Pre-fix the `data_ref.index()?` early-return dropped it: 39.7% of every
/// FNV transform controlled block, and 64 vanilla `.kf` files imported as
/// zero clips.
#[test]
fn extract_transform_channel_emits_constant_pose_for_null_transform_data() {
    use crate::blocks::interpolator::NiTransformInterpolator;
    use crate::types::{BlockRef, NiPoint3, NiQuatTransform};

    let half = std::f32::consts::FRAC_1_SQRT_2;
    let pose = NiQuatTransform {
        translation: NiPoint3 {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        },
        rotation: [half, 0.0, 0.0, half],
        scale: 0.75,
    };
    let interp = NiTransformInterpolator {
        transform: pose,
        data_ref: BlockRef::NULL,
    };
    let scene = NifScene {
        blocks: vec![Box::new(interp)],
        ..NifScene::default()
    };

    let mut cb = dummy_controlled_block();
    cb.interpolator_ref = BlockRef(0);

    let channel = extract_transform_channel(&scene, &cb, &mut super::super::bspline::BsplineSampling::new())
        .expect("a posed NiTransformInterpolator must not drop the channel");
    assert_eq!(channel.translation_keys.len(), 1);
    assert_eq!(channel.rotation_keys.len(), 1);
    assert_eq!(channel.scale_keys.len(), 1);

    // Same Z-up → Y-up conversion the look-at pose gets: (1, 2, 3) → (1, 3, -2).
    let t = channel.translation_keys[0].value;
    assert!((t.x - 1.0).abs() < 1e-6);
    assert!((t.y - 3.0).abs() < 1e-6);
    assert!((t.z + 2.0).abs() < 1e-6);

    let r = channel.rotation_keys[0].value;
    assert!(r.x.abs() < 1e-6);
    assert!((r.y - half).abs() < 1e-6);
    assert!(r.z.abs() < 1e-6);
    assert!((r.w - half).abs() < 1e-6);

    assert!((channel.scale_keys[0].value - 0.75).abs() < 1e-6);
}

/// The FLT_MAX per-axis sentinel must still mean "no pose for this axis"
/// on the null-data fallback, exactly as it does for look-at.
#[test]
fn posed_transform_interpolator_honours_flt_max_sentinel() {
    use crate::blocks::interpolator::NiTransformInterpolator;
    use crate::types::{BlockRef, NiPoint3, NiQuatTransform};

    let interp = NiTransformInterpolator {
        transform: NiQuatTransform {
            translation: NiPoint3 {
                x: f32::MAX,
                y: f32::MAX,
                z: f32::MAX,
            },
            rotation: [1.0, 0.0, 0.0, 0.0],
            scale: f32::MAX,
        },
        data_ref: BlockRef::NULL,
    };
    let scene = NifScene {
        blocks: vec![Box::new(interp)],
        ..NifScene::default()
    };
    let mut cb = dummy_controlled_block();
    cb.interpolator_ref = BlockRef(0);

    let channel = extract_transform_channel(&scene, &cb, &mut super::super::bspline::BsplineSampling::new()).expect("channel still emitted");
    assert!(
        channel.translation_keys.is_empty(),
        "FLT_MAX translation means no authored pose on that axis"
    );
    assert!(
        channel.scale_keys.is_empty(),
        "FLT_MAX scale means no authored pose on that axis"
    );
    assert_eq!(channel.rotation_keys.len(), 1, "rotation was authored");
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

    let channel = extract_transform_channel(&scene, &cb, &mut super::super::bspline::BsplineSampling::new())
        .expect("NiLookAtInterpolator must emit a constant transform channel");
    assert_eq!(channel.translation_keys.len(), 1);
    assert_eq!(channel.rotation_keys.len(), 1);
    assert_eq!(channel.scale_keys.len(), 1);

    // Translation Z-up → Y-up: (1, 2, 3) → (1, 3, -2).
    let t = channel.translation_keys[0].value;
    assert!((t.x - 1.0).abs() < 1e-6);
    assert!((t.y - 3.0).abs() < 1e-6);
    assert!((t.z + 2.0).abs() < 1e-6);

    // Rotation: Z-up (w,x,y,z) = (√2/2, 0, 0, √2/2) → glam (x,y,z,w)
    // via zup_to_yup_quat = (0, √2/2, 0, √2/2).
    let r = channel.rotation_keys[0].value;
    assert!(r.x.abs() < 1e-6);
    assert!((r.y - half).abs() < 1e-6);
    assert!(r.z.abs() < 1e-6);
    assert!((r.w - half).abs() < 1e-6);

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

    let channel = extract_transform_channel(&scene, &cb, &mut super::super::bspline::BsplineSampling::new())
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

/// #4396 / #4397 (NIFAL-D7-2026-09-14-01/-02) — the NaN and zero-quat
/// classes the FLT_MAX pins above could not see: `is_flt_max(NaN)` is
/// false, so a NaN pose component passed every static-pose gate and rode
/// the swizzle into a canonical key (NaN translation verbatim, rotation
/// normalizing to all-NaN, scale copied), and a rotation that squares
/// past f32::MAX became the zero quaternion through `normalize_quat`.
/// The gates now read `is_key_value_sane` and the rotation normalizes
/// through the shared sanitizer, so every class here drops to an empty
/// key list — bind pose — while clean axes stay per-axis independent.
#[test]
fn constant_transform_channel_drops_nan_and_overflow_pose_components() {
    use crate::types::{NiPoint3, NiQuatTransform};

    // NaN on every axis — invisible to the old is_flt_max gates.
    let nan_pose = NiQuatTransform {
        translation: NiPoint3 {
            x: f32::NAN,
            y: f32::NAN,
            z: f32::NAN,
        },
        rotation: [f32::NAN, f32::NAN, f32::NAN, f32::NAN],
        scale: f32::NAN,
    };
    let channel = constant_transform_channel(&nan_pose);
    assert!(channel.translation_keys.is_empty(), "NaN translation drops");
    assert!(channel.rotation_keys.is_empty(), "NaN rotation drops");
    assert!(channel.scale_keys.is_empty(), "NaN scale drops");

    // Individually-sane rotation that squares to inf (→ zero quat), with
    // clean translation/scale on the same pose: only rotation drops.
    let overflow_rot = NiQuatTransform {
        translation: NiPoint3 {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        },
        rotation: [2.0e19, 0.0, 0.0, 0.0],
        scale: 0.75,
    };
    let channel = constant_transform_channel(&overflow_rot);
    assert_eq!(channel.translation_keys.len(), 1, "clean translation survives");
    assert!(
        channel.rotation_keys.is_empty(),
        "a rotation that squares past f32::MAX must skip, not arrive as the \
         zero quaternion (#4396)"
    );
    assert_eq!(channel.scale_keys.len(), 1);

    // An authored all-zero quaternion is malformed, not a pose.
    let zero_rot = NiQuatTransform {
        translation: NiPoint3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        rotation: [0.0, 0.0, 0.0, 0.0],
        scale: 1.0,
    };
    let channel = constant_transform_channel(&zero_rot);
    assert!(
        channel.rotation_keys.is_empty(),
        "an all-zero authored quaternion must skip (#4396)"
    );
    assert_eq!(channel.translation_keys.len(), 1);
    assert_eq!(channel.scale_keys.len(), 1);

    // A non-unit but well-scaled pose quaternion now arrives normalized
    // (pre-#4396 it was stored raw and relied on the sampler's lazy
    // normalize): (w,x,y,z) = (0,3,4,0) → glam (x,z,-y,w) = (0.6,0,-0.8,0).
    let nonunit = NiQuatTransform {
        translation: NiPoint3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        rotation: [0.0, 3.0, 4.0, 0.0],
        scale: 1.0,
    };
    let channel = constant_transform_channel(&nonunit);
    assert_eq!(channel.rotation_keys.len(), 1);
    let r = channel.rotation_keys[0].value;
    assert!((r.x - 0.6).abs() < 1e-5 && r.y.abs() < 1e-5 && (r.z + 0.8).abs() < 1e-5);
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

    let channel = extract_transform_channel(&scene, &cb, &mut super::super::bspline::BsplineSampling::new()).expect("mixed pose channel");
    assert!(channel.translation_keys.is_empty());
    assert_eq!(channel.rotation_keys.len(), 1);
    assert_eq!(channel.scale_keys.len(), 1);
    assert!((channel.scale_keys[0].value - 1.0).abs() < 1e-6);
}

/// Build the scene for a path interpolator: block 0 = NiPosData (a three-point
/// Z-up arch over path-time 0..1), block 1 = NiFloatData percent curve,
/// block 2 = the interpolator. `pct` is `(time, percent)` pairs.
fn path_scene(pct: Option<&[(f32, f32)]>) -> NifScene {
    use crate::blocks::interpolator::{NiFloatData, Vec3Key};
    use crate::types::BlockRef;

    let pt = |time: f32, value: [f32; 3]| Vec3Key {
        time,
        value,
        tangent_forward: [0.0; 3],
        tangent_backward: [0.0; 3],
        tbc: None,
    };
    let pos_data = NiPosData {
        keys: KeyGroup::<Vec3Key> {
            key_type: KeyType::Linear,
            keys: vec![
                pt(0.0, [0.0, 0.0, 0.0]),
                pt(0.5, [10.0, 0.0, 5.0]),
                pt(1.0, [20.0, 0.0, 0.0]),
            ],
        },
    };
    let pct_data = NiFloatData {
        keys: KeyGroup::<FloatKey> {
            key_type: KeyType::Linear,
            keys: pct
                .unwrap_or(&[])
                .iter()
                .map(|&(time, value)| FloatKey {
                    time,
                    value,
                    tangent_forward: 0.0,
                    tangent_backward: 0.0,
                    tbc: None,
                })
                .collect(),
        },
    };
    let path_interp = NiPathInterpolator {
        flags: 3,
        bank_dir: 1,
        max_bank_angle: 0.0,
        smoothing: 0.0,
        follow_axis: 0,
        path_data_ref: BlockRef(0),
        percent_data_ref: if pct.is_some() { BlockRef(1) } else { BlockRef::NULL },
    };
    NifScene {
        blocks: vec![Box::new(pos_data), Box::new(pct_data), Box::new(path_interp)],
        ..NifScene::default()
    }
}

fn path_channel(
    scene: &NifScene,
    sampling: &mut super::super::bspline::BsplineSampling,
) -> Option<TransformChannel> {
    let mut cb = dummy_controlled_block();
    cb.interpolator_ref = crate::types::BlockRef(2);
    extract_transform_channel(scene, &cb, sampling)
}

fn value_at(ch: &TransformChannel, t: f32) -> byroredux_core::math::Vec3 {
    ch.translation_keys
        .iter()
        .min_by(|a, b| (a.time - t).abs().total_cmp(&(b.time - t).abs()))
        .map(|k| k.value)
        .unwrap()
}

/// #605 / #5571 — NiPathInterpolator composes its percent curve with its
/// path spline: position(t) = path(percent(t)). Path keys are a parametric
/// curve over 0..1, NOT animation time; the percent keys carry the timing.
/// Pre-#5571 the path keys were emitted verbatim as translation-over-time,
/// so a percent curve spanning 33 s played in 1 s and the percent data was
/// ignored. Output is Z-up → Y-up converted ((x, y, z) → (x, z, -y)), with
/// rotation / scale left empty so the node keeps its bind values.
#[test]
fn extract_transform_channel_composes_percent_curve_with_path() {
    // Percent 0→1 over 4 seconds.
    let scene = path_scene(Some(&[(0.0, 0.0), (4.0, 1.0)]));
    let ch = path_channel(&scene, &mut super::super::bspline::BsplineSampling::new())
        .expect("path + percent data must emit a translation channel");

    let first = ch.translation_keys.first().unwrap();
    let last = ch.translation_keys.last().unwrap();
    assert_eq!((first.time, first.value), (0.0, Vec3::ZERO));
    assert_eq!(last.time, 4.0, "key times are animation time (percent keys), not path time");
    assert_eq!(last.value, Vec3::new(20.0, 0.0, 0.0));
    // Every key is path(percent(t)): percent = t / 4, and the arch is
    // x = 20·p, y(Y-up, = Z-up z) = 5·(1 − |2p − 1|), z(Y-up) = 0.
    assert!(ch.translation_keys.len() > 30, "resampled, not the 3 path keys");
    for k in &ch.translation_keys {
        let p = k.time / 4.0;
        let expect = Vec3::new(20.0 * p, 5.0 * (1.0 - (2.0 * p - 1.0).abs()), 0.0);
        assert!(
            (k.value - expect).abs().max_element() < 1e-3,
            "t={}: got {:?}, expected {expect:?}",
            k.time,
            k.value
        );
    }
    assert_eq!(ch.translation_type, CoreKeyType::Linear);
    assert!(ch.rotation_keys.is_empty(), "no invented identity rotation (#5571)");
    assert!(ch.scale_keys.is_empty(), "no invented identity scale (#5571)");
}

/// The salmon-effect shape from the audit: a percent curve spanning 33.3 s
/// must keep its 33.3 s timeline instead of being squeezed into the path's
/// 0..1 key range.
#[test]
fn path_timeline_follows_the_percent_keys_not_the_path_keys() {
    let scene = path_scene(Some(&[(0.0, 0.0), (33.3, 1.0)]));
    let ch = path_channel(&scene, &mut super::super::bspline::BsplineSampling::new()).unwrap();
    assert!((ch.translation_keys.last().unwrap().time - 33.3).abs() < 1e-4);
    let half = value_at(&ch, 33.3 / 2.0);
    assert!((half[0] - 10.0).abs() < 0.5, "half way through the timeline, got {half:?}");
}

/// A percentage outside the path's 0..1 key range holds the nearest end of
/// the path (the close-curve behaviour is unsourced, so nothing is invented).
#[test]
fn out_of_range_percent_holds_the_path_end() {
    let scene = path_scene(Some(&[(0.0, 1.5), (2.0, 3.0)]));
    let ch = path_channel(&scene, &mut super::super::bspline::BsplineSampling::new()).unwrap();
    for k in &ch.translation_keys {
        assert_eq!(k.value, Vec3::new(20.0, 0.0, 0.0));
    }
}

/// Without percent data there is no time→path mapping; emit nothing rather
/// than treating the 0..1 path-parameter keys as animation time.
#[test]
fn path_without_percent_data_is_skipped() {
    let scene = path_scene(None);
    assert!(path_channel(&scene, &mut super::super::bspline::BsplineSampling::new()).is_none());
}

/// The resample obeys the owning sequence's span, like B-spline channels.
#[test]
fn path_resample_is_clamped_to_the_owning_sequence() {
    let scene = path_scene(Some(&[(0.0, 0.0), (4.0, 1.0)]));
    let mut sampling = super::super::bspline::BsplineSampling::new();
    sampling.seq_span = Some((0.0, 2.0));
    let ch = path_channel(&scene, &mut sampling).unwrap();
    let last = ch.translation_keys.last().unwrap();
    assert_eq!(last.time, 2.0);
    assert!((last.value[0] - 10.0).abs() < 1e-3, "value at 2 s is still percent 0.5");
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
    assert!(extract_transform_channel(&scene, &cb, &mut super::super::bspline::BsplineSampling::new()).is_none());

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
    assert!(extract_transform_channel(&scene, &cb, &mut super::super::bspline::BsplineSampling::new()).is_none());
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
