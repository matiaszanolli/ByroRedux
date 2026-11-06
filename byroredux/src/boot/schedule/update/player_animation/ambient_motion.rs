//! Spin, loading-model turntable and animated lights.
//!
//! Split out of `player_animation.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};
use crate::systems::{animate_lights_system, loading_model_turntable_system, spin_system};

/// Spin, loading-model turntable and animated lights.
pub(super) fn register_ambient_motion_systems(scheduler: &mut Scheduler) {
    // M27 Phase 3 — `spin_system` writes Transform on entities
    // tagged with `Spinning` (the demo cube). `animation_system`
    // also writes Transform on its own (disjoint) entity set.
    // They never touch the same entity, but the analyzer can't see
    // that — they pair as a WriteWrite Transform conflict. Moving
    // `spin_system` to exclusive sequences it after the Update
    // parallel batch and removes the conflict from the report
    // without changing observable behaviour. Cost: ~µs of lost
    // parallelism on the demo cube; negligible.
    scheduler.add_exclusive(Stage::Update, spin_system);
    // Creation-era loading-screen turntable — same shape as spin_system:
    // writes Transform on its own (marker-gated, at most one entity) set,
    // sequenced after the Update batch to keep the analyzer's
    // Transform WriteWrite row clean.
    scheduler.add_exclusive(Stage::Update, loading_model_turntable_system);
    // Phase 17 — procedural light flicker. Writes
    // LightSource.intensity + Transform.translation on entities
    // with a LightFlicker companion. Exclusive in Update so it
    // sequences AFTER the parallel batch (no Transform conflict
    // with animation_system / spin_system) but BEFORE
    // PostUpdate's transform propagation reads the result.
    scheduler.add_exclusive(Stage::Update, animate_lights_system);
}
