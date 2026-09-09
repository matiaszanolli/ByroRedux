//! `World` construction (#3855, split from `boot.rs`): every
//! `insert_resource` and component-storage registration the engine needs
//! before the first frame.

use byroredux_core::animation::AnimationClipRegistry;
use byroredux_core::ecs::{
    DebugStats, DeltaTime, EngineConfig, MetricsSnapshot, SkinCoverageStats, TotalTime, World,
};
use byroredux_core::string::StringPool;

use crate::components::{
    CellRootIndex, FootstepConfig, InputState, NameIndex, SubtreeCache, WaterAudioConfig,
    WaterAudioState,
};
use crate::interaction::{
    ActionBindings, ActionState, InjectedKeyHold, InjectedKeyPulse, InteractionCandidateScratch,
    InteractionState, InteractionTrace,
};
use crate::systems::MetricsState;

/// Phase 1 of construction (#1670) — boot/config plumbing: build the
/// `World` and install every engine resource + pre-registered component
/// storage. Extracted verbatim from the former 581-LOC `App::new`.
pub(crate) fn build_world(debug_mode: bool, args: &[String]) -> World {
    let mut world = World::new();

    // Register built-in resources.
    world.insert_resource(DeltaTime(0.0));
    world.insert_resource(TotalTime(0.0));
    world.insert_resource(EngineConfig {
        debug_logging: debug_mode || cfg!(debug_assertions),
        ..Default::default()
    });
    world.insert_resource(DebugStats::default());
    // #3836 — caches the scene-wide EFFECT_SOFT answer so `build_render_data`
    // stops rescanning every Material and ParticleEmitter each frame. Absent
    // resource degrades to the old per-frame scan, so this is an optimisation,
    // not a requirement.
    world.insert_resource(crate::render::SceneEffectSoftCache::default());
    // #2950 — `pool_regen_tick_system` needs BOTH `PoolRegenConfig` (per-game,
    // inserted when a live `CharacterRuleset` lands) and this accumulator.
    // `try_resource_mut` does not default-insert, so leaving the accumulator to
    // the same future wiring commit would have left the tick returning at its
    // second line forever — registered, declared in `sys.accesses`, and silently
    // dead. The accumulator is game-agnostic (`Default` + `Copy`, a single f32)
    // and a no-op while no config exists, so it is armed unconditionally here
    // and only the per-game config remains outstanding.
    world.insert_resource(byroredux_core::character::PoolRegenAccumulator::default());
    world.insert_resource(byroredux_core::ecs::ScratchTelemetry::default());
    // EX-08 / #2374 — cross-subsystem ownership accounting. `Telemetry` is the
    // latest sample (refreshed on the throttled stats cadence); `Tracker` holds
    // the soak baseline + per-cycle history and stays empty until an operator
    // runs `world.owners baseline`.
    world.insert_resource(byroredux_core::ecs::ImageHealth::default());
    // EX-10/11 / #2371 — exterior LOD residency coverage audit. Refreshed by
    // every `reconcile_lod_rings` call (`streaming_helpers.rs`); starts
    // `sampled = false` so `lod.coverage` reads PENDING before the first
    // reconcile (e.g. an interior-only session, which never streams LOD).
    world.insert_resource(byroredux_core::ecs::LodCoverageStats::default());
    // EX-10/11 item 7 / #2371 — adjacent-loaded-cell terrain-seam agreement
    // audit. Same refresh cadence and PENDING-before-first-sample posture
    // as `LodCoverageStats` immediately above.
    world.insert_resource(byroredux_core::ecs::TerrainSeamStats::default());
    world.insert_resource(byroredux_core::ecs::OwnershipTelemetry::default());
    world.insert_resource(byroredux_core::ecs::OwnershipTracker::new());
    world.insert_resource(byroredux_core::ecs::UpscalerTelemetry::default());
    world.insert_resource(byroredux_core::ecs::PendingUpscalerSwitch::default());
    world.insert_resource(SkinCoverageStats::default());
    world.insert_resource(byroredux_core::ecs::RtIntegrityStats::default());
    // #3305 — shadow-mask census, published alongside RtIntegrityStats.
    world.insert_resource(byroredux_core::ecs::ShadowMaskCensus::default());
    // REND-#1451 — live attenuation tuning, read into the renderer
    // each frame and mutated by the `light.atten` console command.
    // Seed from the config `[defaults]` so a benched knee persists
    // across runs.
    let light_defaults = crate::game_profiles::load_launch_defaults();
    let mut light_tuning = crate::components::LightTuning::default();
    if let Some(knee) = light_defaults.light_atten_knee {
        light_tuning.knee_frac = knee.clamp(0.05, 1.0);
    }
    if let Some(legacy) = light_defaults.light_atten_legacy {
        light_tuning.legacy = legacy;
    }
    world.insert_resource(light_tuning);
    world.insert_resource(crate::components::RenderDebugControl::default());
    // CPU-side per-frame timings — fence_wait / submit_present /
    // etc. Filled by the binary's RedrawRequested handler after
    // each `draw_frame` from the renderer's `FrameTimings`
    // struct. Surfaces the "GPU stall hidden from per-pass
    // timestamps" diagnostic the debug UI's Metrics panel
    // exposed at Phase 7.
    world.insert_resource(byroredux_core::ecs::CpuFrameTimings::default());
    // Per-system wall-time list (Phase 11). Filled by
    // `Scheduler::run` at the end of each invocation, sorted
    // desc. The egui Metrics panel renders the top entries
    // so the operator can see which ECS system dominates
    // `atw_scheduler_ms`.
    //
    // Deliberately NOT inserted unconditionally: the resource's
    // presence is what arms the scheduler's per-system tracker, so
    // inserting it here made every registered system pay an
    // `Instant::now()` + a shared-`Mutex` push every frame for a
    // consumer that samples at ≤ 2 Hz — defeating the #1647 gate
    // outright (PERF-D1-01 / #2166). `BYRO_PROFILE=1` arms it up
    // front for offline profiling runs; otherwise it is inserted
    // lazily by `App` the first time the F3 debug overlay opens.
    if std::env::var_os("BYRO_PROFILE").is_some() {
        world.insert_resource(byroredux_core::ecs::SchedulerSystemTimings::default());
    }
    // Debug-UI sampler state + the aggregated snapshot. Snapshot is
    // empty until `metrics_sample_system` fires its first tick
    // (~500 ms in), at which point CPU / RAM / VRAM / GPU pass
    // times are filled and refreshed at 2 Hz.
    world.insert_resource(MetricsState::default());
    world.insert_resource(MetricsSnapshot::default());
    // Debug-UI load queue. Always present so the debug-server's
    // `LoadNif` / `LoadInteriorCell` / `LoadExteriorCell`
    // handlers can push into it via `try_resource_mut` without
    // structurally inserting. Drained by `App::step_debug_loads`
    // between frames where `&mut World + &mut VulkanContext` are
    // both held.
    world.insert_resource(byroredux_core::ecs::PendingDebugLoadSlot::default());
    // Phase 5 — game profile registry. Loads
    // `assets/debug_profiles.toml` (engine-shipped defaults)
    // plus `~/.byroredux/profiles.toml` (per-user override).
    // Both files missing = empty registry, never an error.
    world.insert_resource(crate::game_profiles::load_default());
    world.insert_resource(byroredux_core::ecs::SelectedRef::default());
    world.insert_resource(InputState::default());
    world.insert_resource(ActionBindings::default());
    world.insert_resource(ActionState::default());
    world.insert_resource(InjectedKeyPulse::default());
    world.insert_resource(InjectedKeyHold::default());
    world.insert_resource(InteractionState::default());
    world.insert_resource(InteractionTrace::default());
    // #3059 — reused per-frame scratch for `collect_candidates`'s
    // FxHashMap instead of reallocating one every frame the crosshair
    // path runs. `collect_candidates` also has a fresh-map fallback for
    // any world that skips this registration (bare test worlds).
    world.insert_resource(InteractionCandidateScratch::default());
    world.insert_resource(crate::combat::CombatState::default());
    // #3709 — per-combatant melee cooldown/blocking, split out of
    // CombatState (a Resource can only ever represent one combatant).
    world.register::<crate::combat::MeleeState>();
    world.insert_resource(crate::combat::PendingDeathReconciliations::default());
    world.insert_resource(StringPool::new());
    // #1212 / D1-NEW-01 — FormIdPool is the intern table backing
    // `FormIdComponent` and `World::find_by_form_id`. Every
    // cell-loaded REFR's placement form-id interns through this
    // pool at spawn time so console (`prid <fid>`), debug-server,
    // and future Papyrus `ObjectReference` lookups all resolve.
    world.insert_resource(byroredux_core::form_id::FormIdPool::new());
    world.insert_resource(AnimationClipRegistry::new());
    world.insert_resource(crate::components::HavokIdleCatalog::default());
    world.insert_resource(NameIndex::new());
    world.insert_resource(SubtreeCache::new());
    world.insert_resource(CellRootIndex::new());
    world.insert_resource(byroredux_physics::PhysicsWorld::new());
    // M28.5 follow-up — engine-wide contact / KCC tunables. Owned
    // as a resource so a single edit propagates through every
    // collider spawn path (Path A NIF imports, character kinematic
    // capsule) and the KCC offset stays in lockstep. Defaults match
    // the pre-unification inline values.
    world.insert_resource(byroredux_physics::ContactConfig::default());
    // WATAL Phase 2 — engine-canonical water-physics constants
    // (buoyancy density ratio + submerged damping). Game-invariant: no
    // game's WATR authors physics params (docs/engine/watal.md §5.3), so
    // one default resource serves every game. The buoyancy phase of
    // `physics_sync_system` reads it; absent it falls back to the same
    // default, so this insert is the single source of truth.
    world.insert_resource(byroredux_physics::PhysicsWaterConstants::default());
    world.insert_resource(byroredux_physics::WaterContactScratch::default());
    world.insert_resource(crate::render::WaterDrawIndexScratch::default());
    // M44 Phase 1 — audio world. Init failure (no audio device,
    // CI, headless server) leaves the inner `AudioManager` as
    // `None` and every subsequent audio operation no-ops. Boot
    // never fails on a missing audio device.
    world.insert_resource(byroredux_audio::AudioWorld::new());
    // M44 Phase 3.5 — footstep config. `default_sound` is None
    // until/unless the cell loader (or a future asset-provider
    // hook) decodes a BSA-archived sound and stores it here.
    // Defaults are safe: `None` makes `footstep_system` no-op.
    world.insert_resource(FootstepConfig::default());
    world.insert_resource(WaterAudioConfig::default());
    world.insert_resource(WaterAudioState::default());
    // M44 Phase 3.5 / #932 — `footstep_system` reuses this Vec<Vec3>
    // scratch across frames instead of allocating a fresh one each
    // tick. Preallocated to capacity 32 to cover typical 5-10 NPC
    // walking case without re-growing.
    world.insert_resource(crate::components::FootstepScratch::default());
    world.insert_resource(crate::components::WaterDisturbanceScratch::default());
    // EX-16 item 5 (#2372) — persistent `--sounds-bsa` handle, shared by
    // FormID-driven REGN ambient dispatch AND the two one-off canonical-
    // path loads immediately below (#3776 — built first and shared rather
    // than each of the three re-parsing `args` on its own, which used to
    // mean the one-off loaders only ever saw the *first* `--sounds-bsa`
    // occurrence despite the flag being documented as repeatable). Empty
    // provider (no flag, or every occurrence failed to open) makes every
    // lookup a clean no-op.
    let sound_archives = crate::asset_provider::build_sound_archive_provider(args);
    // M44 Phase 3.5 — opportunistic footstep BSA load. Decodes the
    // canonical dirt-walk WAV and stashes the `Arc<StaticSoundData>` in
    // `FootstepConfig.default_sound`. Silently no-op when no
    // `--sounds-bsa` archive carries it.
    crate::asset_provider::try_load_default_footstep(&mut world, &sound_archives);
    crate::asset_provider::try_load_default_water_splash(&mut world, &sound_archives);
    world.insert_resource(sound_archives);
    // Process-lifetime cache of parsed-and-imported NIF scenes.
    // Persists across cell transitions so repeat visits don't re-
    // parse every clutter mesh. See #381.
    world.insert_resource(crate::cell_loader::NifImportRegistry::new());

    // #880 / CELL-PERF-02 — companion cache for the hierarchical
    // scene-import path used by NPC spawn (`load_nif_bytes_with_
    // skeleton`). Pre-fix every NPC re-parsed the same skeleton +
    // body + hand NIFs from BSA bytes (~280 redundant parses /
    // Megaton load). Different output shape from `CachedNifImport`
    // — that one is the flat-import variant for REFR placements;
    // this one carries the hierarchical `ImportedScene` with its
    // `nodes: Vec<ImportedNode>` so the bone hierarchy spawns
    // correctly.
    world.insert_resource(crate::scene_import_cache::SceneImportCache::new());

    // Pre-register component storages that the physics sync system
    // queries on the first frame (before anything has been inserted).
    world.register::<byroredux_physics::RapierHandles>();
    // #2873 — registration reads this to file a live actor's ragdoll-bone
    // colliders under `ACTOR_BONE_GROUP` so ground probes skip them. Must
    // exist before the first `collect_newcomers`, which spawns NPCs' bones
    // in the same frame the cell loads.
    world.register::<byroredux_physics::ActorBoneCollider>();
    world.register::<byroredux_physics::ActorColliderOwner>();
    world.register::<byroredux_core::ecs::components::ActorVitals>();
    world.register::<byroredux_core::ecs::components::EquippedWeapon>();
    // #3762 — a creature's authored `CREA.DATA.Damage`, read by
    // `combat::attack_damage`'s no-weapon arm. Registered beside
    // `EquippedWeapon` because they answer the same question.
    world.register::<byroredux_core::ecs::components::CreatureAttack>();
    // MQ101 scene CTDAs read actor death and authored CELL identity through
    // shared sparse components. Register them before any scene/cell exists so
    // condition evaluation safely sees the default alive/unowned state.
    world.register::<byroredux_core::ecs::components::Dead>();
    // #3159 — `Locked` must exist as a storage even when a session's cells
    // authored no XLOC, or the first scripted `Lock(..)` finds no storage to
    // write into and silently no-ops. The cell loader's insert is
    // conditional; this registration is not.
    world.register::<byroredux_core::ecs::components::Locked>();
    // #3299 — actor state carried across ordinary stream-tile eviction.
    world.insert_resource(crate::cell_loader::stream_snapshot::StreamStateSnapshots::default());
    world.register::<byroredux_core::ecs::components::CellFormId>();
    // WATAL Phase 2 — pre-register `WaterContact` so the buoyancy phase's
    // `query_mut::<WaterContact>().insert(..)` succeeds the first time a
    // body enters water (mirrors the `RapierHandles` pre-register).
    world.register::<byroredux_core::ecs::components::water::WaterContact>();
    // M41.x ragdoll — pre-register so the `ragdoll` command's
    // `query_mut::insert` and the writeback system's queries return
    // `Some` even before any actor has been ragdolled.
    world.register::<byroredux_physics::Ragdoll>();
    world.register::<crate::ragdoll::RagdollTemplate>();
    world.register::<crate::ragdoll::RagdollActive>();
    // M44 Phase 3.5: pre-register footstep emitter storage so
    // `footstep_system`'s `query_mut::<FootstepEmitter>` returns
    // `Some` even before the first emitter is inserted (e.g. on
    // startup with no scene loaded).
    world.register::<crate::components::FootstepEmitter>();
    world.register::<crate::components::HavokAnimationTarget>();
    world.register::<byroredux_core::animation::AnimationPlayer>();
    world.register::<byroredux_core::animation::RootMotionDelta>();

    // M42 — pre-register the Sandbox marker storages so
    // `sandbox_seat_system`'s `query_mut::<Seated>().insert(...)` and the
    // `query::<SandboxBehavior>()` skip-scan resolve even before the first
    // actor spawns (the seat guard depends on `Seated` inserts landing).
    world.register::<byroredux_core::ecs::components::SandboxBehavior>();
    world.register::<byroredux_core::ecs::components::Seated>();
    // Claimant-owned seat reservations (pruned per cell-reference load) + the
    // per-cell sit clip handle (set at cell load where the archive provider
    // lives).
    world.insert_resource(crate::components::SeatReservations::default());
    world.insert_resource(crate::components::SandboxSitClip::default());

    // M42.3 — pre-register the Wander marker + runtime-state storages so
    // `wander_system`'s `query::<WanderBehavior>()` skip-scan and
    // `query_mut::<WanderState>().insert(...)` resolve even before the
    // first wandering actor spawns.
    world.register::<byroredux_core::ecs::components::WanderBehavior>();
    world.register::<byroredux_core::ecs::components::WanderState>();

    // M42.4 — pre-register the Travel marker + runtime-state + terminal
    // storages so `travel_system`'s `query::<TravelBehavior>()` skip-scan,
    // `query_mut::<TravelState>().insert(...)`, and
    // `query_mut::<Traveled>().insert(...)` resolve even before the first
    // traveling actor spawns.
    world.register::<byroredux_core::ecs::components::TravelBehavior>();
    world.register::<byroredux_core::ecs::components::TravelState>();
    world.register::<byroredux_core::ecs::components::Traveled>();

    // M42.5 — pre-register the Follow marker + runtime-state storages so
    // `follow_system`'s `query::<FollowBehavior>()` skip-scan and
    // `query_mut::<FollowState>().insert(...)` resolve even before the
    // first following actor spawns.
    world.register::<byroredux_core::ecs::components::FollowBehavior>();
    world.register::<byroredux_core::ecs::components::FollowState>();

    // M42.6 — pre-register the Escort marker + runtime-state + terminal
    // storages so `escort_system`'s `query::<EscortBehavior>()` skip-scan,
    // `query_mut::<EscortState>().insert(...)`, and
    // `query_mut::<Escorted>().insert(...)` resolve even before the first
    // escorting actor spawns.
    world.register::<byroredux_core::ecs::components::EscortBehavior>();
    world.register::<byroredux_core::ecs::components::EscortState>();
    world.register::<byroredux_core::ecs::components::Escorted>();

    // M42.7 — pre-register the Guard marker + runtime-state storages so
    // `guard_system`'s `query::<GuardBehavior>()` skip-scan and
    // `query_mut::<GuardState>().insert(...)` resolve even before the
    // first guarding actor spawns.
    world.register::<byroredux_core::ecs::components::GuardBehavior>();
    world.register::<byroredux_core::ecs::components::GuardState>();

    // M42.8 — pre-register the Patrol marker + runtime-state storages so
    // `patrol_system`'s `query::<PatrolBehavior>()` skip-scan and
    // `query_mut::<PatrolState>().insert(...)` resolve even before the
    // first patrolling actor spawns.
    world.register::<byroredux_core::ecs::components::PatrolBehavior>();
    world.register::<byroredux_core::ecs::components::PatrolState>();
    world.register::<crate::components::AmbientPackageRuntime>();

    // #3319 — EX-16 item 3 Phase 3/4: the single-tile NAVM path cache. Every
    // pathed procedure reads it via `query::<NavPath>()` and writes it via
    // `query_mut::<NavPath>().insert(...)`, and BOTH bail to `None` on a
    // storage that was never registered (`World::query`/`query_mut` open with
    // `self.storages.get(&type_id)?`). Without this line the cache is inert in
    // every shipped build — silently, because the write site is an
    // `if let Some(..)` — so all six pathed procedures re-ran a full A* plus a
    // ~10.6k-triangle localize scan every tick instead of once per goal.
    //
    // `NavmeshTile` needs no equivalent line: it is inserted through
    // `&mut World` (`components::spawn_navmesh_tiles`), which creates its
    // storage on first insert. `NavPath` is only ever inserted through a
    // `query_mut` guard, which cannot.
    world.register::<crate::components::NavPath>();

    // Register scripting component storages.
    byroredux_scripting::register(&mut world);

    // M47.0 Phase 2 — the SCPT `editor_id` → spawner map consulted by
    // `attach_scpt_script` (the pre-Skyrim `SCRI` → `SCPT` →
    // `ScriptRegistry` Obscript path) on every cell load.
    //
    // #2191 — boot no longer seeds it with `papyrus_demo::
    // register_spawners`. That hardcoded registration bound exactly one
    // entry, `defaultRumbleOnActivate`, which M47.2 Phase 0 superseded:
    // `translate::recognizers::rumble` now promotes the same script
    // dynamically off its decompiled `.pex` / parsed `.psc`. The static
    // entry was also provably unreachable — the only route into this
    // registry is an `SCRI`-sourced form id, a sub-record Skyrim+
    // records don't carry, and `defaultRumbleOnActivate` is a
    // Skyrim-era Papyrus script that never appears as an `SCPT`
    // editor_id in Oblivion/FO3/FNV content.
    //
    // The resource itself stays: it is the extension point the
    // pre-Skyrim path resolves against (and `attach_scpt_script` logs
    // an error when it is absent), so downstream crates shipping
    // Obscript translations still have somewhere to register. It is
    // simply empty until the M47.2 `SCTX` Obscript recognizer lands.
    let script_registry = byroredux_scripting::ScriptRegistry::new();
    log::info!(
        "ScriptRegistry initialised with {} editor_id mappings",
        script_registry.len()
    );
    world.insert_resource(script_registry);

    // SDK Phase 3 — engine-owned executable-extension host. A missing or
    // rejected runtime disables code extensions without preventing content or
    // the base engine from starting; the slot retains the attributed reason
    // for future debug/UI reporting.
    let extension_host = crate::extensions::ExtensionHostSlot::initialize_default();
    if let Some(error) = extension_host.init_error() {
        log::error!("executable extension host is disabled: {error}");
    }
    world.insert_resource(extension_host);
    world.insert_resource(crate::extensions::SessionEventQueue::default());

    world
}

#[cfg(test)]
mod ai_storage_registration_tests {
    //! #3319 — every AI runtime-state component the package systems write
    //! through a `query_mut::<T>()` guard MUST be pre-registered in
    //! `build_world`.
    //!
    //! `World::query`/`query_mut` both open with
    //! `self.storages.get(&type_id)?`, so an unregistered storage makes the
    //! read return `None` and the write site — always an `if let Some(..)` —
    //! do nothing at all. There is no panic and no log: the feature is just
    //! silently inert. That is exactly how `NavPath` shipped dead, with all
    //! eight of its NavPath registration calls sitting inside
    //! `#[cfg(test)]` blocks where they only ever made the tests pass.
    //!
    //! A component inserted through `&mut World` (like `NavmeshTile`, via
    //! `spawn_navmesh_tiles`) creates its own storage and is deliberately not
    //! on this list.
    //!
    //! Static source check, matching this file's existing `include_str!`
    //! convention: the live boot path wants a Vulkan device and on-disk game
    //! data, which is out of `cargo test` scope.

    const BOOT_SRC: &str = crate::boot::SOURCES;

    /// Components written via `query_mut` by
    /// `systems/{sandbox,wander,travel,follow,escort,guard,patrol}.rs`,
    /// excluding the engine-wide ones (`Transform`, `GlobalTransform`,
    /// `AnimationPlayer`) that other subsystems already register.
    const AI_WRITE_STORAGES: &[&str] = &[
        "Seated",
        "WanderState",
        "TravelState",
        "Traveled",
        "FollowState",
        "EscortState",
        "Escorted",
        "GuardState",
        "PatrolState",
        "NavPath",
    ];

    #[test]
    fn every_ai_query_mut_storage_is_pre_registered_in_build_world() {
        // Only look at `build_world`'s body, so a `register::<T>()` inside
        // some test module elsewhere in this file cannot satisfy the check.
        // `split` hands back everything after build_world's opening line —
        // including this very test module further down the file. Truncate at
        // the first `#[cfg(test)]` so a `register::<T>()` written in a test
        // (or quoted in a doc comment) cannot satisfy the check; that is the
        // precise mistake #3319 was about in the first place.
        let after_fn = BOOT_SRC
            .split("pub(crate) fn build_world")
            .nth(1)
            .expect("build_world must exist in boot.rs");
        let build_world = after_fn
            .split_once("#[cfg(test)]")
            .map_or(after_fn, |(body, _)| body);

        // Collect the *actual* `register::<Path::To::T>()` calls and reduce
        // each to its final path segment. Substring matching is not good
        // enough here: this module's own doc comment mentions
        // `query_mut::<NavPath>()`, which a naive `::NavPath>()` search
        // would happily accept as proof of registration.
        let registered: Vec<&str> = build_world
            .match_indices("register::<")
            .filter_map(|(i, _)| {
                let rest = &build_world[i + "register::<".len()..];
                let end = rest.find(">()")?;
                Some(rest[..end].rsplit("::").next().unwrap_or(""))
            })
            .collect();

        for component in AI_WRITE_STORAGES {
            assert!(
                registered.contains(component),
                "build_world does not pre-register `{component}`, but an AI system \
                 writes it through `query_mut::<{component}>()`. An unregistered \
                 storage makes that write silently do nothing — see #3319, where \
                 NavPath shipped inert for exactly this reason."
            );
        }
    }
}
