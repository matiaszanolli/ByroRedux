//! Water-surface events, water audio, the reverb send and the live audio update.
//!
//! Split out of `late.rs` (#6703); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Access, ActiveCamera, Scheduler, Stage};

/// Water-surface events, water audio, the reverb send and the live audio update.
pub(super) fn register_audio_systems(scheduler: &mut Scheduler) {
    // M44 Phase 6 — cell-acoustics → reverb send (#846). This
    // `build_scheduler` block is the registration authority; the system
    // runs before `audio_system` so any new spatial track constructed this
    // frame picks up the right send level. Already-playing sounds keep their
    // construction-time send (kira contract); long-running ambients across
    // interior/exterior transitions are tracked separately in AUD-D5-NEW-06.
    scheduler.add_to_with_access(
        Stage::Late,
        crate::systems::reverb_zone_system,
        Access::new()
            .reads_resource::<crate::components::CellLightingRes>()
            .writes_resource::<byroredux_audio::AudioWorld>(),
    );
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::systems::make_water_interaction_system(),
        Access::new()
            .reads::<byroredux_core::ecs::components::water::WaterContact>()
            .reads::<byroredux_core::ecs::GlobalTransform>()
            .writes::<byroredux_scripting::SplashEvent>()
            .writes::<byroredux_scripting::RippleEvent>(),
    );
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::systems::water_audio_system,
        Access::new()
            .reads_resource::<crate::components::WaterAudioConfig>()
            .writes_resource::<crate::components::WaterAudioState>()
            .reads_resource::<ActiveCamera>()
            .reads::<byroredux_core::ecs::components::water::SubmersionState>()
            .reads::<byroredux_scripting::SplashEvent>()
            .reads::<byroredux_scripting::RippleEvent>()
            .writes_resource::<byroredux_audio::AudioWorld>(),
    );
    // M44 — the live audio update synchronizes listener pose, applies
    // underwater filtering, dispatches queued and entity-backed one-shots,
    // and prunes finished emitters. It runs in Stage::Late so it sees final
    // world transforms after propagation. `build_scheduler` is the canonical
    // registration authority for this ordering contract.
    //
    // M27 Phase 3 — registered as **exclusive** so it sequences
    // after the Late parallel batch. `camera_follow_system` authors
    // the camera pose this system reads; exclusive sequencing places
    // this after that batch and makes the dependency structural.
    // Side effect: removes two analyzer-visible conflicts
    // (camera_follow ↔ audio on GlobalTransform; reverb_zone ↔
    // audio on AudioWorld) — exclusive systems aren't paired
    // against anything in the access report.
    scheduler.add_exclusive(Stage::Late, byroredux_audio::audio_system);
    // Water audio consumes the markers the interaction bridge wrote; the audio update runs last.
    scheduler.run_after(
        byroredux_core::ecs::system_name(&byroredux_audio::audio_system),
        byroredux_core::ecs::system_name(&crate::systems::water_audio_system),
    );
}
