//! Scripted conversations and the force-greet lane.
//!
//! Split out of `quest_scene.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// Scripted conversations and the force-greet lane.
pub(super) fn register_forcegreet_systems(scheduler: &mut Scheduler) {
    // #5367 Phase F — force-greet directives (Dialogue-procedure
    // packages) walk their NPCs to the player and open the conversation
    // without activation. After the ambient package system so a fresh
    // install greets on the next tick; before the Late dialogue
    // selection so the opened surface serials once.
    //
    // #5430 — the approach step is AI motion, so it honours the
    // `BYRO_NO_AI_LOCOMOTION` kill switch; the in-radius open does not.
    // M47.3 — `StartConversation player <topic>` requests become directives first.
    scheduler.add_exclusive(Stage::Update, crate::systems::forcegreet::scripted_conversation_system);
    scheduler.add_exclusive(
        Stage::Update,
        crate::systems::forcegreet::make_forcegreet_system(super::ai_locomotion_enabled()),
    );
}
