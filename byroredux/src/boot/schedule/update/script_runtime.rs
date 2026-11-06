//! Reference / alias scripts, quest fragment dispatch, the Papyrus update clock and the effects scripts request (traps, hits, magic, knockdown, flee, requests).
//!
//! A contiguous run of `Stage::Update` registrations (#6359): registration order is the
//! contract, so [`super::register_update_systems`] calls the segments in file order.

use byroredux_core::ecs::Scheduler;

mod reference_scripts;
mod scripted_sound;
mod quest_fragments;
mod script_updates;
mod idles;
mod scripted_physics;
mod archetype_idle;
mod projectiles;
mod hit_events;
mod magic;
mod destruction_and_revival;
mod knockdown_and_flee;
mod script_requests;
mod challenges;

/// Reference / alias scripts, quest fragment dispatch, the Papyrus update clock and the effects scripts request (traps, hits, magic, knockdown, flee, requests).
pub(super) fn register_script_runtime(scheduler: &mut Scheduler) {
    reference_scripts::register_reference_scripts_systems(scheduler);
    scripted_sound::register_scripted_sound_systems(scheduler);
    quest_fragments::register_quest_fragments_systems(scheduler);
    script_updates::register_script_updates_systems(scheduler);
    idles::register_idles_systems(scheduler);
    scripted_physics::register_scripted_physics_systems(scheduler);
    archetype_idle::register_archetype_idle_systems(scheduler);
    projectiles::register_projectiles_systems(scheduler);
    hit_events::register_hit_events_systems(scheduler);
    magic::register_magic_systems(scheduler);
    destruction_and_revival::register_destruction_and_revival_systems(scheduler);
    knockdown_and_flee::register_knockdown_and_flee_systems(scheduler);
    script_requests::register_script_requests_systems(scheduler);
    challenges::register_challenges_systems(scheduler);
}
