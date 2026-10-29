//! `Stage::Update` registrations (#3855, split from `boot.rs`).
//!
//! The largest stage, and the one carrying the nested dispatch shims that
//! adapt per-game runtimes to the scheduler's `fn(&World, f32)` shape.

use byroredux_core::ecs::{Access, ActiveCamera, Scheduler, Stage, Transform, World};
use byroredux_core::string::StringPool;

/// Reached as `super::ai_locomotion_enabled()` by `quest_scene`: the `mod.rs` source scan pins that spelling.
use super::ai_locomotion_enabled;

use crate::interaction::{
    ActionState, InteractionCandidateScratch, InteractionState, InteractionTrace,
};
use crate::systems::{
    animate_lights_system, loading_model_turntable_system, make_animation_system, spin_system,
};

mod combat;
mod interaction_and_crime;
mod player_animation;
mod quest_scene;
mod script_runtime;

use combat::register_combat;
use interaction_and_crime::register_interaction_and_crime;
use player_animation::register_player_and_animation;
use quest_scene::register_quest_story_scene_and_packages;
use script_runtime::register_script_runtime;

/// `Stage::Update` registrations (#3739 split of `build_scheduler`; #6359 split into ordered segments).
///
/// Registration order is the contract (producer before consumer), so the segments run in this order and
/// each is a contiguous run of the original sequence.
pub(super) fn register_update_systems(scheduler: &mut Scheduler) {
    register_interaction_and_crime(scheduler);
    register_combat(scheduler);
    register_quest_story_scene_and_packages(scheduler);
    register_script_runtime(scheduler);
    register_player_and_animation(scheduler);
}
