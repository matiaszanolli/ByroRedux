//! Single-clip animation player component and time advancement.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::{Component, EntityId};

use super::types::{AnimationClip, CycleType};

/// ECS component that drives animation playback on an entity subtree.
///
/// Attached to the root entity of an animated mesh. The animation system
/// uses the clip's channel map to find child entities by `Name` and
/// update their `Transform` each frame.
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
            if player.reverse_direction {
                player.local_time -= delta;
                if player.local_time <= 0.0 {
                    player.local_time = -player.local_time;
                    player.reverse_direction = false;
                }
            } else {
                player.local_time += delta;
                if player.local_time >= clip.duration {
                    player.local_time = 2.0 * clip.duration - player.local_time;
                    player.reverse_direction = true;
                }
            }
        }
    }
}
