//! Single-clip animation player component and time advancement.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::{Component, EntityId};

use super::types::{AnimationClip, CycleType};

/// ECS component that drives animation playback on an entity subtree.
///
/// Attached to the root entity of an animated mesh. The animation system
/// uses the clip's channel map to find child entities by `Name` and
/// update their `Transform` each frame.
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct AnimationPlayer {
    pub clip_handle: u32,
    pub local_time: f32,
    pub playing: bool,
    pub speed: f32,
    /// Tracks ping-pong direction for CycleType::Reverse.
    pub reverse_direction: bool,
    /// Root entity of the subtree to animate. When set, name lookups are
    /// scoped to this entity's descendants only (no global name collisions).
    pub root_entity: Option<EntityId>,
    /// Previous frame's local_time — used by `collect_text_key_events()` to
    /// detect which text keys were crossed during the last `advance_time()`.
    pub prev_time: f32,
}

impl AnimationPlayer {
    pub fn new(clip_handle: u32) -> Self {
        Self {
            clip_handle,
            local_time: 0.0,
            playing: true,
            speed: 1.0,
            reverse_direction: false,
            root_entity: None,
            prev_time: 0.0,
        }
    }

    /// Create a player scoped to a specific entity subtree.
    pub fn with_root(mut self, root: EntityId) -> Self {
        self.root_entity = Some(root);
        self
    }
}

impl Component for AnimationPlayer {
    type Storage = SparseSetStorage<Self>;
}

/// Fold a ping-pong (`CycleType::Reverse`) clock advanced by `delta` back into
/// `[0, duration]` via a triangle wave over period `2*duration`.
///
/// Returns the new `(local_time, reverse_direction)`, where `reverse_direction`
/// means time is currently moving backward (duration → 0). Unlike a single
/// reflection, this stays in range for **any** `delta` magnitude (a frame hitch
/// on a short clip, a large `speed`/`frequency`, or a negative `delta`), because
/// it reconstructs the monotonic phase, advances it, and wraps a full period.
pub(crate) fn fold_reverse_time(
    local_time: f32,
    reverse_direction: bool,
    delta: f32,
    duration: f32,
) -> (f32, bool) {
    if duration <= 0.0 {
        return (0.0, false);
    }
    let period = 2.0 * duration;
    // Reconstruct the monotonic phase along [0, 2*duration): forward maps
    // directly, backward is mirrored into the second half.
    let phase = if reverse_direction {
        period - local_time
    } else {
        local_time
    };
    let m = (phase + delta).rem_euclid(period);
    if m > duration {
        (period - m, true)
    } else {
        (m, false)
    }
}

/// Advance the animation time according to the cycle type.
/// Updates `prev_time` to the value of `local_time` before advancing.
pub fn advance_time(player: &mut AnimationPlayer, clip: &AnimationClip, dt: f32) {
    if !player.playing {
        return;
    }

    player.prev_time = player.local_time;
    let delta = dt * player.speed * clip.frequency;

    match clip.cycle_type {
        CycleType::Clamp => {
            player.local_time = (player.local_time + delta).min(clip.duration);
        }
        CycleType::Loop => {
            player.local_time += delta;
            if clip.duration > 0.0 {
                player.local_time %= clip.duration;
                if player.local_time < 0.0 {
                    player.local_time += clip.duration;
                }
            }
        }
        CycleType::Reverse => {
            let (local_time, reverse_direction) = fold_reverse_time(
                player.local_time,
                player.reverse_direction,
                delta,
                clip.duration,
            );
            player.local_time = local_time;
            player.reverse_direction = reverse_direction;
        }
    }
}

#[cfg(test)]
mod fold_tests {
    //! #1980 — the triangle-wave fold shared by `advance_time`
    //! (`player.rs`) and `advance_stack` (`stack.rs`) must keep
    //! `local_time` in `[0, duration]` for any `delta` magnitude,
    //! including one larger than a full `2*duration` period.
    use super::fold_reverse_time;

    fn in_range(t: f32, duration: f32) -> bool {
        t >= 0.0 && t <= duration
    }

    #[test]
    fn single_reflection_case_matches_legacy() {
        // 0.8 forward + 0.4 → past 1.0, bounce to 0.8 reversing.
        let (t, rev) = fold_reverse_time(0.8, false, 0.4, 1.0);
        assert!((t - 0.8).abs() < 1e-5);
        assert!(rev);
    }

    #[test]
    fn delta_larger_than_full_period_stays_in_range() {
        let duration = 0.1;
        // delta 0.55 spans 2.75 periods of 2*duration=0.2.
        let (t, _) = fold_reverse_time(0.05, false, 0.55, duration);
        assert!(in_range(t, duration), "t={t} escaped [0,{duration}]");
    }

    #[test]
    fn negative_delta_stays_in_range() {
        let duration = 1.0;
        // A negative advance (speed<0) folds correctly too.
        let (t, _) = fold_reverse_time(0.2, false, -3.7, duration);
        assert!(in_range(t, duration), "t={t} escaped [0,{duration}]");
    }

    #[test]
    fn zero_duration_is_safe() {
        let (t, rev) = fold_reverse_time(0.5, true, 1.0, 0.0);
        assert_eq!(t, 0.0);
        assert!(!rev);
    }
}

#[cfg(all(test, feature = "inspect"))]
mod inspect_tests {
    //! #486 — debug snapshots must preserve ping-pong `reverse_direction`
    //! (and every other field). Round-trips AnimationPlayer through
    //! JSON and asserts byte-for-byte recovery of playback state.
    use super::*;

    #[test]
    fn reverse_direction_round_trips_through_json() {
        let mut player = AnimationPlayer::new(42);
        // Simulate a ping-pong animation that has crossed the end
        // boundary — `reverse_direction` has latched to true, the
        // time has rebounded back from past-duration.
        player.reverse_direction = true;
        player.local_time = 0.75;
        player.prev_time = 1.05;
        player.speed = 1.25;
        player.playing = true;

        let json = serde_json::to_value(&player).expect("serialize");
        assert_eq!(
            json.get("reverse_direction"),
            Some(&serde_json::Value::Bool(true))
        );

        let reloaded: AnimationPlayer = serde_json::from_value(json).expect("deserialize");
        assert_eq!(reloaded.clip_handle, 42);
        assert!(
            reloaded.reverse_direction,
            "ping-pong direction must survive snapshot reload"
        );
        assert_eq!(reloaded.local_time, 0.75);
        assert_eq!(reloaded.prev_time, 1.05);
        assert_eq!(reloaded.speed, 1.25);
        assert!(reloaded.playing);
    }

    #[test]
    fn default_player_round_trips_cleanly() {
        // Guard against serde forgetting a field default — the `new`
        // constructor is the canonical initial state; snapshotting it
        // immediately should reload identical.
        let original = AnimationPlayer::new(7);
        let json = serde_json::to_value(&original).unwrap();
        let reloaded: AnimationPlayer = serde_json::from_value(json).unwrap();
        assert_eq!(reloaded.clip_handle, 7);
        assert!(!reloaded.reverse_direction);
        assert_eq!(reloaded.local_time, 0.0);
        assert_eq!(reloaded.speed, 1.0);
        assert!(reloaded.playing);
        assert!(reloaded.root_entity.is_none());
    }
}
