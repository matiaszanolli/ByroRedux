//! Ragdoll writeback: the simulated pose is the last word before render.
//!
//! Split out of `late.rs` (#6703); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// Ragdoll writeback: the simulated pose is the last word before render.
pub(super) fn register_ragdoll_systems(scheduler: &mut Scheduler) {
    // M41.x — ragdoll writeback. Stage::Late guarantees it runs after
    // `physics_sync_system` (Stage::Physics) has stepped the multibody
    // *and* after PostUpdate transform propagation, so overwriting each
    // ragdoll bone's GlobalTransform with the simulated pose is the last
    // word before render (no propagation/animation skip needed).
    //
    // Registered **exclusive** (not in the Late parallel batch): it
    // writes GlobalTransform, and so does `camera_follow_system` in the
    // same stage — declaring both in the parallel batch is a WriteWrite
    // conflict (#1601). They write entity-disjoint sets (camera entity
    // vs. ragdoll bones), so ordering is irrelevant; exclusive sequencing
    // keeps the scheduler's known_conflict_count() at 0. Matches the
    // existing add_exclusive treatment of audio_system / event_cleanup.
    scheduler.add_exclusive(Stage::Late, crate::ragdoll::ragdoll_writeback_system);
}
