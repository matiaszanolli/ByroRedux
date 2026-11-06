//! Papyrus-demo dispatchers, quest / story / scene playback, package fragments, and the ObScript quest and dialogue lanes.
//!
//! A contiguous run of `Stage::Update` registrations (#6359): registration order is the
//! contract, so [`super::register_update_systems`] calls the segments in file order.

use byroredux_core::ecs::Scheduler;
use super::ai_locomotion_enabled;

mod rumble;
mod trigger_volumes;
mod quest_advance;
mod story_manager;
mod quest_aliases;
mod scene_playback;
mod package_fragments;
mod forcegreet;
mod obscript_quests;
mod scene_actions;
mod obscript_objects;
mod dialogue_speech;

/// Papyrus-demo dispatchers, quest / story / scene playback, package fragments, and the ObScript quest and dialogue lanes.
pub(super) fn register_quest_story_scene_and_packages(scheduler: &mut Scheduler) {
    rumble::register_rumble_systems(scheduler);
    trigger_volumes::register_trigger_volumes_systems(scheduler);
    quest_advance::register_quest_advance_systems(scheduler);
    story_manager::register_story_manager_systems(scheduler);
    quest_aliases::register_quest_aliases_systems(scheduler);
    scene_playback::register_scene_playback_systems(scheduler);
    package_fragments::register_package_fragments_systems(scheduler);
    forcegreet::register_forcegreet_systems(scheduler);
    obscript_quests::register_obscript_quests_systems(scheduler);
    scene_actions::register_scene_actions_systems(scheduler);
    obscript_objects::register_obscript_objects_systems(scheduler);
    dialogue_speech::register_dialogue_speech_systems(scheduler);
}
