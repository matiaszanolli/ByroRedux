//! `Stage::Late` registrations (#3855, split from `boot.rs`).

use byroredux_core::ecs::Scheduler;

mod audio;
mod camera;
mod diagnostics;
mod equipment_appearance;
mod extension_dispatch;
mod extension_events;
mod extension_sync;
mod footsteps;
mod npc_dialogue;
mod ragdoll;
mod script_cleanup;
mod script_hosts;
mod scripted_combat;
mod water_hazards;

/// `Stage::Late` registrations (#3739 split of `build_scheduler`).
pub(super) fn register_late_systems(scheduler: &mut Scheduler) {
    camera::register_camera_systems(scheduler);
    footsteps::register_footstep_systems(scheduler);
    ragdoll::register_ragdoll_systems(scheduler);
    water_hazards::register_water_hazard_systems(scheduler);
    scripted_combat::register_scripted_combat_systems(scheduler);
    audio::register_audio_systems(scheduler);
    diagnostics::register_diagnostic_systems(scheduler);
    extension_sync::register_extension_sync_systems(scheduler);
    script_hosts::register_script_host_systems(scheduler);
    extension_events::register_extension_event_systems(scheduler);
    equipment_appearance::register_equipment_appearance_systems(scheduler);
    npc_dialogue::register_npc_dialogue_systems(scheduler);
    extension_dispatch::register_extension_dispatch_systems(scheduler);
    script_cleanup::register_script_cleanup_systems(scheduler);
}
