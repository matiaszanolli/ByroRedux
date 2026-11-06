//! Activation flush and door activation.
//!
//! Split out of `interaction_and_crime.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// Activation flush and door activation.
pub(super) fn register_activation_systems(scheduler: &mut Scheduler) {
    // #2654 — quest fragments queue their `<Ref>.Activate()` targets rather
    // than inserting `ActivateEvent` directly, because
    // `quest_fragment_dispatch` runs *after* most of the consumers below (it
    // has to: it consumes the `QuestStageAdvanced` markers
    // `quest_advance_dispatch` emits) and `event_cleanup_system` drains the
    // marker at Stage::Late the same frame. Flush the queue here, directly
    // after the canonical player-interaction producer, so a fragment
    // activation reaches every consumer exactly once on the following frame.
    // #4712 — that includes `container_loot_system` immediately below: a
    // flush registered after it turned a scripted `ContainerRef.Activate()`
    // or loose-item `Activate()` into a silent no-op.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::fragment_activation_flush_system,
    );
    // #5868 — a scripted / relayed activation of a load door by the player
    // opens it (the E-press path queues its own transition directly).
    scheduler.add_exclusive(Stage::Update, crate::interaction::door_activation_system);
}
