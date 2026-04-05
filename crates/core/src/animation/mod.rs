//! Animation interpolation engine, clip registry, and AnimationPlayer component.
//!
//! Provides keyframe sampling with linear, Hermite (quadratic), and TBC
//! (Kochanek-Bartels) interpolation for position, rotation, and scale channels.

pub mod interpolation;
pub mod player;
pub mod registry;
pub mod root_motion;
pub mod stack;
pub mod text_events;
pub mod types;

// Re-export everything at the module level to preserve the public API.
pub use interpolation::{
    sample_bool_channel, sample_color_channel, sample_float_channel, sample_rotation, sample_scale,
    sample_translation,
};
pub use player::{advance_time, AnimationPlayer};
pub use registry::AnimationClipRegistry;
pub use root_motion::{split_root_motion, RootMotionDelta};
pub use stack::{advance_stack, sample_blended_transform, AnimationLayer, AnimationStack};
pub use text_events::collect_text_key_events;
pub use types::{
    AnimBoolKey, AnimColorKey, AnimFloatKey, AnimationClip, BoolChannel, ColorChannel, ColorTarget,
    CycleType, FloatChannel, FloatTarget, KeyType, RotationKey, ScaleKey, TransformChannel,
    TranslationKey,
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
            text_keys: Vec::new(),
        };
        let events = collect_text_key_events(&clip, 0.0, 1.0);
        assert!(events.is_empty());
    }

    #[test]
    fn find_key_pair_basic() {
        let times = vec![0.0, 0.5, 1.0];
        let (i0, i1, t) = interpolation::find_key_pair(&times, 0.25);
        assert_eq!(i0, 0);
        assert_eq!(i1, 1);
        assert!((t - 0.5).abs() < 1e-5);
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
