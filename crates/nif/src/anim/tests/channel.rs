//! Float / colour / bool / texture-transform channel extraction and the
//! embedded-animation entry point (`anim/channel.rs`, `anim/entry.rs`).

use super::super::*;

use crate::blocks::interpolator::{NiFloatData, NiFloatInterpolator};
use crate::scene::NifScene;

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
        data_ref: BlockRef::NULL,
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

/// Regression: #3097. An authored `NiTimeControllerBase` envelope
/// (non-default cycle type, frequency, phase, timing) must reach the
/// merged embedded clip instead of the old hardcoded
/// `Loop` / `1.0` / `duration 0.0`. Same scene shape as the test
/// above, but `flags` encodes `CYCLE_REVERSE` (nif.xml raw value 1,
/// bits 1-2 of the bitfield) alongside unrelated bits 0 and 3 set —
/// proving the decode masks precisely, not by accident of a
/// convenient flags value.
#[test]
fn import_embedded_animations_carries_authored_timing_envelope() {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::controller::{NiTextureTransformController, NiTimeControllerBase};
    use crate::blocks::interpolator::{FloatKey, KeyGroup, KeyType};
    use crate::blocks::node::NiNode;
    use crate::types::{BlockRef, NiTransform};
    use std::sync::Arc;

    let data = NiFloatData {
        keys: KeyGroup {
            key_type: KeyType::Linear,
            keys: vec![FloatKey {
                time: 0.0,
                value: 0.0,
                tangent_forward: 0.0,
                tangent_backward: 0.0,
                tbc: None,
            }],
        },
    };
    let interp = NiFloatInterpolator {
        value: 0.0,
        data_ref: BlockRef(0),
    };
    let ctrl = NiTextureTransformController {
        base: NiTimeControllerBase {
            next_controller_ref: BlockRef::NULL,
            // bits: 0=AnimType(1), 1-2=CycleType(0b01=CYCLE_REVERSE), 3=Active(1)
            flags: 0b0000_1011,
            frequency: 2.5,
            phase: 0.75,
            start_time: 1.0,
            stop_time: 3.0,
            target_ref: BlockRef::NULL,
        },
        interpolator_ref: BlockRef(1),
        shader_map: false,
        texture_slot: 0,
        operation: 0,
        data_ref: BlockRef::NULL,
    };
    let node = NiNode {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(Arc::from("LavaFlow")),
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
    assert_eq!(clip.cycle_type, CycleType::Reverse);
    assert!((clip.frequency - 2.5).abs() < 1e-6);
    assert!((clip.phase - 0.75).abs() < 1e-6);
    // `duration` deliberately does NOT come from the envelope's
    // stop_time(3.0) - start_time(1.0) — it's the real max key time
    // across channels, which stays the existing constant-key fallback
    // (1.0) here since this fixture's lone key sits at time 0.0.
    assert!(
        (clip.duration - 1.0).abs() < 1e-6,
        "duration must stay derived from channel key times, not the envelope; got {}",
        clip.duration
    );
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
        accum_time: 0.0,
        delta: 0.0,
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
        data_ref: BlockRef::NULL,
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
            Box::new(float_data(0.0, 1.0)),     // [0] dimmer data
            Box::new(float_interp(0)),          // [1] dimmer interp
            Box::new(float_data(0.0, 2.0)),     // [2] intensity data
            Box::new(float_interp(2)),          // [3] intensity interp
            Box::new(float_data(100.0, 200.0)), // [4] radius data
            Box::new(float_interp(4)),          // [5] radius interp
            Box::new(pos_data),                 // [6] color data
            Box::new(p3_interp),                // [7] color interp
            Box::new(color_ctrl),               // [8] color ctrl
            Box::new(radius_ctrl),              // [9] radius ctrl
            Box::new(intensity_ctrl),           // [10] intensity ctrl
            Box::new(dimmer_ctrl),              // [11] dimmer ctrl (chain head)
            Box::new(light),                    // [12] NiPointLight
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

/// Regression: #1440 (LC-D5-01). An inline transform controller hung
/// directly off a node — no `NiControllerSequence` — must surface as a
/// looping embedded `AnimationClip` carrying a transform channel keyed by
/// the node name. Animated scenery (fans, doors, lifts, swinging signs in
/// loose Oblivion/FO3/FNV `.nif`s) relies on this. The controller parses
/// into a bare `NiSingleInterpController` whose `block_type_name()` erases
/// the `NiTransformController` / `NiKeyframeController` RTTI, so the
/// embedded dispatch must discriminate on the interpolator type, not the
/// class string. Pre-fix the controller walked into `_ => debug!("Skipping
/// unsupported embedded controller type")` and the mesh rendered static.
#[test]
fn import_embedded_animations_captures_inline_transform_controller() {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::controller::{NiSingleInterpController, NiTimeControllerBase};
    use crate::blocks::interpolator::{
        KeyGroup, KeyType, NiTransformData, NiTransformInterpolator, Vec3Key,
    };
    use crate::blocks::node::NiNode;
    use crate::types::{BlockRef, NiQuatTransform, NiTransform};
    use std::sync::Arc;

    // Scene layout:
    //   [0] NiTransformData (two linear translation keys over 4 s)
    //   [1] NiTransformInterpolator → data=[0]
    //   [2] NiSingleInterpController → interp=[1]  (the parsed form of
    //       NiTransformController / NiKeyframeController — RTTI erased)
    //   [3] NiNode (name="Fan01") with controller_ref=[2]
    let data = NiTransformData {
        rotation_type: None,
        rotation_keys: Vec::new(),
        xyz_rotations: None,
        translations: KeyGroup {
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
                    time: 4.0,
                    value: [0.0, 0.0, 10.0],
                    tangent_forward: [0.0; 3],
                    tangent_backward: [0.0; 3],
                    tbc: None,
                },
            ],
        },
        scales: KeyGroup {
            key_type: KeyType::Linear,
            keys: Vec::new(),
        },
    };
    let interp = NiTransformInterpolator {
        transform: NiQuatTransform::default(),
        data_ref: BlockRef(0),
    };
    let ctrl = NiSingleInterpController {
        base: NiTimeControllerBase {
            next_controller_ref: BlockRef::NULL,
            flags: 0,
            frequency: 1.0,
            phase: 0.0,
            start_time: 0.0,
            stop_time: 4.0,
            target_ref: BlockRef::NULL,
        },
        interpolator_ref: BlockRef(1),
    };
    let node = NiNode {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(Arc::from("Fan01")),
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

    let clip = import_embedded_animations(&scene)
        .expect("inline transform controller should produce an embedded clip");
    assert_eq!(clip.cycle_type, CycleType::Loop);
    assert_eq!(
        clip.channels.len(),
        1,
        "exactly one transform channel expected, keyed by node name"
    );
    let channel = clip
        .channels
        .get(&Arc::<str>::from("Fan01"))
        .expect("transform channel must be keyed by the node name 'Fan01'");
    assert_eq!(
        channel.translation_keys.len(),
        2,
        "both authored translation keys must survive import"
    );
    assert!(
        (clip.duration - 4.0).abs() < 1e-6,
        "duration follows the last transform key time"
    );
}

/// #2304 / NIFAL-D7-03 — `float_target_from_operation` is the single
/// source of truth for `NiTextureTransformController.operation` ->
/// `FloatTarget`, shared by the KF (`extract_texture_transform_channel`)
/// and embedded (`entry.rs`) controller-import arms. Pin every discriminant
/// (including the out-of-range fallback) so the two arms can't silently
/// diverge if a new operation value is ever added to only one of them.
#[test]
fn float_target_from_operation_covers_every_discriminant() {
    assert_eq!(float_target_from_operation(0), FloatTarget::UvOffsetU);
    assert_eq!(float_target_from_operation(1), FloatTarget::UvOffsetV);
    assert_eq!(float_target_from_operation(2), FloatTarget::UvScaleU);
    assert_eq!(float_target_from_operation(3), FloatTarget::UvScaleV);
    assert_eq!(float_target_from_operation(4), FloatTarget::UvRotation);
    assert_eq!(
        float_target_from_operation(5),
        FloatTarget::UvOffsetU,
        "unrecognized operation values fall back to UvOffsetU"
    );
}

/// #2304 / NIFAL-D7-03 — `color_target_from_target_color` is the single
/// source of truth for `NiMaterialColorController.target_color` ->
/// `ColorTarget`, shared by the KF (`extract_color_channel`) and embedded
/// (`entry.rs`) controller-import arms.
#[test]
fn color_target_from_target_color_covers_every_discriminant() {
    assert_eq!(color_target_from_target_color(0), ColorTarget::Diffuse);
    assert_eq!(color_target_from_target_color(1), ColorTarget::Ambient);
    assert_eq!(color_target_from_target_color(2), ColorTarget::Specular);
    assert_eq!(color_target_from_target_color(3), ColorTarget::Emissive);
    assert_eq!(
        color_target_from_target_color(4),
        ColorTarget::Diffuse,
        "unrecognized target_color values fall back to Diffuse"
    );
}
