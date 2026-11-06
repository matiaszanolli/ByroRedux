//! Scene playback and scene fragments.
//!
//! Split out of `quest_scene.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// Scene playback and scene fragments.
pub(super) fn register_scene_playback_systems(scheduler: &mut Scheduler) {
    // SCEN playback must observe the original quest-start batch before the
    // fragment dispatcher can replace the shared sink with chained SetStage
    // advances. Phase conditions affected by those fragments are retried on
    // the following tick.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::scene_playback_system);
    // Execute SCEN VMAD begin/end/phase fragments immediately after playback
    // emits them. Any SetStage effect enters the canonical quest journal for
    // the quest-fragment dispatcher later in this same update.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::scene_fragment_dispatch_system,
    );
}
