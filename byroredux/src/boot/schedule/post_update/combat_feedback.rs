//! Combat feedback.
//!
//! Split out of `post_update.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// Combat feedback.
pub(super) fn register_combat_feedback_systems(scheduler: &mut Scheduler) {
    // P2 combat tail — combat feedback (attack/hit/death clip takes +
    // combat sounds), AFTER walk_anim when it is registered: a death take
    // must be installed after walk_anim's abandon pass (Dead cedes
    // playback), and a mid-walk stagger relies on walk_anim's yield rule
    // having already given up ownership this frame. See
    // `systems::combat_anim` docs.
    //
    // #4709 — deliberately OUTSIDE the `locomotion_enabled` block: combat
    // feedback is not AI motion, and isolating motion with
    // `BYRO_NO_AI_LOCOMOTION=1` must not silently mute combat takes and
    // sounds. `locomotion_kill_switch_gates_exactly_the_motion_systems`
    // pins the block's contents.
    scheduler.add_exclusive(
        Stage::PostUpdate,
        crate::systems::make_combat_feedback_system(),
    );
}
