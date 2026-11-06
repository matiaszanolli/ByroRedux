//! The camera pose author and the billboards that read it.
//!
//! Split out of `late.rs` (#6703); the rows, their order and their comments are unchanged.

use crate::components::InputState;
use crate::systems::make_billboard_system;
use byroredux_core::ecs::{Access, ActiveCamera, Scheduler, Stage, TotalTime, Transform};

/// The camera pose author and the billboards that read it.
pub(super) fn register_camera_systems(scheduler: &mut Scheduler) {
    // M28.5 — camera follow runs in Stage::Late, AFTER
    // `physics_sync_system` has settled the kinematic body's
    // post-step pose. Must run BEFORE `audio_system` /
    // `submersion_system` (both read camera GlobalTransform).
    // Both are `Stage::Late` **exclusives**, which sequence after this
    // parallel batch, so the ordering is structural rather than incidental.
    // #3180 — `submersion_system` was in `Stage::PostUpdate` when this
    // comment was written, so the second half of the claim was false; it was
    // moved to a Late exclusive rather than the comment being weakened.
    // The character system writes both Transform and
    // GlobalTransform on the camera to bypass the missing
    // late-stage propagation pass.
    //
    // #3652 — `make_billboard_system` joined that list of Late consumers
    // just below, for the same reason `submersion_system` did (#3180): it
    // used to sit in `Stage::PostUpdate`, an earlier stage, so it read this
    // pose one frame stale in player/third-person mode. This system cannot
    // move the other direction (to PostUpdate) to fix that instead — its
    // whole reason for being in `Stage::Late` is the post-Physics body
    // position in the paragraph above, which PostUpdate runs before Physics
    // even has a chance to produce.
    scheduler.add_to_with_access(
        Stage::Late,
        crate::systems::camera_follow_system,
        Access::new()
            // #2676 / CONC-D3-NEW-02 — the body's very first statement
            // reads `PlayerMode` as an early-out gate. Undeclared, the
            // access analyzer could not see it, so the `Stage::Late`
            // parallel batch's `known_conflict_count() == 0` invariant
            // — the thing that makes cross-thread ABBA structurally
            // unreachable among parallel systems — was computed from an
            // incomplete declaration. No live race today (the only
            // writer, `toggle_player_mode`, takes `&mut World` and so
            // can't run inside the parallel window), but this system
            // writes the camera pose the renderer, audio listener, and
            // `submersion_system` all consume. Same shape of fix as
            // #1787's `ContactConfig` on `physics_sync_system`.
            .reads_resource::<crate::systems::PlayerMode>()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads_resource::<ActiveCamera>()
            .reads_resource::<InputState>()
            // P3 player body — third-person boom consult.
            .reads_resource::<crate::player_body::PlayerCameraView>()
            .reads::<byroredux_physics::CharacterController>()
            .reads::<byroredux_core::ecs::GlobalTransform>()
            .writes::<byroredux_core::ecs::GlobalTransform>()
            .reads::<Transform>()
            .writes::<Transform>(),
    );
    // #3652 (CONC-D4-2026-08-30-01) — moved here from `Stage::PostUpdate`.
    // Registered exclusive (not in the Late parallel batch above) so it
    // sequences AFTER `camera_follow_system`'s write — exclusives run after
    // the whole parallel batch completes, same structural guarantee
    // `submersion_system` below relies on for the same pose. Entity-disjoint
    // from every other Late exclusive (billboard meshes vs. ragdoll bones /
    // the camera / water volumes), so its position among them is otherwise
    // free; placed first since it's the most direct consumer of what the
    // parallel batch just wrote.
    scheduler.add_exclusive_with_access(
        Stage::Late,
        make_billboard_system(),
        Access::new()
            .reads_resource::<ActiveCamera>()
            .reads_resource::<TotalTime>()
            .reads_resource::<byroredux_core::ecs::components::groundcover::WindField>()
            .reads::<byroredux_core::ecs::Billboard>()
            .reads::<byroredux_core::ecs::SpeedTreeWind>()
            .writes::<byroredux_core::ecs::GlobalTransform>(),
    );
}
