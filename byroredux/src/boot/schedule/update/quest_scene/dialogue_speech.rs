//! Scripted line voice and ObScript speech.
//!
//! Split out of `quest_scene.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// Scripted line voice and ObScript speech.
pub(super) fn register_dialogue_speech_systems(scheduler: &mut Scheduler) {
    // M47.3 — the lines `Say` / `SayTo` made actors say: the voice player picks up this
    // frame's, then the timers tick and finished lines run their `SayToDone` blocks.
    scheduler.add_exclusive(Stage::Update, crate::systems::dialogue_voice::scripted_line_voice_system);
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::obscript_speech::obscript_speech_system);
}
