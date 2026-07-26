//! #1443 — mainline keyframe-stream finite / FLT_MAX sanitizer.

use super::super::*;

use crate::blocks::interpolator::{
    FloatKey, KeyGroup, KeyType, NiFloatData, NiFloatInterpolator, NiTransformData, Vec3Key,
};
use crate::scene::NifScene;

// ── #1443 — mainline keyframe-stream finite/FLT_MAX sanitizer ────────
//
// The static-pose / B-spline paths already gate on `is_flt_max`; these
// cover the previously-unguarded mainline converters (`convert_vec3_keys`,
// `convert_quat_keys`, `convert_float_keys`, and the float/color channel
// extractors). A corrupt value must be dropped before it reaches the
// sampler and poisons a bone/shader uniform with NaN.
#[cfg(test)]
mod sanitize_keyframe_streams {
    use super::*;
    use crate::blocks::interpolator::{Color4Key, NiColorData, NiColorInterpolator, QuatKey};
    use crate::types::BlockRef;

    fn all_finite3(v: [f32; 3]) -> bool {
        v.iter().all(|c| c.is_finite())
    }

    fn vec3_key(time: f32, value: [f32; 3]) -> Vec3Key {
        Vec3Key {
            time,
            value,
            tangent_forward: [0.0; 3],
            tangent_backward: [0.0; 3],
            tbc: None,
        }
    }

    #[test]
    fn convert_vec3_keys_drops_non_finite_and_flt_max() {
        let group = KeyGroup {
            key_type: KeyType::Linear,
            keys: vec![
                vec3_key(0.0, [1.0, 2.0, 3.0]),
                vec3_key(1.0, [f32::NAN, 0.0, 0.0]),
                vec3_key(2.0, [0.0, f32::INFINITY, 0.0]),
                vec3_key(3.0, [3.0e38, 0.0, 0.0]), // FLT_MAX sentinel
                vec3_key(4.0, [4.0, 5.0, 6.0]),
            ],
        };
        let (keys, _) = convert_vec3_keys(&group);
        assert_eq!(keys.len(), 2, "only the two clean keys survive");
        assert!(keys.iter().all(|k| all_finite3(k.value)));
        assert_eq!(keys[0].time, 0.0);
        assert_eq!(keys[1].time, 4.0);
    }

    #[test]
    fn convert_vec3_keys_clamps_bad_tangents_without_dropping_key() {
        let group = KeyGroup {
            key_type: KeyType::Quadratic,
            keys: vec![Vec3Key {
                time: 0.0,
                value: [1.0, 1.0, 1.0],
                tangent_forward: [f32::NAN, 1.0, 1.0],
                tangent_backward: [1.0, f32::INFINITY, 1.0],
                tbc: None,
            }],
        };
        let (keys, _) = convert_vec3_keys(&group);
        assert_eq!(
            keys.len(),
            1,
            "clean value keeps the key despite bad tangents"
        );
        assert!(all_finite3(keys[0].forward), "forward tangent sanitized");
        assert!(all_finite3(keys[0].backward), "backward tangent sanitized");
    }

    #[test]
    fn convert_quat_keys_drops_non_finite_quaternion() {
        let data = NiTransformData {
            rotation_type: Some(KeyType::Linear),
            rotation_keys: vec![
                QuatKey {
                    time: 0.0,
                    value: [1.0, 0.0, 0.0, 0.0],
                    tbc: None,
                },
                QuatKey {
                    time: 1.0,
                    value: [f32::NAN, 0.0, 0.0, 0.0],
                    tbc: None,
                },
                QuatKey {
                    time: 2.0,
                    value: [3.0e38, 0.0, 0.0, 0.0], // FLT_MAX sentinel
                    tbc: None,
                },
            ],
            xyz_rotations: None,
            translations: KeyGroup {
                key_type: KeyType::Linear,
                keys: vec![],
            },
            scales: KeyGroup {
                key_type: KeyType::Linear,
                keys: vec![],
            },
        };
        let (keys, _) = convert_quat_keys(&data);
        assert_eq!(keys.len(), 1, "only the clean identity quaternion survives");
        assert_eq!(keys[0].time, 0.0);
    }

    #[test]
    fn convert_float_keys_drops_bad_values_and_clamps_tangents() {
        let group = KeyGroup {
            key_type: KeyType::Quadratic,
            keys: vec![
                FloatKey {
                    time: 0.0,
                    value: 1.0,
                    tangent_forward: f32::NAN,
                    tangent_backward: 0.0,
                    tbc: None,
                },
                FloatKey {
                    time: 1.0,
                    value: f32::INFINITY,
                    tangent_forward: 0.0,
                    tangent_backward: 0.0,
                    tbc: None,
                },
                FloatKey {
                    time: 2.0,
                    value: 3.0e38, // FLT_MAX sentinel
                    tangent_forward: 0.0,
                    tangent_backward: 0.0,
                    tbc: None,
                },
            ],
        };
        let (keys, _) = convert_float_keys(&group);
        assert_eq!(
            keys.len(),
            1,
            "only the finite, sub-sentinel value survives"
        );
        assert_eq!(keys[0].value, 1.0);
        assert!(keys[0].forward.is_finite(), "NaN tangent clamped to 0");
        assert_eq!(keys[0].forward, 0.0);
    }

    #[test]
    fn extract_float_channel_drops_non_finite_values() {
        let data = NiFloatData {
            keys: KeyGroup {
                key_type: KeyType::Linear,
                keys: vec![
                    FloatKey {
                        time: 0.0,
                        value: 0.5,
                        tangent_forward: 0.0,
                        tangent_backward: 0.0,
                        tbc: None,
                    },
                    FloatKey {
                        time: 1.0,
                        value: f32::NAN,
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
        let scene = NifScene {
            blocks: vec![Box::new(data), Box::new(interp)],
            ..NifScene::default()
        };
        let channel =
            extract_float_channel_at(&scene, 1, FloatTarget::Alpha).expect("one clean key remains");
        assert_eq!(channel.keys.len(), 1);
        assert_eq!(channel.keys[0].value, 0.5);
    }

    #[test]
    fn resolve_color_keys_drops_non_finite_rgb() {
        let data = NiColorData {
            keys: KeyGroup {
                key_type: KeyType::Linear,
                keys: vec![
                    Color4Key {
                        time: 0.0,
                        value: [0.1, 0.2, 0.3, 1.0],
                        tangent_forward: [0.0; 4],
                        tangent_backward: [0.0; 4],
                        tbc: None,
                    },
                    Color4Key {
                        time: 1.0,
                        value: [f32::NAN, 0.2, 0.3, 1.0],
                        tangent_forward: [0.0; 4],
                        tangent_backward: [0.0; 4],
                        tbc: None,
                    },
                ],
            },
        };
        let interp = NiColorInterpolator {
            value: [0.0; 4],
            data_ref: BlockRef(0),
        };
        let scene = NifScene {
            blocks: vec![Box::new(data), Box::new(interp)],
            ..NifScene::default()
        };
        let keys = resolve_color_keys_at(&scene, 1);
        assert_eq!(keys.len(), 1, "only the clean RGB key survives");
        assert_eq!(keys[0].value, [0.1, 0.2, 0.3]);
    }
}
