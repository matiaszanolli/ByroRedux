//! Submersion detection and water damage.
//!
//! Split out of `late.rs` (#6703); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Access, ActiveCamera, Scheduler, Stage};

/// Submersion detection and water damage.
pub(super) fn register_water_hazard_systems(scheduler: &mut Scheduler) {
    // Submersion detection. #3180 — this was registered in
    // `Stage::PostUpdate`, whose placement comment claimed the camera's
    // GlobalTransform was "already current for the frame". That held only in
    // fly-cam mode, where the fly camera writes `Transform` in `Stage::Update`
    // and PostUpdate propagation resolves it. In player / third-person mode
    // `camera_follow_system` is the pose author and writes both `Transform`
    // and `GlobalTransform` in `Stage::Late`, so a PostUpdate read saw the
    // PREVIOUS frame's pose — one frame of lag on the underwater low-pass and
    // the underwater composite tint, and a `camera_follow_system` comment
    // asserting an ordering that did not hold.
    //
    // Registered here as a `Stage::Late` exclusive: exclusives sequence after
    // the Late parallel batch, which contains `camera_follow_system`, so the
    // camera pose is this frame's in BOTH modes. Placed immediately after
    // `ragdoll_writeback_system` and before `water_damage_system` to preserve
    // the documented Late exclusive order (ragdoll -> submersion ->
    // water_damage -> water_interaction -> water_audio -> audio_system ->
    // event_cleanup), keeping `SubmersionState` and the Splash/Ripple markers
    // written before `water_audio_system` consumes them.
    //
    // Reads `WaterPlane`/`WaterVolume` + `GlobalTransform`, writes
    // `SubmersionState` on the active camera entity.
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::systems::submersion_system,
        Access::new()
            .reads_resource::<ActiveCamera>()
            .reads_resource::<byroredux_core::ecs::resources::TotalTime>()
            .reads_resource::<byroredux_core::ecs::components::groundcover::WindField>()
            .reads::<byroredux_core::ecs::components::WaterPlane>()
            .reads::<byroredux_core::ecs::components::WaterVolume>()
            .reads::<byroredux_core::ecs::components::WaterSurfaceMesh>()
            .reads::<byroredux_core::ecs::components::water::WaterBedField>()
            .reads::<byroredux_core::ecs::GlobalTransform>()
            .writes::<byroredux_core::ecs::components::ParticleEmitter>()
            .writes::<byroredux_scripting::RippleEvent>()
            .writes::<byroredux_scripting::SplashEvent>()
            .writes::<byroredux_core::ecs::components::SubmersionState>(),
    );
    // WATAL — bridge physics-derived `WaterContact` into transient surface
    // interaction markers after the Physics stage has written body poses and
    // before water audio consumes the events. The physics crate stays free of
    // scripting/presentation dependencies; this is the canonical adapter.
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::systems::water_damage_system,
        Access::new()
            .reads::<byroredux_core::ecs::components::water::WaterContact>()
            .reads::<byroredux_core::ecs::components::ActorVitals>()
            .reads::<byroredux_core::ecs::components::Dead>()
            .writes_resource::<crate::combat::PendingDeathReconciliations>()
            .writes::<byroredux_core::ecs::components::ActorValues>()
            // M47.2 — spare_from_death: essential / protected actors bleed out.
            .reads::<byroredux_scripting::ActorProtection>()
            .reads::<byroredux_scripting::SceneAliasCandidate>()
            .reads_resource::<byroredux_scripting::ActorBaseProtectionOverrides>()
            .writes::<byroredux_scripting::Bleedout>()
            .reads_resource::<byroredux_scripting::BleedoutRules>()
            .reads_resource::<byroredux_scripting::SceneQuestAliasRegistry>()
            .reads_resource::<byroredux_scripting::SceneActorBindings>()
            .reads_resource::<byroredux_scripting::quest_stages::QuestStageState>()
            .writes::<byroredux_core::ecs::components::Dead>(),
    );
    // The documented Late exclusive order: ragdoll -> submersion -> water_damage -> ...
    scheduler.run_after(
        byroredux_core::ecs::system_name(&crate::systems::submersion_system),
        byroredux_core::ecs::system_name(&crate::ragdoll::ragdoll_writeback_system),
    );
    scheduler.run_after(
        byroredux_core::ecs::system_name(&crate::systems::water_damage_system),
        byroredux_core::ecs::system_name(&crate::systems::submersion_system),
    );
}
