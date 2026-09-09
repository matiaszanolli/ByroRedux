//! `Stage::Late` registrations (#3855, split from `boot.rs`).

use byroredux_core::ecs::{
    Access, ActiveCamera, DebugStats, DeltaTime, MetricsSnapshot, Scheduler, SkinCoverageStats,
    Stage, TotalTime, Transform,
};

use crate::components::{FootstepConfig, InputState};
use crate::systems::{
    footstep_system, log_stats_system, make_billboard_system, metrics_sample_system, MetricsState,
};

/// `Stage::Late` registrations (#3739 split of `build_scheduler`).
pub(super) fn register_late_systems(scheduler: &mut Scheduler) {
    // M28.5 — camera follow runs in Stage::Late, AFTER
    // `physics_sync_system` has settled the kinematic body's
    // post-step pose. Must run BEFORE `audio_system` /
    // `submersion_system` (both read camera GlobalTransform).
    // Both are `Stage::Late` **exclusives**, which sequence after this
    // parallel batch, so the ordering is structural rather than incidental.
    // #3180 — `submersion_system` was in `Stage::PostUpdate` when this
    // comment was written, so the second half of the claim was false; it was
    // moved to a Late exclusive rather than the comment being weakened.
    // The character system writes both Transform and
    // GlobalTransform on the camera to bypass the missing
    // late-stage propagation pass.
    //
    // #3652 — `make_billboard_system` joined that list of Late consumers
    // just below, for the same reason `submersion_system` did (#3180): it
    // used to sit in `Stage::PostUpdate`, an earlier stage, so it read this
    // pose one frame stale in player/third-person mode. This system cannot
    // move the other direction (to PostUpdate) to fix that instead — its
    // whole reason for being in `Stage::Late` is the post-Physics body
    // position in the paragraph above, which PostUpdate runs before Physics
    // even has a chance to produce.
    scheduler.add_to_with_access(
        Stage::Late,
        crate::systems::camera_follow_system,
        Access::new()
            // #2676 / CONC-D3-NEW-02 — the body's very first statement
            // reads `PlayerMode` as an early-out gate. Undeclared, the
            // access analyzer could not see it, so the `Stage::Late`
            // parallel batch's `known_conflict_count() == 0` invariant
            // — the thing that makes cross-thread ABBA structurally
            // unreachable among parallel systems — was computed from an
            // incomplete declaration. No live race today (the only
            // writer, `toggle_player_mode`, takes `&mut World` and so
            // can't run inside the parallel window), but this system
            // writes the camera pose the renderer, audio listener, and
            // `submersion_system` all consume. Same shape of fix as
            // #1787's `ContactConfig` on `physics_sync_system`.
            .reads_resource::<crate::systems::PlayerMode>()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads_resource::<ActiveCamera>()
            .reads_resource::<InputState>()
            .reads::<byroredux_physics::CharacterController>()
            .reads::<byroredux_core::ecs::GlobalTransform>()
            .writes::<byroredux_core::ecs::GlobalTransform>()
            .reads::<Transform>()
            .writes::<Transform>(),
    );
    // #3652 (CONC-D4-2026-08-30-01) — moved here from `Stage::PostUpdate`.
    // Registered exclusive (not in the Late parallel batch above) so it
    // sequences AFTER `camera_follow_system`'s write — exclusives run after
    // the whole parallel batch completes, same structural guarantee
    // `submersion_system` below relies on for the same pose. Entity-disjoint
    // from every other Late exclusive (billboard meshes vs. ragdoll bones /
    // the camera / water volumes), so its position among them is otherwise
    // free; placed first since it's the most direct consumer of what the
    // parallel batch just wrote.
    scheduler.add_exclusive_with_access(
        Stage::Late,
        make_billboard_system(),
        Access::new()
            .reads_resource::<ActiveCamera>()
            .reads_resource::<TotalTime>()
            .reads_resource::<byroredux_core::ecs::components::groundcover::WindField>()
            .reads::<byroredux_core::ecs::Billboard>()
            .reads::<byroredux_core::ecs::SpeedTreeWind>()
            .writes::<byroredux_core::ecs::GlobalTransform>(),
    );
    // M44 Phase 3.5 / #3652 — footstep dispatch. Reads `GlobalTransform` for
    // the world-space spawn position; the only entity that ever carries a
    // `FootstepEmitter` is the active camera (`scene.rs`'s player-spawn
    // path), so in player/third-person mode this is the SAME cross-stage
    // hazard `make_billboard_system` just above was moved here to fix —
    // `camera_follow_system`'s pose, not last frame's. Pre-#848 this was in
    // `Stage::Update`, ahead of PostUpdate propagation, for the same class
    // of staleness one stage earlier; #848 moved it to a PostUpdate
    // exclusive, which fixed the fly-cam case (there `Transform` is written
    // in `Stage::Update` and PostUpdate propagation resolves it same-frame)
    // but left the player-mode case exactly as stale as billboard's, just
    // quieter (a spatial-audio trigger position a few cm off, versus a
    // visibly wrong facing) — an oversight, never an accepted trade-off
    // like `particle_system`'s #3653 comment above documents. Also gains a
    // declared Access here for the first time (previously a bare
    // `add_exclusive`, unlike every neighboring exclusive in this file).
    scheduler.add_exclusive_with_access(
        Stage::Late,
        footstep_system,
        Access::new()
            .reads_resource::<FootstepConfig>()
            .writes_resource::<crate::components::FootstepScratch>()
            .writes_resource::<byroredux_audio::AudioWorld>()
            .reads::<byroredux_core::ecs::GlobalTransform>()
            .writes::<crate::components::FootstepEmitter>(),
    );
    // M41.x — ragdoll writeback. Stage::Late guarantees it runs after
    // `physics_sync_system` (Stage::Physics) has stepped the multibody
    // *and* after PostUpdate transform propagation, so overwriting each
    // ragdoll bone's GlobalTransform with the simulated pose is the last
    // word before render (no propagation/animation skip needed).
    //
    // Registered **exclusive** (not in the Late parallel batch): it
    // writes GlobalTransform, and so does `camera_follow_system` in the
    // same stage — declaring both in the parallel batch is a WriteWrite
    // conflict (#1601). They write entity-disjoint sets (camera entity
    // vs. ragdoll bones), so ordering is irrelevant; exclusive sequencing
    // keeps the scheduler's known_conflict_count() at 0. Matches the
    // existing add_exclusive treatment of audio_system / event_cleanup.
    scheduler.add_exclusive(Stage::Late, crate::ragdoll::ragdoll_writeback_system);
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
            .reads::<byroredux_core::ecs::GlobalTransform>()
            .writes::<byroredux_core::ecs::components::ParticleEmitter>()
            .writes::<byroredux_scripting::RippleEvent>()
            .writes::<byroredux_scripting::SplashEvent>()
            .writes::<byroredux_core::ecs::components::SubmersionState>(),
    );
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
            .writes::<byroredux_core::ecs::components::Dead>(),
    );
    // Water hazards and drowning can mark actors dead from different stages.
    // Reconcile their structural consequences only after both producers have
    // run, in this exclusive lane where AI removal and ragdoll activation are
    // legal. This is the same reconciler combat and save-load use (#3119).
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::combat::reconcile_pending_dead_actors_system,
        Access::new()
            .writes_resource::<crate::combat::PendingDeathReconciliations>()
            .reads::<byroredux_core::ecs::components::Dead>(),
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
    // after the Late parallel batch. The ordering comment at
    // line 650-656 above ("MUST run BEFORE audio_system" /
    // "Must run BEFORE audio_system") encodes a real
    // dependency that the parallel batch can't guarantee on its
    // own; exclusive sequencing makes the dependency structural.
    // Side effect: removes two analyzer-visible conflicts
    // (camera_follow ↔ audio on GlobalTransform; reverb_zone ↔
    // audio on AudioWorld) — exclusive systems aren't paired
    // against anything in the access report.
    scheduler.add_exclusive(Stage::Late, byroredux_audio::audio_system);
    scheduler.add_to_with_access(
        Stage::Late,
        log_stats_system,
        Access::new()
            .reads_resource::<TotalTime>()
            .reads_resource::<DeltaTime>()
            .reads_resource::<DebugStats>()
            // #2389 / ECS-D5-01 — the `want_breakdown` arm of the body
            // reads two more resources (`systems/debug.rs`). The runtime
            // gate is invisible to the analyzer, so both must be declared
            // unconditionally — same shape as #1787's `ContactConfig`.
            .reads_resource::<SkinCoverageStats>()
            .reads_resource::<byroredux_core::ecs::CpuFrameTimings>(),
    );
    // Debug-UI metrics sampler — throttles itself to ~2 Hz, so the
    // per-frame cost is a single resource read + compare. On a
    // sample tick it walks sysinfo + the gpu-allocator block list
    // and writes the snapshot read by the protocol / TUI / egui
    // overlay.
    scheduler.add_to_with_access(
        Stage::Late,
        metrics_sample_system,
        Access::new()
            .reads_resource::<TotalTime>()
            .reads_resource::<SkinCoverageStats>()
            .reads_resource::<byroredux_renderer::vulkan::allocator::AllocatorResource>()
            .reads_resource::<byroredux_renderer::vulkan::allocator::GpuMemoryBudget>()
            // #2389 / ECS-D5-01 — the sample tick also reads the CPU
            // frame timings and the per-system scheduler timings
            // (`systems/metrics.rs`); both were missing. The latter is
            // the likeliest future write target in this stage (an
            // in-ECS profiler), which is exactly the pairing the
            // analyzer would have waved through.
            .reads_resource::<byroredux_core::ecs::CpuFrameTimings>()
            .reads_resource::<byroredux_core::ecs::SchedulerSystemTimings>()
            .writes_resource::<MetricsState>()
            .writes_resource::<MetricsSnapshot>(),
    );
    // Custom events committed on the previous frame are drained before any
    // callback in this frame can publish another one. This is the scheduler
    // boundary that prevents nested/reentrant guest execution.
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::extensions::extension_engine_settings_sync_system,
        Access::new()
            .reads_resource::<byroredux_core::settings::SettingsRegistry>()
            .writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::extensions::extension_content_catalog_sync_system,
        Access::new()
            .reads_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>()
            .writes_resource::<byroredux_scripting::LegacyObscriptContentCatalog>()
            .writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::extensions::extension_input_bindings_sync_system,
        Access::new()
            .reads_resource::<crate::interaction::ActionBindings>()
            .writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::extensions::extension_player_entity_sync_system,
        Access::new()
            .reads_resource::<crate::systems::PlayerEntity>()
            .writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
    // Run translated OBSE/xNVSE load-order handlers after their shared live
    // catalog snapshot is published and before transient OnLoad/OnActivate
    // markers are drained. The program component is static translated data;
    // numeric results land in the existing save-backed ScriptVariables.
    scheduler.add_exclusive_with_access(
        Stage::Late,
        byroredux_scripting::legacy_obscript_load_order_system,
        Access::new()
            .reads_resource::<byroredux_scripting::LegacyObscriptContentCatalog>()
            // #3951 — the host-function invoker was missing. These
            // declarations do not affect scheduling today (the analyzer only
            // walks parallel-stage pairs, and exclusives run serially), so
            // this is not a deadlock fix; their stated purpose (#3473) is to
            // be the thing compared against if either system is ever
            // promoted to a parallel lane, and an under-declaration defeats
            // exactly that.
            .reads_resource::<byroredux_scripting::ExtensionScriptFunctionInvoker>()
            .reads::<byroredux_scripting::LegacyObscriptProgram>()
            .reads::<byroredux_scripting::OnCellLoadEvent>()
            .reads::<byroredux_scripting::ActivateEvent>()
            .writes::<byroredux_scripting::ScriptVariables>(),
    );
    // Manifest-published `Provider.Function(...)` calls use the same live host
    // as ObScript, but their static Papyrus/PEX program is snapshotted before
    // guest entry so no ECS guard crosses the sandbox boundary.
    scheduler.add_exclusive_with_access(
        Stage::Late,
        byroredux_scripting::papyrus_provider_system,
        Access::new()
            .reads_resource::<byroredux_scripting::PapyrusProviderRuntime>()
            // #3951 — the continuation queue and the mod-event runtime are
            // `resource_mut`s, the form pool and six further event/identity
            // component storages are reads, and none of them were declared:
            // the body acquires thirteen types where this list named four.
            // See the sibling above for why that matters even though these
            // declarations do not currently affect scheduling.
            .writes_resource::<byroredux_scripting::PapyrusProviderContinuationQueue>()
            .writes_resource::<byroredux_scripting::PapyrusModEventRuntime>()
            .reads_resource::<byroredux_core::form_id::FormIdPool>()
            .reads::<byroredux_scripting::PapyrusProviderProgram>()
            .reads::<byroredux_scripting::OnCellLoadEvent>()
            .reads::<byroredux_scripting::ActivateEvent>()
            .reads::<byroredux_scripting::OnInitEvent>()
            .reads::<byroredux_scripting::HitEvent>()
            .reads::<byroredux_scripting::EquipmentEventBatch>()
            .reads::<byroredux_scripting::OnTriggerEnterEvent>()
            .reads::<byroredux_scripting::OnUpdateEvent>()
            .reads::<byroredux_core::ecs::components::FormIdComponent>()
            .writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::extensions::extension_custom_event_dispatch_system,
        Access::new().writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
    // SDK Phase 3 — deliver activation only after every built-in consumer has
    // run and immediately before the transient marker is drained. The adapter
    // snapshots `ActivateEvent` and the cloneable host slot, drops both ECS
    // guards, then enters untrusted code and atomically commits own-state
    // commands. `writes_resource` describes the logical mutation behind the
    // slot's Arc<Mutex<_>>, even though no ECS resource guard spans the call.
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::extensions::extension_activation_dispatch_system,
        Access::new()
            .reads::<byroredux_scripting::ActivateEvent>()
            .reads::<byroredux_core::ecs::components::FormIdComponent>()
            .reads::<byroredux_core::ecs::components::Name>()
            .reads::<byroredux_core::ecs::components::GlobalTransform>()
            .reads::<byroredux_core::ecs::components::Transform>()
            .reads_resource::<byroredux_scripting::papyrus_demo::PapyrusPlayerEntity>()
            .reads_resource::<byroredux_core::form_id::FormIdPool>()
            .reads_resource::<byroredux_core::string::StringPool>()
            .writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::extensions::extension_cell_load_dispatch_system,
        Access::new()
            .reads::<byroredux_scripting::OnCellLoadEvent>()
            .reads::<byroredux_core::ecs::components::FormIdComponent>()
            .reads::<byroredux_core::ecs::components::Name>()
            .reads::<byroredux_core::ecs::components::GlobalTransform>()
            .reads::<byroredux_core::ecs::components::Transform>()
            .reads_resource::<byroredux_core::form_id::FormIdPool>()
            .reads_resource::<byroredux_core::string::StringPool>()
            .writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::extensions::extension_equipment_dispatch_system,
        Access::new()
            .reads::<byroredux_scripting::EquipmentEventBatch>()
            .reads::<byroredux_core::ecs::components::FormIdComponent>()
            .reads::<byroredux_core::ecs::components::Name>()
            .reads::<byroredux_core::ecs::components::GlobalTransform>()
            .reads::<byroredux_core::ecs::components::Transform>()
            .reads_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>()
            .reads_resource::<byroredux_core::form_id::FormIdPool>()
            .reads_resource::<byroredux_core::string::StringPool>()
            .writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::extensions::extension_input_dispatch_system,
        Access::new()
            .reads_resource::<crate::interaction::ActionState>()
            .writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::extensions::extension_session_dispatch_system,
        Access::new()
            .writes_resource::<crate::extensions::SessionEventQueue>()
            .writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::extensions::extension_hit_dispatch_system,
        Access::new()
            .reads::<byroredux_scripting::HitEvent>()
            .reads::<byroredux_core::ecs::components::FormIdComponent>()
            .reads::<byroredux_core::ecs::components::Name>()
            .reads::<byroredux_core::ecs::components::GlobalTransform>()
            .reads::<byroredux_core::ecs::components::Transform>()
            .reads_resource::<byroredux_core::form_id::FormIdPool>()
            .reads_resource::<byroredux_core::string::StringPool>()
            .writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::extensions::extension_update_dispatch_system,
        Access::new().writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::extensions::extension_setting_write_apply_system,
        Access::new()
            .writes_resource::<byroredux_core::settings::SettingsRegistry>()
            .reads_resource::<crate::settings_io::SettingsPersistence>()
            .writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
    scheduler.add_exclusive(Stage::Late, byroredux_scripting::event_cleanup_system);
}
