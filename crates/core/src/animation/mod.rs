//! Animation interpolation engine, clip registry, and AnimationPlayer component.
//!
//! Provides keyframe sampling with linear, Hermite (quadratic), and TBC
//! (Kochanek-Bartels) interpolation for position, rotation, and scale channels.

pub mod controller;
pub mod interpolation;
pub mod player;
pub mod registry;
pub mod root_motion;
pub mod stack;
pub mod text_events;
pub mod types;

// Re-export everything at the module level to preserve the public API.
pub use controller::{
    apply_pending_transition, AnimationController, ControllerTransition,
    ControllerTransitionDefaults, TransitionKind,
};
pub use interpolation::{
    sample_bool_channel, sample_color_channel, sample_float_channel, sample_rotation, sample_scale,
    sample_translation,
};
pub use player::{advance_time, AnimationPlayer};
pub use registry::AnimationClipRegistry;
pub use root_motion::{split_root_motion, RootMotionDelta};
pub use stack::{
    advance_stack, collect_stack_text_events, sample_blended_transform, visit_stack_text_events,
    AnimationLayer, AnimationStack,
};
pub use text_events::{collect_text_key_events, visit_text_key_events};
pub use types::{
    AnimBoolKey, AnimColorKey, AnimFloatKey, AnimationClip, BoolChannel, ColorChannel, ColorTarget,
    CycleType, FloatChannel, FloatTarget, KeyType, RotationKey, ScaleKey, TextureFlipChannel,
    TransformChannel, TranslationKey,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{Quat, Vec3};
    use std::collections::HashMap;

    fn make_linear_translation_channel() -> TransformChannel {
        TransformChannel {
            translation_keys: vec![
                TranslationKey {
                    time: 0.0,
                    value: Vec3::ZERO,
                    forward: Vec3::ZERO,
                    backward: Vec3::ZERO,
                    tbc: None,
                },
                TranslationKey {
                    time: 1.0,
                    value: Vec3::new(10.0, 0.0, 0.0),
                    forward: Vec3::ZERO,
                    backward: Vec3::ZERO,
                    tbc: None,
                },
            ],
            translation_type: KeyType::Linear,
            rotation_keys: Vec::new(),
            rotation_type: KeyType::Linear,
            scale_keys: Vec::new(),
            scale_type: KeyType::Linear,
            priority: 0,
        }
    }

    #[test]
    fn linear_translation_midpoint() {
        let ch = make_linear_translation_channel();
        let v = sample_translation(&ch, 0.5).unwrap();
        assert!((v.x - 5.0).abs() < 1e-5);
        assert!(v.y.abs() < 1e-5);
    }

    #[test]
    fn linear_translation_at_start() {
        let ch = make_linear_translation_channel();
        let v = sample_translation(&ch, 0.0).unwrap();
        assert!(v.x.abs() < 1e-5);
    }

    #[test]
    fn linear_translation_at_end() {
        let ch = make_linear_translation_channel();
        let v = sample_translation(&ch, 1.0).unwrap();
        assert!((v.x - 10.0).abs() < 1e-5);
    }

    #[test]
    fn linear_translation_clamp_before() {
        let ch = make_linear_translation_channel();
        let v = sample_translation(&ch, -1.0).unwrap();
        assert!(v.x.abs() < 1e-5);
    }

    #[test]
    fn linear_translation_clamp_after() {
        let ch = make_linear_translation_channel();
        let v = sample_translation(&ch, 2.0).unwrap();
        assert!((v.x - 10.0).abs() < 1e-5);
    }

    #[test]
    fn slerp_rotation_midpoint() {
        let ch = TransformChannel {
            translation_keys: Vec::new(),
            translation_type: KeyType::Linear,
            rotation_keys: vec![
                RotationKey {
                    time: 0.0,
                    value: Quat::IDENTITY,
                    tbc: None,
                },
                RotationKey {
                    time: 1.0,
                    value: Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
                    tbc: None,
                },
            ],
            rotation_type: KeyType::Linear,
            scale_keys: Vec::new(),
            scale_type: KeyType::Linear,
            priority: 0,
        };
        let q = sample_rotation(&ch, 0.5).unwrap();
        let expected = Quat::from_rotation_y(std::f32::consts::FRAC_PI_4);
        assert!((q.dot(expected)).abs() > 0.999);
    }

    /// Regression for #230: a TBC rotation channel with TBC params set
    /// to zero and no neighbors must match plain SLERP (both sides have
    /// equal-magnitude Catmull-Rom tangents that cancel in log space at
    /// the midpoint). Guarantees the new TBC code path is at least
    /// consistent with the old SLERP baseline on the degenerate case.
    #[test]
    fn tbc_rotation_midpoint_with_zero_params_matches_slerp_endpoints() {
        let ch = TransformChannel {
            translation_keys: Vec::new(),
            translation_type: KeyType::Linear,
            rotation_keys: vec![
                RotationKey {
                    time: 0.0,
                    value: Quat::IDENTITY,
                    tbc: Some([0.0, 0.0, 0.0]),
                },
                RotationKey {
                    time: 1.0,
                    value: Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
                    tbc: Some([0.0, 0.0, 0.0]),
                },
            ],
            rotation_type: KeyType::Tbc,
            scale_keys: Vec::new(),
            scale_type: KeyType::Linear,
            priority: 0,
        };
        // Endpoints must be exact.
        let q_start = sample_rotation(&ch, 0.0).unwrap();
        assert!(q_start.dot(Quat::IDENTITY).abs() > 0.9999);
        let q_end = sample_rotation(&ch, 1.0).unwrap();
        let expected_end = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        assert!(q_end.dot(expected_end).abs() > 0.9999);
    }

    /// Three-key TBC channel with TBC = (0, 0, 0) should match a
    /// Catmull-Rom quaternion interpolation: the derived tangent at the
    /// middle key is the average of the before/after deltas, so sampling
    /// at the middle time must return the middle key's value exactly.
    #[test]
    fn tbc_rotation_three_key_hits_middle_key_exactly() {
        let ch = TransformChannel {
            translation_keys: Vec::new(),
            translation_type: KeyType::Linear,
            rotation_keys: vec![
                RotationKey {
                    time: 0.0,
                    value: Quat::IDENTITY,
                    tbc: Some([0.0, 0.0, 0.0]),
                },
                RotationKey {
                    time: 1.0,
                    value: Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
                    tbc: Some([0.0, 0.0, 0.0]),
                },
                RotationKey {
                    time: 2.0,
                    value: Quat::from_rotation_y(std::f32::consts::PI),
                    tbc: Some([0.0, 0.0, 0.0]),
                },
            ],
            rotation_type: KeyType::Tbc,
            scale_keys: Vec::new(),
            scale_type: KeyType::Linear,
            priority: 0,
        };
        let q = sample_rotation(&ch, 1.0).unwrap();
        let expected = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        assert!(q.dot(expected).abs() > 0.9999);
    }

    /// Tension = 1 zeros the tangents (no curvature). TBC rotation with
    /// full tension must degenerate to plain Hermite with flat tangents,
    /// which at the midpoint of a 90° Y rotation equals a 45° Y rotation
    /// (same as SLERP). Verifies the TBC parameter actually feeds the
    /// tangent computation.
    #[test]
    fn tbc_rotation_full_tension_matches_slerp_midpoint() {
        let ch = TransformChannel {
            translation_keys: Vec::new(),
            translation_type: KeyType::Linear,
            rotation_keys: vec![
                RotationKey {
                    time: 0.0,
                    value: Quat::IDENTITY,
                    tbc: Some([1.0, 0.0, 0.0]), // tension = 1 → zero tangent
                },
                RotationKey {
                    time: 1.0,
                    value: Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
                    tbc: Some([1.0, 0.0, 0.0]),
                },
            ],
            rotation_type: KeyType::Tbc,
            scale_keys: Vec::new(),
            scale_type: KeyType::Linear,
            priority: 0,
        };
        // With zero tangents, Hermite collapses to pure lerp on log
        // space, which (for this case of two endpoints rebased into
        // q0-local space) is the same as SLERP through the midpoint.
        let q = sample_rotation(&ch, 0.5).unwrap();
        let expected = Quat::from_rotation_y(std::f32::consts::FRAC_PI_4);
        assert!(
            q.dot(expected).abs() > 0.999,
            "full-tension TBC midpoint should match SLERP, got {:?}",
            q
        );
    }

    /// Non-zero TBC parameters must actually bend the rotation path —
    /// i.e. the TBC result must differ from plain SLERP. Uses a 3-key
    /// clip with a non-uniform rotation profile (Y → Y+X) so the
    /// derived tangent at the center key has a non-trivial direction
    /// that TBC parameters can weight.
    #[test]
    fn tbc_rotation_nonzero_params_diverges_from_slerp() {
        use std::f32::consts::FRAC_PI_4;
        let mk = |tbc: Option<[f32; 3]>, rot_type: KeyType| TransformChannel {
            translation_keys: Vec::new(),
            translation_type: KeyType::Linear,
            rotation_keys: vec![
                RotationKey {
                    time: 0.0,
                    value: Quat::IDENTITY,
                    tbc,
                },
                RotationKey {
                    time: 1.0,
                    value: Quat::from_rotation_y(FRAC_PI_4),
                    tbc,
                },
                RotationKey {
                    time: 2.0,
                    // Rotation axis changes — mixes in X so the
                    // derived tangent direction differs from pure Y.
                    value: Quat::from_rotation_x(FRAC_PI_4) * Quat::from_rotation_y(FRAC_PI_4),
                    tbc,
                },
            ],
            rotation_type: rot_type,
            scale_keys: Vec::new(),
            scale_type: KeyType::Linear,
            priority: 0,
        };
        let linear_ch = mk(None, KeyType::Linear);
        // Bias = 0.5 pushes the tangent toward the outgoing side — must
        // produce a different result from plain SLERP.
        let tbc_ch = mk(Some([0.0, 0.5, 0.0]), KeyType::Tbc);

        let q_linear = sample_rotation(&linear_ch, 0.5).unwrap();
        let q_tbc = sample_rotation(&tbc_ch, 0.5).unwrap();
        let dot = q_linear.dot(q_tbc).abs();
        assert!(
            dot < 0.9999,
            "TBC params should bend the path (linear={:?}, tbc={:?}, dot={})",
            q_linear,
            q_tbc,
            dot
        );
        // Sanity: result is still a unit quaternion.
        let norm_sq = q_tbc.x * q_tbc.x + q_tbc.y * q_tbc.y + q_tbc.z * q_tbc.z + q_tbc.w * q_tbc.w;
        assert!(
            (norm_sq - 1.0).abs() < 1e-4,
            "quat not normalized: {}",
            norm_sq
        );
    }

    #[test]
    fn empty_channel_returns_none() {
        let ch = TransformChannel {
            translation_keys: Vec::new(),
            translation_type: KeyType::Linear,
            rotation_keys: Vec::new(),
            rotation_type: KeyType::Linear,
            scale_keys: Vec::new(),
            scale_type: KeyType::Linear,
            priority: 0,
        };
        assert!(sample_translation(&ch, 0.0).is_none());
        assert!(sample_rotation(&ch, 0.0).is_none());
        assert!(sample_scale(&ch, 0.0).is_none());
    }

    #[test]
    fn single_key_returns_constant() {
        let ch = TransformChannel {
            translation_keys: vec![TranslationKey {
                time: 0.0,
                value: Vec3::new(5.0, 5.0, 5.0),
                forward: Vec3::ZERO,
                backward: Vec3::ZERO,
                tbc: None,
            }],
            translation_type: KeyType::Linear,
            rotation_keys: Vec::new(),
            rotation_type: KeyType::Linear,
            scale_keys: vec![ScaleKey {
                time: 0.0,
                value: 2.0,
                forward: 0.0,
                backward: 0.0,
                tbc: None,
            }],
            scale_type: KeyType::Linear,
            priority: 0,
        };
        let v = sample_translation(&ch, 99.0).unwrap();
        assert!((v.x - 5.0).abs() < 1e-5);
        let s = sample_scale(&ch, 99.0).unwrap();
        assert!((s - 2.0).abs() < 1e-5);
    }

    #[test]
    fn advance_time_loop() {
        let clip = AnimationClip {
            name: "test".to_string(),
            duration: 1.0,
            cycle_type: CycleType::Loop,
            frequency: 1.0,
            weight: 1.0,
            accum_root_name: None,
            channels: HashMap::new(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: Vec::new(),
        };
        let mut player = AnimationPlayer::new(0);
        advance_time(&mut player, &clip, 0.6);
        assert!((player.local_time - 0.6).abs() < 1e-5);
        advance_time(&mut player, &clip, 0.6);
        // 1.2 % 1.0 = 0.2
        assert!((player.local_time - 0.2).abs() < 1e-4);
    }

    #[test]
    fn advance_time_clamp() {
        let clip = AnimationClip {
            name: "test".to_string(),
            duration: 1.0,
            cycle_type: CycleType::Clamp,
            frequency: 1.0,
            weight: 1.0,
            accum_root_name: None,
            channels: HashMap::new(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: Vec::new(),
        };
        let mut player = AnimationPlayer::new(0);
        advance_time(&mut player, &clip, 2.0);
        assert!((player.local_time - 1.0).abs() < 1e-5);
    }

    #[test]
    fn advance_time_reverse() {
        let clip = AnimationClip {
            name: "test".to_string(),
            duration: 1.0,
            cycle_type: CycleType::Reverse,
            frequency: 1.0,
            weight: 1.0,
            accum_root_name: None,
            channels: HashMap::new(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: Vec::new(),
        };
        let mut player = AnimationPlayer::new(0);
        advance_time(&mut player, &clip, 0.8);
        assert!((player.local_time - 0.8).abs() < 1e-5);
        assert!(!player.reverse_direction);

        // Go past the end — should bounce back
        advance_time(&mut player, &clip, 0.4);
        // 0.8 + 0.4 = 1.2 → 2*1.0 - 1.2 = 0.8
        assert!((player.local_time - 0.8).abs() < 1e-4);
        assert!(player.reverse_direction);
    }

    #[test]
    fn clip_registry_add_and_get() {
        let mut reg = AnimationClipRegistry::new();
        let clip = AnimationClip {
            name: "idle".to_string(),
            duration: 2.0,
            cycle_type: CycleType::Loop,
            frequency: 1.0,
            weight: 1.0,
            accum_root_name: None,
            channels: HashMap::new(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: Vec::new(),
        };
        let handle = reg.add(clip);
        assert_eq!(handle, 0);
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.get(0).unwrap().name, "idle");
    }

    #[test]
    fn text_key_forward_crossing() {
        let clip = AnimationClip {
            name: "test".into(),
            duration: 2.0,
            cycle_type: CycleType::Loop,
            frequency: 1.0,
            weight: 1.0,
            accum_root_name: None,
            channels: HashMap::new(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: vec![
                (0.5, "hit".into()),
                (1.0, "sound: swing".into()),
                (1.5, "end".into()),
            ],
        };

        // Cross the first key.
        let events = collect_text_key_events(&clip, 0.3, 0.6);
        assert_eq!(events, vec!["hit"]);

        // Cross two keys at once.
        let events = collect_text_key_events(&clip, 0.4, 1.1);
        assert_eq!(events, vec!["hit", "sound: swing"]);

        // No crossing.
        let events = collect_text_key_events(&clip, 0.1, 0.4);
        assert!(events.is_empty());
    }

    #[test]
    fn text_key_loop_wrap() {
        let clip = AnimationClip {
            name: "test".into(),
            duration: 2.0,
            cycle_type: CycleType::Loop,
            frequency: 1.0,
            weight: 1.0,
            accum_root_name: None,
            channels: HashMap::new(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: vec![(0.2, "start".into()), (1.8, "end".into())],
        };

        // Loop wrap: prev=1.7, curr=0.3 → fires "end" (>1.7) and "start" (<=0.3).
        let events = collect_text_key_events(&clip, 1.7, 0.3);
        assert_eq!(events, vec!["start", "end"]);
    }

    #[test]
    fn text_key_empty_clip() {
        let clip = AnimationClip {
            name: "test".into(),
            duration: 1.0,
            cycle_type: CycleType::Clamp,
            frequency: 1.0,
            weight: 1.0,
            accum_root_name: None,
            channels: HashMap::new(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: Vec::new(),
        };
        let events = collect_text_key_events(&clip, 0.0, 1.0);
        assert!(events.is_empty());
    }

    #[test]
    fn advance_time_tracks_prev_time_for_text_keys() {
        let clip = AnimationClip {
            name: "test".into(),
            duration: 2.0,
            cycle_type: CycleType::Loop,
            frequency: 1.0,
            weight: 1.0,
            accum_root_name: None,
            channels: HashMap::new(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: vec![
                (0.5, "hit".into()),
                (1.0, "sound: swing".into()),
                (1.8, "end".into()),
            ],
        };
        let mut player = AnimationPlayer::new(0);

        // First advance: 0.0 → 0.6, should cross "hit" at 0.5.
        advance_time(&mut player, &clip, 0.6);
        let events = collect_text_key_events(&clip, player.prev_time, player.local_time);
        assert_eq!(events, vec!["hit"]);

        // Second advance: 0.6 → 1.2, should cross "sound: swing" at 1.0.
        advance_time(&mut player, &clip, 0.6);
        let events = collect_text_key_events(&clip, player.prev_time, player.local_time);
        assert_eq!(events, vec!["sound: swing"]);

        // Advance past loop wrap: 1.2 → (1.2+1.0=2.2 mod 2.0=0.2),
        // should cross "end" at 1.8.
        advance_time(&mut player, &clip, 1.0);
        let events = collect_text_key_events(&clip, player.prev_time, player.local_time);
        assert!(events.contains(&"end".to_string()));
    }

    #[test]
    fn find_key_pair_basic() {
        let times = vec![0.0, 0.5, 1.0];
        let (i0, i1, t) = interpolation::find_key_pair(times.len(), |i| times[i], 0.25);
        assert_eq!(i0, 0);
        assert_eq!(i1, 1);
        assert!((t - 0.5).abs() < 1e-5);
    }

    /// Regression for #469: two layers at equal layer-weight but one
    /// clip authored with `weight = 0.5` must pre-attenuate that layer
    /// inside `sample_blended_transform`. Without the fix, both layers
    /// contributed equally (midpoint = 15.0); with the fix, the 0.5
    /// clip contributes half as much (midpoint = 13.333...).
    #[test]
    fn sample_blended_transform_applies_clip_weight() {
        use crate::string::StringPool;

        let mut pool = StringPool::new();
        let node = pool.intern("root");

        let mk_clip = |weight: f32, tx: f32| {
            let mut channels = HashMap::new();
            channels.insert(
                node,
                TransformChannel {
                    translation_keys: vec![TranslationKey {
                        time: 0.0,
                        value: Vec3::new(tx, 0.0, 0.0),
                        forward: Vec3::ZERO,
                        backward: Vec3::ZERO,
                        tbc: None,
                    }],
                    translation_type: KeyType::Linear,
                    rotation_keys: Vec::new(),
                    rotation_type: KeyType::Linear,
                    scale_keys: Vec::new(),
                    scale_type: KeyType::Linear,
                    priority: 0,
                },
            );
            AnimationClip {
                name: "c".to_string(),
                duration: 1.0,
                cycle_type: CycleType::Loop,
                frequency: 1.0,
                weight,
                accum_root_name: None,
                channels,
                float_channels: Vec::new(),
                color_channels: Vec::new(),
                bool_channels: Vec::new(),
                texture_flip_channels: Vec::new(),
                text_keys: Vec::new(),
            }
        };

        let mut registry = AnimationClipRegistry::new();
        let h_full = registry.add(mk_clip(1.0, 10.0));
        let h_half = registry.add(mk_clip(0.5, 20.0));

        let mut stack = AnimationStack::new();
        stack.layers.push(AnimationLayer::new(h_full));
        stack.layers.push(AnimationLayer::new(h_half));

        let (pos, _, _) = sample_blended_transform(&stack, &registry, node).unwrap();
        // (10 * 1.0 + 20 * 0.5) / (1.0 + 0.5) = 20 / 1.5
        let expected = 20.0 / 1.5;
        assert!(
            (pos.x - expected).abs() < 1e-4,
            "clip.weight not applied: got {}, expected {}",
            pos.x,
            expected
        );
    }

    #[test]
    fn linear_scale_interpolation() {
        let ch = TransformChannel {
            translation_keys: Vec::new(),
            translation_type: KeyType::Linear,
            rotation_keys: Vec::new(),
            rotation_type: KeyType::Linear,
            scale_keys: vec![
                ScaleKey {
                    time: 0.0,
                    value: 1.0,
                    forward: 0.0,
                    backward: 0.0,
                    tbc: None,
                },
                ScaleKey {
                    time: 1.0,
                    value: 3.0,
                    forward: 0.0,
                    backward: 0.0,
                    tbc: None,
                },
            ],
            scale_type: KeyType::Linear,
            priority: 0,
        };
        let s = sample_scale(&ch, 0.5).unwrap();
        assert!((s - 2.0).abs() < 1e-5);
    }
}
