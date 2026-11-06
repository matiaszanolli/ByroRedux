//! ObScript PlayGroup swaps.
//!
//! Split out of `player_animation.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage, World};

/// ObScript PlayGroup swaps.
pub(super) fn register_obscript_anim_systems(scheduler: &mut Scheduler) {
    // #4128 — ObScript `PlayGroup <group> 0` ("let the current cycle finish
    // first"): start the held sequence at the cycle boundary the animation
    // batch just crossed, on the same exclusive tail so the wrap is visible.
    fn pending_group_swap_dispatch(world: &World, _dt: f32) {
        byroredux_scripting::obscript_anim::apply_pending_group_swaps(world)
    }
    scheduler.add_exclusive(Stage::Update, pending_group_swap_dispatch);
}
