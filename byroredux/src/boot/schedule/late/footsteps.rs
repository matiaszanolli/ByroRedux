//! Footstep and impact audio dispatch.
//!
//! Split out of `late.rs` (#6703); the rows, their order and their comments are unchanged.

use crate::components::{FootstepConfig, ImpactConfig};
use crate::systems::{footstep_system, impact_audio_system};
use byroredux_core::ecs::{Access, ActiveCamera, Scheduler, Stage};

/// Footstep and impact audio dispatch.
pub(super) fn register_footstep_systems(scheduler: &mut Scheduler) {
    // M44 Phase 3.5 / #3652 — footstep dispatch. Reads `GlobalTransform` for
    // the world-space spawn position. #3652 moved it here (from a PostUpdate
    // exclusive, #848) when the camera was the character-mode walker, so it
    // would read `camera_follow_system`'s pose, not last frame's. #5146 made
    // the player body the only accumulating emitter in character mode: its
    // `GlobalTransform` comes from Early `character_controller_system` plus
    // PostUpdate propagation (Physics writes back only the local
    // `Transform`), so no audible footstep reads `camera_follow_system`'s
    // pose any more, and the FlyCam camera is likewise resolved by
    // PostUpdate. What the Late slot after `camera_follow_system` still buys
    // is the re-seed of the skipped camera emitter (#5348): it tracks the
    // boom's current pose, so a later FlyCam switch starts its stride from
    // where the camera actually is. `footstep_runs_after_camera_follow_in_late`
    // pins that order; moving the system to PostUpdate after propagation
    // would be equally correct for the body and would cost only that
    // one-frame re-seed lag. Also gains a declared Access here for the first
    // time (previously a bare `add_exclusive`, unlike every neighboring
    // exclusive in this file).
    // #5146 adds the character-mode locomotion reads: `PlayerEntity` picks
    // the walking body, `CharacterController.is_grounded` and the
    // `WaterContact` swimlevel gate accumulation to grounded land motion.
    // #5556 — the Wave-4 surface-aware lookup reads `PhysicsWorld` (the
    // per-trigger ray, #886edcded) and, since #5545, the player body's
    // `RapierHandles` to exclude its own capsule; both were missing here.
    scheduler.add_exclusive_with_access(
        Stage::Late,
        footstep_system,
        Access::new()
            .reads_resource::<FootstepConfig>()
            .reads_resource::<crate::systems::PlayerEntity>()
            // #5348 — the camera-emitter skip: `ActiveCamera` while
            // `PlayerMode` is `Character`.
            .reads_resource::<crate::systems::PlayerMode>()
            .reads_resource::<ActiveCamera>()
            .reads_resource::<byroredux_physics::PhysicsWorld>()
            .reads::<byroredux_physics::RapierHandles>()
            .writes_resource::<crate::components::FootstepScratch>()
            .writes_resource::<byroredux_audio::AudioWorld>()
            .reads::<byroredux_core::ecs::GlobalTransform>()
            .reads::<byroredux_physics::CharacterController>()
            .reads::<byroredux_core::ecs::components::water::WaterContact>()
            .writes::<crate::components::FootstepEmitter>(),
    );
    // Wave 4 clutter impacts — drains the `started` contact-force
    // crossings `physics_sync_system`'s step collected and plays the
    // surface-material-driven impact sound. Exclusive on `PhysicsWorld`
    // (the take is a write) beside the AudioWorld write; runs right after
    // the footstep adapter so the two audio consumers never interleave
    // their one-lock-at-a-time sequences with each other.
    scheduler.add_exclusive_with_access(
        Stage::Late,
        impact_audio_system,
        Access::new()
            .reads_resource::<ImpactConfig>()
            .writes_resource::<byroredux_physics::PhysicsWorld>()
            .writes_resource::<byroredux_audio::AudioWorld>(),
    );
    // The re-seed of the skipped camera emitter reads `camera_follow_system`'s pose (#5348).
    scheduler.run_after(
        byroredux_core::ecs::system_name(&crate::systems::footstep_system),
        byroredux_core::ecs::system_name(&crate::systems::camera_follow_system),
    );
}
