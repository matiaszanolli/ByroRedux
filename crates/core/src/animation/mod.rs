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
    sample_texture_flip_index, sample_translation,
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

    /// Regression for LC-D5-02 / #1441: a `KeyType::Const` (stepped)
    /// channel must HOLD the start key's value across the whole segment,
    /// not LERP toward the next key. Covers all three TRS samplers.
    #[test]
    fn const_keytype_holds_start_value_across_segment() {
        // Translation: held at k0 (ZERO) for the whole [0,1) segment,
        // snapping to k1 only at the next key time.
        let mut ch = make_linear_translation_channel();
        ch.translation_type = KeyType::Const;
        // Mid-segment must equal k0, NOT the 5.0 a Linear LERP would give.
        let v = sample_translation(&ch, 0.5).unwrap();
        assert!(v.x.abs() < 1e-6, "const must hold k0, got {}", v.x);
        // Just before the next key, still k0.
        let v = sample_translation(&ch, 0.999).unwrap();
        assert!(v.x.abs() < 1e-6, "const must hold k0 up to next key");
        // At the next key time, value is k1.
        let v = sample_translation(&ch, 1.0).unwrap();
        assert!((v.x - 10.0).abs() < 1e-6);

        // Rotation: held at q0 (IDENTITY) mid-segment.
        let rot = TransformChannel {
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
            rotation_type: KeyType::Const,
            scale_keys: Vec::new(),
            scale_type: KeyType::Linear,
            priority: 0,
        };
        let q = sample_rotation(&rot, 0.5).unwrap();
        assert!(
            q.dot(Quat::IDENTITY).abs() > 0.9999,
            "const rotation must hold q0 (identity) mid-segment"
        );

        // Scale: held at k0 (1.0) mid-segment.
        let scl = TransformChannel {
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
                    value: 4.0,
                    forward: 0.0,
                    backward: 0.0,
                    tbc: None,
                },
            ],
            scale_type: KeyType::Const,
            priority: 0,
        };
        let s = sample_scale(&scl, 0.5).unwrap();
        assert!((s - 1.0).abs() < 1e-6, "const scale must hold k0, got {s}");
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
            phase: 0.0,
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

    /// #3258 — `clip.frequency` is raw `NiControllerSequence` data. A NaN
    /// one used to latch `local_time` permanently: the `Loop` arm's
    /// `%= duration` wrap is a no-op on NaN and its `< 0.0` repair is false,
    /// so the pose never recovered and the NaN reached the GPU matrices.
    #[test]
    fn advance_time_survives_a_non_finite_clip_frequency() {
        for frequency in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for cycle_type in [CycleType::Loop, CycleType::Clamp, CycleType::Reverse] {
                let clip = AnimationClip {
                    name: "test".to_string(),
                    duration: 1.0,
                    cycle_type,
                    frequency,
                    phase: 0.0,
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
                player.local_time = 0.25;
                for _ in 0..3 {
                    advance_time(&mut player, &clip, 0.1);
                }
                assert!(
                    player.local_time.is_finite(),
                    "{cycle_type:?} latched local_time to {} on frequency {frequency}",
                    player.local_time
                );
                assert_eq!(
                    player.local_time, 0.25,
                    "a non-integrable rate must advance nothing, not drift"
                );
            }
        }
    }

    /// Sibling of the above — `advance_stack` carries a byte-identical
    /// `Loop` arm and the same unvalidated product (#3258).
    #[test]
    fn advance_stack_survives_a_non_finite_clip_frequency() {
        let clip = AnimationClip {
            name: "c".to_string(),
            duration: 1.0,
            cycle_type: CycleType::Loop,
            frequency: f32::NAN,
            phase: 0.0,
            weight: 1.0,
            accum_root_name: None,
            channels: HashMap::new(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: Vec::new(),
        };
        let mut registry = AnimationClipRegistry::new();
        let handle = registry.add(clip);
        let mut stack = AnimationStack::new();
        stack.layers.push(AnimationLayer::new(handle));

        for _ in 0..3 {
            advance_stack(&mut stack, &registry, 0.1);
        }
        assert!(stack.layers[0].local_time.is_finite());
    }

    #[test]
    fn advance_time_clamp() {
        let clip = AnimationClip {
            name: "test".to_string(),
            duration: 1.0,
            cycle_type: CycleType::Clamp,
            frequency: 1.0,
            phase: 0.0,
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
            phase: 0.0,
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
    fn advance_time_reverse_hitch_larger_than_period() {
        // #1980: a frame hitch on a short Reverse clip advances `delta` past a
        // full `2*duration` period; the triangle-wave fold must keep
        // `local_time` in `[0, duration]` (a single reflection would not).
        let clip = AnimationClip {
            name: "short_reverse".to_string(),
            duration: 0.1,
            cycle_type: CycleType::Reverse,
            frequency: 1.0,
            phase: 0.0,
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
        // dt=0.55 → delta=0.55, far beyond 2*duration=0.2.
        advance_time(&mut player, &clip, 0.55);
        assert!(
            player.local_time >= 0.0 && player.local_time <= clip.duration,
            "local_time {} escaped [0, {}]",
            player.local_time,
            clip.duration
        );
    }

    #[test]
    fn clip_registry_add_and_get() {
        let mut reg = AnimationClipRegistry::new();
        let clip = AnimationClip {
            name: "idle".to_string(),
            duration: 2.0,
            cycle_type: CycleType::Loop,
            frequency: 1.0,
            phase: 0.0,
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
        use crate::string::StringPool;
        let mut pool = StringPool::new();
        let clip = AnimationClip {
            name: "test".into(),
            duration: 2.0,
            cycle_type: CycleType::Loop,
            frequency: 1.0,
            phase: 0.0,
            weight: 1.0,
            accum_root_name: None,
            channels: HashMap::new(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: vec![
                (0.5, pool.intern("hit")),
                (1.0, pool.intern("sound: swing")),
                (1.5, pool.intern("end")),
            ],
        };

        // Cross the first key.
        let events = collect_text_key_events(&clip, &pool, 0.3, 0.6, false, 0.3f32);
        assert_eq!(events, vec!["hit"]);

        // Cross two keys at once.
        let events = collect_text_key_events(&clip, &pool, 0.4, 1.1, false, 0.7f32);
        assert_eq!(events, vec!["hit", "sound: swing"]);

        // No crossing.
        let events = collect_text_key_events(&clip, &pool, 0.1, 0.4, false, 0.3f32);
        assert!(events.is_empty());
    }

    #[test]
    fn text_key_loop_wrap() {
        use crate::string::StringPool;
        let mut pool = StringPool::new();
        let clip = AnimationClip {
            name: "test".into(),
            duration: 2.0,
            cycle_type: CycleType::Loop,
            frequency: 1.0,
            phase: 0.0,
            weight: 1.0,
            accum_root_name: None,
            channels: HashMap::new(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: vec![(0.2, pool.intern("start")), (1.8, pool.intern("end"))],
        };

        // Loop wrap: prev=1.7, curr=0.3 → fires "end" (>1.7) and "start" (<=0.3).
        // Honest advance: forward +0.6 through the wrap in a 2.0 clip, not
        // the `curr - prev` difference (which is negative because of the wrap).
        let events = collect_text_key_events(&clip, &pool, 1.7, 0.3, false, 0.6f32);
        assert_eq!(events, vec!["start", "end"]);
    }

    #[test]
    fn text_key_full_period_advance_fires_every_key_once() {
        // #3034 — a `CycleType::Loop` clip whose per-frame delta lands on
        // an exact multiple of `duration` wraps back onto the exact instant
        // it started from (prev_time == curr_time). The naive forward-
        // window scan `(prev, curr]` is empty for that pair, so pre-fix
        // every text key silently dropped instead of firing for the
        // period(s) actually traversed.
        use crate::string::StringPool;
        let mut pool = StringPool::new();
        let clip = AnimationClip {
            name: "test".into(),
            duration: 2.0,
            cycle_type: CycleType::Loop,
            frequency: 1.0,
            phase: 0.0,
            weight: 1.0,
            accum_root_name: None,
            channels: HashMap::new(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: vec![(0.5, pool.intern("hit")), (1.8, pool.intern("end"))],
        };

        // A whole period (2.0s) elapsed and the loop landed exactly back at
        // 0.5 — every key must fire once, not zero times. #3470: the delta is
        // now what says a period elapsed; the (prev, curr) pair alone cannot.
        let events = collect_text_key_events(&clip, &pool, 0.5, 0.5, false, 2.0f32);
        assert_eq!(events, vec!["hit", "end"]);

        // The pair carries no period count — a caller reporting 2 or 3
        // full periods presents the identical (prev, curr) pair. Each key
        // must still fire exactly once, not N times (ONCE-EACH).
        let events = collect_text_key_events(&clip, &pool, 0.5, 0.5, false, 6.0f32);
        assert_eq!(events, vec!["hit", "end"]);
    }

    /// #3470 — the `Loop` sibling of the `Clamp` guard below, and the case
    /// #3034's arm did not distinguish.
    ///
    /// `prev == curr` on a `Loop` clip has two causes: N full periods elapsed
    /// (fire every key), or the playhead did not move at all (fire none). The
    /// pair carries no period count, so only the applied delta separates them.
    ///
    /// The zero case is live, not hypothetical: `App::resumed` runs the
    /// scheduler once with `dt == 0.0` to prime transform state, so pre-fix
    /// every looping clip in the scene delivered ALL of its text keys on the
    /// priming tick — and `AnimationTextKeyEvents` feeds
    /// `cinematic_animation_event_system`, which writes `QuestStageState`.
    #[test]
    fn text_key_loop_at_zero_advance_stays_silent() {
        use crate::string::StringPool;
        let mut pool = StringPool::new();
        let clip = AnimationClip {
            name: "test".into(),
            duration: 2.0,
            cycle_type: CycleType::Loop,
            frequency: 1.0,
            phase: 0.0,
            weight: 1.0,
            accum_root_name: None,
            channels: HashMap::new(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: vec![(0.5, pool.intern("hit")), (1.8, pool.intern("end"))],
        };

        // Identical (prev, curr) to `text_key_full_period_advance_fires_every_key_once`
        // — the ONLY difference is that nothing advanced.
        let events = collect_text_key_events(&clip, &pool, 0.5, 0.5, false, 0.0f32);
        assert!(
            events.is_empty(),
            "#3470: a zero advance must fire nothing; got {events:?}"
        );

        // Drive it through the real player at dt == 0.0, the exact shape of
        // `App::resumed`'s priming tick.
        let mut player = AnimationPlayer::new(0);
        player.local_time = 0.5;
        player.prev_time = 0.5;
        advance_time(&mut player, &clip, 0.0);
        assert_eq!(player.prev_time, player.local_time, "premise: prev == curr");
        let primed = collect_text_key_events(
            &clip,
            &pool,
            player.prev_time,
            player.local_time,
            false,
            player.last_delta,
        );
        assert!(
            primed.is_empty(),
            "#3470: the dt == 0.0 priming tick must not fire text keys; got {primed:?}"
        );
    }

    /// #3470's worse latent variant, named in the issue: `finite_time_delta`
    /// folds a non-finite `dt * speed * frequency` to `0.0` (#3258). A clip
    /// reaching the registry with a NaN frequency from any producer other than
    /// `anim_convert` would therefore present `prev == curr` on EVERY frame —
    /// so pre-fix it fired every text key forever, not just once at startup.
    ///
    /// `advance_stack_survives_a_non_finite_clip_frequency` builds this clip
    /// already but gives it no `text_keys`, which is why the interaction was
    /// invisible to the suite.
    #[test]
    fn text_key_loop_with_non_finite_frequency_stays_silent_every_frame() {
        use crate::string::StringPool;
        let mut pool = StringPool::new();
        let clip = AnimationClip {
            name: "nan-freq".into(),
            duration: 2.0,
            cycle_type: CycleType::Loop,
            frequency: f32::NAN,
            phase: 0.0,
            weight: 1.0,
            accum_root_name: None,
            channels: HashMap::new(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: vec![(0.5, pool.intern("hit")), (1.8, pool.intern("end"))],
        };

        let mut player = AnimationPlayer::new(0);
        player.local_time = 0.5;
        player.prev_time = 0.5;
        for frame in 0..4 {
            advance_time(&mut player, &clip, 0.016);
            let events = collect_text_key_events(
                &clip,
                &pool,
                player.prev_time,
                player.local_time,
                false,
                player.last_delta,
            );
            assert!(
                events.is_empty(),
                "#3470: a NaN-frequency clip folds to a zero advance, so frame \
                 {frame} must fire nothing; got {events:?}"
            );
        }
    }

    #[test]
    fn text_key_settled_clamp_stays_silent_at_prev_eq_curr() {
        // Sibling guard: a `Clamp` clip parked at `duration` also presents
        // prev_time == curr_time on every subsequent frame once it has
        // fully played out — but that's a *settled* clip, not a wrap, and
        // must stay silent forever after, unlike the Loop case above.
        // Pins that #3034's fix is gated on `CycleType::Loop` and doesn't
        // regress `clamped_completion_key_fires_once_at_clip_end` below.
        use crate::string::StringPool;
        let mut pool = StringPool::new();
        let clip = AnimationClip {
            name: "test".into(),
            duration: 1.0,
            cycle_type: CycleType::Clamp,
            frequency: 1.0,
            phase: 0.0,
            weight: 1.0,
            accum_root_name: None,
            channels: HashMap::new(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: vec![(1.0, pool.intern("end"))],
        };
        // Clamp saturated at `duration`: the playhead genuinely stopped, so
        // the applied delta is whatever the caller supplied but the arm is
        // gated on CycleType anyway. Non-zero here to prove that gate, not
        // the #3470 one, is what keeps it silent.
        let events = collect_text_key_events(&clip, &pool, 1.0, 1.0, false, 0.5f32);
        assert!(events.is_empty());
    }

    #[test]
    fn text_key_reverse_backward_leg() {
        // FNV-D6-01 / #2082 — a ping-pong `CycleType::Reverse` clip on its
        // backward leg steps DOWN (prev > curr) with no loop wrap. The keys
        // fired must be those actually crossed — the closed interval
        // `(curr, prev]` — NOT the loop-wrap complement the pre-fix code
        // produced when it branched on `curr < prev` alone.
        use crate::string::StringPool;
        let mut pool = StringPool::new();
        let clip = AnimationClip {
            name: "test".into(),
            duration: 2.0,
            cycle_type: CycleType::Reverse,
            frequency: 1.0,
            phase: 0.0,
            weight: 1.0,
            accum_root_name: None,
            channels: HashMap::new(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: vec![
                (0.5, pool.intern("hit")),
                (1.0, pool.intern("swing")),
                (1.5, pool.intern("end")),
            ],
        };

        // Backward leg 1.2 → 0.4 crosses "hit" (0.5) and "swing" (1.0), NOT
        // "end" (1.5). With reverse_direction=true we get exactly those.
        // The backward leg's delta really is negative — this one is honest.
        let events = collect_text_key_events(&clip, &pool, 1.2, 0.4, true, -0.8f32);
        assert_eq!(events, vec!["hit", "swing"]);

        // The pre-#2082 path (reverse_direction=false) mis-reads the
        // descending step as a loop wrap and fires the complement — "end".
        // Kept as a guard so a regression that drops the direction flag fails.
        let wrap_misfire = collect_text_key_events(&clip, &pool, 1.2, 0.4, false, -0.8f32);
        assert_eq!(wrap_misfire, vec!["end"]);

        // Forward leg 0.4 → 1.2 (no reverse, no wrap) crosses the same
        // interior keys — parity with the backward leg.
        let forward = collect_text_key_events(&clip, &pool, 0.4, 1.2, false, 0.8f32);
        assert_eq!(forward, vec!["hit", "swing"]);
    }

    #[test]
    fn text_key_empty_clip() {
        use crate::string::StringPool;
        let pool = StringPool::new();
        let clip = AnimationClip {
            name: "test".into(),
            duration: 1.0,
            cycle_type: CycleType::Clamp,
            frequency: 1.0,
            phase: 0.0,
            weight: 1.0,
            accum_root_name: None,
            channels: HashMap::new(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: Vec::new(),
        };
        let events = collect_text_key_events(&clip, &pool, 0.0, 1.0, false, 1.0f32);
        assert!(events.is_empty());
    }

    #[test]
    fn advance_time_tracks_prev_time_for_text_keys() {
        use crate::string::StringPool;
        let mut pool = StringPool::new();
        let clip = AnimationClip {
            name: "test".into(),
            duration: 2.0,
            cycle_type: CycleType::Loop,
            frequency: 1.0,
            phase: 0.0,
            weight: 1.0,
            accum_root_name: None,
            channels: HashMap::new(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: vec![
                (0.5, pool.intern("hit")),
                (1.0, pool.intern("sound: swing")),
                (1.8, pool.intern("end")),
            ],
        };
        let mut player = AnimationPlayer::new(0);

        // First advance: 0.0 → 0.6, should cross "hit" at 0.5.
        advance_time(&mut player, &clip, 0.6);
        let events = collect_text_key_events(
            &clip,
            &pool,
            player.prev_time,
            player.local_time,
            false,
            player.last_delta,
        );
        assert_eq!(events, vec!["hit"]);

        // Second advance: 0.6 → 1.2, should cross "sound: swing" at 1.0.
        advance_time(&mut player, &clip, 0.6);
        let events = collect_text_key_events(
            &clip,
            &pool,
            player.prev_time,
            player.local_time,
            false,
            player.last_delta,
        );
        assert_eq!(events, vec!["sound: swing"]);

        // Advance past loop wrap: 1.2 → (1.2+1.0=2.2 mod 2.0=0.2),
        // should cross "end" at 1.8.
        advance_time(&mut player, &clip, 1.0);
        let events = collect_text_key_events(
            &clip,
            &pool,
            player.prev_time,
            player.local_time,
            false,
            player.last_delta,
        );
        assert!(events.contains(&"end".to_string()));
    }

    #[test]
    fn clamped_completion_key_fires_once_at_clip_end() {
        use crate::string::StringPool;
        let mut pool = StringPool::new();
        let clip = AnimationClip {
            name: "cart exit".into(),
            duration: 1.0,
            cycle_type: CycleType::Clamp,
            frequency: 1.0,
            phase: 0.0,
            weight: 1.0,
            accum_root_name: None,
            channels: HashMap::new(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: vec![(1.0, pool.intern("ExitCartEnd"))],
        };
        let mut player = AnimationPlayer::new(0);

        advance_time(&mut player, &clip, 2.0);
        assert_eq!(
            collect_text_key_events(
                &clip,
                &pool,
                player.prev_time,
                player.local_time,
                false,
                player.last_delta,
            ),
            vec!["exitcartend"]
        );

        advance_time(&mut player, &clip, 1.0);
        assert!(collect_text_key_events(
            &clip,
            &pool,
            player.prev_time,
            player.local_time,
            false,
            player.last_delta,
        )
        .is_empty());
    }

    #[test]
    fn find_key_pair_basic() {
        let times = [0.0, 0.5, 1.0];
        let (i0, i1, t) = interpolation::find_key_pair(times.len(), |i| times[i], 0.25);
        assert_eq!(i0, 0);
        assert_eq!(i1, 1);
        assert!((t - 0.5).abs() < 1e-5);
    }

    /// Regression for #3471 — a keyless channel at the winning priority must
    /// be excluded from the blend, not just from the weight sum.
    ///
    /// `sample_blended_transform`'s weight pass skipped an all-empty channel;
    /// its blend pass filtered only on priority. The excluded layer therefore
    /// still blended, and because the three `sample_*` calls fall back to
    /// identity values rather than "skip", it contributed a real `+1.0 * w` to
    /// `blended_scale` and slerped the rotation toward identity — while
    /// `accumulated_weight` ran past the `total_weight` denominator that never
    /// counted it.
    ///
    /// All-empty channels are ordinary output: `constant_transform_channel`
    /// emits them for every axis whose pose is the `FLT_MAX` sentinel.
    #[test]
    fn keyless_channel_at_max_priority_is_excluded_from_the_blend() {
        use crate::string::StringPool;

        let mut pool = StringPool::new();
        let node = pool.intern("root");

        let mk_clip = |keyed: bool| {
            let mut channels = HashMap::new();
            channels.insert(
                node,
                TransformChannel {
                    translation_keys: if keyed {
                        vec![TranslationKey {
                            time: 0.0,
                            value: Vec3::new(10.0, 0.0, 0.0),
                            forward: Vec3::ZERO,
                            backward: Vec3::ZERO,
                            tbc: None,
                        }]
                    } else {
                        Vec::new()
                    },
                    translation_type: KeyType::Linear,
                    rotation_keys: Vec::new(),
                    rotation_type: KeyType::Linear,
                    scale_keys: if keyed {
                        vec![ScaleKey {
                            time: 0.0,
                            value: 1.0,
                            forward: 0.0,
                            backward: 0.0,
                            tbc: None,
                        }]
                    } else {
                        Vec::new()
                    },
                    scale_type: KeyType::Linear,
                    // Same priority on purpose: the keyless layer must lose to
                    // the filter, not to a priority comparison.
                    priority: 0,
                },
            );
            AnimationClip {
                name: "c".to_string(),
                duration: 1.0,
                cycle_type: CycleType::Loop,
                frequency: 1.0,
                phase: 0.0,
                weight: 1.0,
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
        let keyed = registry.add(mk_clip(true));
        let keyless = registry.add(mk_clip(false));

        let mut stack = AnimationStack::new();
        stack.layers.push(AnimationLayer::new(keyed));
        stack.layers.push(AnimationLayer::new(keyless));

        let (pos, _rot, scale) = sample_blended_transform(&stack, &registry, node)
            .expect("the keyed layer alone must still produce a transform");

        assert!(
            (scale - 1.0).abs() < 1e-5,
            "#3471: keyless channel contributed its 1.0 scale fallback — got {scale}, \
             expected the keyed layer's 1.0 alone"
        );
        assert!(
            (pos.x - 10.0).abs() < 1e-4,
            "#3471: the keyed layer's translation must not be diluted by a layer \
             the weight pass excluded — got {}, expected 10.0",
            pos.x
        );
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
                phase: 0.0,
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
