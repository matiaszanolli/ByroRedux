//! Extracted from `save_io.rs`'s inline `mod tests` (#2407 / TD1-004).
//! Production code there is ~1030 LOC; the test bulk alone pushed the
//! file past 3000. Split by topic, contents unchanged.

use super::*;

/// Recursively collect every `.rs` file under `dir`. Panics on an
/// unreadable directory — a moved/renamed scan root should fail loud,
/// not silently scan nothing (same posture as the `SAVE_TYPE_SOURCES`
/// guard in the sibling `serde_default_guard_tests`).
fn collect_rs_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| {
        panic!(
            "SAVE-D1-12 guard can't read directory {} ({e}); a scan root \
             moved — see discover_scan_roots.",
            dir.display()
        )
    });
    for entry in entries {
        let path = entry
            .expect("SAVE-D1-12 guard: unreadable dir entry")
            .path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// #3497 (SAVE-D1-2026-08-27-03) — discovered, not hardcoded. The guard
/// used to scan a fixed `SCAN_ROOTS: &[&str]` list; that has a strong
/// defence against a root *moving* (`collect_rs_files`'s panic above, and
/// the `!found.is_empty()` assert below) but none against a root that was
/// never added — `crates/sdk` shipped with a live `impl Resource for
/// StudioSession` and sat unscanned until this fix, silently narrowing
/// what the ledger actually covers relative to the workspace.
///
/// Enumerates every `crates/*/src` directory from the workspace root, plus
/// `byroredux/src` itself, so a new crate is scanned automatically the
/// moment it exists — no second commit has to remember to widen a list.
/// [`NOT_SCANNED`] is the deliberate escape hatch for a crate that must be
/// excluded for a real structural reason; today nothing needs it; every
/// crate's `impl Component`/`impl Resource` surface is either registered
/// or carries a `NOT_SAVED_BY_DESIGN` entry, so there is no blind spot
/// left to open by omission.
fn discover_scan_roots(manifest: &std::path::Path) -> Vec<std::path::PathBuf> {
    let workspace_crates = manifest.join("../crates");
    let mut roots = vec![manifest.join("src")];
    let entries = std::fs::read_dir(&workspace_crates).unwrap_or_else(|e| {
        panic!(
            "SAVE-D1-12 guard can't read {} ({e}); the workspace crates/ \
             directory moved — update discover_scan_roots.",
            workspace_crates.display()
        )
    });
    for entry in entries {
        let path = entry
            .expect("SAVE-D1-12 guard: unreadable crates/ entry")
            .path();
        if !path.is_dir() {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if NOT_SCANNED.iter().any(|(excluded, _)| *excluded == name) {
            continue;
        }
        let src = path.join("src");
        if src.is_dir() {
            roots.push(src);
        }
    }
    roots.sort();
    roots
}

/// See [`discover_scan_roots`]. Empty today — kept as the named place a
/// future genuinely-must-exclude crate goes, with a reason, rather than
/// silently dropping out of the enumeration.
const NOT_SCANNED: &[(&str, &str)] = &[];

/// #3497 — pins the discovery mechanism itself, not just its downstream
/// effect. `crates/sdk` is the crate that shipped unscanned (`21a840d5`)
/// and prompted this fix; a hardcoded-list regression (someone reverting
/// to `SCAN_ROOTS` and forgetting a crate again) fails here directly
/// rather than only showing up as a missing allowlist entry two tests
/// away.
#[test]
fn discover_scan_roots_finds_every_workspace_crate_and_byroredux() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let roots = discover_scan_roots(manifest);

    let has_suffix = |suffix: &str| {
        roots
            .iter()
            .any(|r| r.to_string_lossy().replace('\\', "/").ends_with(suffix))
    };
    assert!(
        has_suffix("crates/sdk/src"),
        "the crate that motivated #3497 must be discovered: {roots:?}"
    );
    assert!(
        has_suffix("byroredux/src"),
        "byroredux's own src must always be scanned: {roots:?}"
    );
    // A handful of longstanding crates, as a sanity check that discovery
    // isn't accidentally scoped to only the newest addition.
    for known in [
        "crates/core/src",
        "crates/scripting/src",
        "crates/plugin/src",
    ] {
        assert!(has_suffix(known), "{known} missing from {roots:?}");
    }
}

/// Extract `X` from a `impl Component for X` / `impl Resource for X`
/// line (leading whitespace stripped first — every real impl in this
/// tree sits at module level with no indentation, but this tolerates
/// one anyway). Returns `None` for a non-matching line. A generic type
/// (`impl Component for Foo<T>`) would capture just `Foo`, which is
/// fine — no generic Component/Resource impl exists in the scanned
/// directories today.
fn impl_target_type(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let rest = trimmed
        .strip_prefix("impl Component for ")
        .or_else(|| trimmed.strip_prefix("impl Resource for "))?;
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    (end > 0).then(|| &rest[..end])
}

/// #2295 (SAVE-D1-12) — registry-completeness guard, generalized past
/// the NPC-spawn-stamped surface
/// `npc_spawn_stamped_components_are_saved_or_intentionally_rederived`
/// (sibling `round_trip_tests`) covers. Scans every
/// `.rs` file under the three directories where gameplay-relevant ECS
/// state is defined for a top-level `impl Component for X` / `impl
/// Resource for X` line, and requires each `X` found to be EITHER
/// registered in [`build_save_registry`] OR listed in
/// [`NOT_SAVED_BY_DESIGN`] with a one-line reason — never both, never
/// neither. Same manually-maintained-allowlist philosophy as
/// `REDERIVED_NOT_SAVED` above and `MUTABLE_DELTA_COLUMNS`'s `AUDITED`
/// tripwire; static source scan, not reflection (Rust has none).
///
/// This guard exists because the NPC-spawn-stamped guard has zero
/// visibility into components a *system* inserts later during gameplay
/// (script recognition, condition evaluation, package/scene execution)
/// — precisely the class `TwoStateActivator`/`ScriptVariables`/
/// `ActorControlState`/`Dead` all belong to (#2291/#1834/#2292/#2293).
/// Building this guard's allowlist (2026-08-05) surfaced 7 genuine,
/// previously-untracked save gaps on the first pass — filed as
/// #2378-#2382 rather than fixed inline here, since each needs its own
/// per-field delta-safety review before registration (matching the
/// care #1834/#2291/#2292 each took). They're allowlisted below with a
/// `KNOWN GAP` reason, not silently blessed as safe, so this guard
/// passes today without hiding the debt it found.
#[test]
fn every_component_or_resource_impl_is_saved_or_explicitly_allowlisted() {
    /// A type is safe to omit from `build_save_registry` when either:
    /// - it's write-once at spawn/NIF-import/cell-load from static
    ///   ESM/NIF data, so a reload deterministically re-derives it
    ///   (the `REDERIVED_NOT_SAVED` / `*Behavior`-vs-`*State` pattern);
    /// - it's GPU/physics/render-derived, rebuilt from other
    ///   already-saved state every load or every frame (the
    ///   `GlobalTransform`/`MeshHandle`/`TextureHandle` class
    ///   `build_save_registry`'s own doc already names);
    /// - it's a one-shot event/request/command/batch marker drained
    ///   every frame (transient, per `build_save_registry`'s doc);
    /// - it's forward-latent: no live (non-test) insertion site exists
    ///   in the codebase today, so nothing is lost by a save/load —
    ///   same posture as `Dead` (#2293). Register the moment a real
    ///   system starts inserting it.
    /// - it's a KNOWN GAP: a real runtime mutator exists with no
    ///   reload re-derivation path, tracked by a filed issue and
    ///   deliberately NOT fixed in the same commit that discovered it.
    ///
    /// Verified 2026-08-05 against real (non-`#[cfg(test)]`) insertion
    /// sites for every entry — see the individual reasons.
    const NOT_SAVED_BY_DESIGN: &[(&str, &str)] = &[
        // ── crates/core/src outside ecs/components/ (#3166) ───────
        ("StringPool", "serialized specially as Snapshot.strings so FixedString symbol order is restored before component columns"),
        ("CommandRegistry", "boot-time function-pointer command table; reconstructed by command registration and not serializable gameplay state"),
        ("RootMotionDelta", "per-frame animation output consumed and cleared by movement systems"),
        ("AnimationClipRegistry", "asset registry rebuilt from NIF/KF/KFM content; numeric handles are session-local"),
        ("AnimationController", "KFM-derived controller catalog with session-local clip handles; playback requests are transient and the catalog is rebuilt with the actor"),
        ("SettingsRegistry", "user preferences persist independently in settings.toml and are installed before scene setup"),
        ("AfflictionStatus", "forward-latent: affliction_tick_system has no production scheduler registration; classify as gameplay state when activated"),
        ("CharacterRuleset", "immutable game-profile rules selected at boot from the source game"),
        ("MeleeDamageConfig", "immutable Fallout combat tuning selected from the game profile at boot"),
        ("CharacterLevel", "known progression gap guarded by validate_progression_state: saves are refused once non-default XP/level state exists (#2947)"),
        ("Perks", "stamped verbatim from NPC_.PRKR at spawn with no production mutator anywhere (npc_spawn.rs only; nothing calls set_rank/try_set_rank outside #[cfg(test)]) — NOT guarded by validate_progression_state, which inspects only CharacterLevel; register it the moment an AddPerk-style effect or perk-selection UI lands (#3491, corrects the #2947 cross-reference this reason previously shared with CharacterLevel)"),
        ("Background", "derived character-creation metadata with no live production mutator; re-created with the actor"),
        ("FactionReputation", "forward-latent: no production insertion or mutation site exists yet"),
        ("PoolRegenAccumulator", "fractional fixed-step carry only; canonical pool values live in saved ActorValues and carry is safely re-seeded"),
        ("PoolRegenConfig", "immutable game-profile regeneration tuning installed at boot"),
        ("MetricsSnapshot", "diagnostic telemetry sampled from current storages each frame"),
        ("GameProfileRegistry", "static per-game profile table installed at boot"),
        ("SystemList", "scheduler introspection snapshot rebuilt from the installed scheduler at boot"),
        ("SchedulerAccessReport", "scheduler access diagnostics rebuilt from system declarations at boot"),
        ("ScreenshotBridge", "one-shot renderer/debug-server screenshot handoff"),
        ("DepthCaptureBridge", "one-shot renderer/console depth-capture handoff (#3308); holds Arcs into renderer-owned staging state that does not survive a device teardown, and a captured depth field describes one frame's camera pose rather than any world state"),
        ("DeltaTime", "per-frame scheduler input overwritten from the current frame clock"),
        ("TotalTime", "process-session elapsed time used for animation/effects, restarted rather than persisted"),
        ("EngineConfig", "boot/CLI engine configuration, not mutable gameplay state"),
        ("DebugStats", "per-frame diagnostic counters"),
        ("ScratchTelemetry", "per-frame scratch-allocation telemetry"),
        ("UpscalerTelemetry", "renderer telemetry rebuilt every frame"),
        ("CpuFrameTimings", "per-frame performance telemetry"),
        ("SkinCoverageStats", "renderer coverage telemetry recomputed from current draws"),
        ("ImageHealth", "swapchain/frame health telemetry"),
        ("RtIntegrityStats", "ray-tracing integrity telemetry recomputed from renderer state"),
        ("LodCoverageStats", "LOD diagnostic telemetry recomputed from resident content (#3166)"),
        ("TerrainSeamStats", "terrain diagnostic telemetry recomputed from resident tiles"),
        ("SelectedRef", "debug-console selection using session-local entity identity"),
        ("SkinSlotPool", "GPU bone-palette allocation bookkeeping rebuilt as skinned meshes spawn"),
        ("OwnershipTracker", "derived entity-ownership index rebuilt by spawn/stamp paths"),
        ("OwnershipTelemetry", "diagnostic counters derived from OwnershipTracker"),
        ("PendingDebugLoadSlot", "one-shot debug load request drained between frames"),
        ("PendingUpscalerSwitch", "one-shot renderer configuration request"),
        ("SchedulerSystemTimings", "per-system performance telemetry"),
        ("FormIdPool", "stable FormIdPairs are serialized by the special form-id column; session-local handles are re-interned into a fresh pool"),
        // ── newly-covered scripting/audio/plugin roots (#3166) ─────
        ("QuestAliasReadinessGateRegistry", "static engine-supplied quest-alias gate definitions rebuilt from installed quest content"),
        ("SceneFragments", "static lowered SCEN VMAD fragment definitions rebuilt from plugin data on scene installation"),
        ("AudioWorld", "owns live kira manager/handles and is reconstructed as process audio infrastructure"),
        ("AudioListener", "derived marker attached to the active camera during scene setup"),
        ("AudioEmitter", "decoded asset/handle payload rebuilt from authored sound data when its source spawns"),
        ("OneShotSound", "one-frame dispatch marker removed immediately after audio playback begins"),
        ("SoundCache", "decoded audio asset cache repopulated on demand"),
        ("DataStore", "immutable resolved plugin-record database rebuilt from manifests/plugins at boot"),
        // ── crates/core/src/ecs/components/ ─────────────────────────
        ("ActiveCamera", "set once at scene/cell setup (scene.rs), no gameplay mutator reassigns it"),
        ("AnimatedAlpha", "per-frame output re-derived every tick from saved AnimationPlayer/AnimationStack"),
        ("AnimatedAmbientColor", "per-frame output re-derived every tick from saved AnimationPlayer/AnimationStack"),
        ("AnimatedDiffuseColor", "per-frame output re-derived every tick from saved AnimationPlayer/AnimationStack"),
        ("AnimatedEmissiveColor", "per-frame output re-derived every tick from saved AnimationPlayer/AnimationStack"),
        ("AnimatedMorphWeights", "per-frame output re-derived every tick from saved AnimationPlayer/AnimationStack"),
        ("AnimatedShaderColor", "per-frame output re-derived every tick from saved AnimationPlayer/AnimationStack"),
        ("AnimatedShaderFloat", "per-frame output re-derived every tick from saved AnimationPlayer/AnimationStack"),
        ("AnimatedSpecularColor", "per-frame output re-derived every tick from saved AnimationPlayer/AnimationStack"),
        ("AnimatedTextureFlip", "per-frame output re-derived every tick from saved AnimationPlayer/AnimationStack; handles are bindless texture indices re-resolved by attach_animation_sinks on load, same as the spawn path"),
        ("AnimatedUvTransform", "per-frame output re-derived every tick from saved AnimationPlayer/AnimationStack"),
        ("AnimatedVisibility", "per-frame output re-derived every tick from saved AnimationPlayer/AnimationStack"),
        ("AttachPoints", "write-once NIF-import data, no runtime mutator (no query_mut/get_mut site exists)"),
        ("Billboard", "write-once at NIF/spawn time, no runtime mutator"),
        ("SpeedTreeWind", "neutral (1.0, 0.0) canopy response rebuilt with the SpeedTree import and consumed by the shared weather-wind system; TREE.CNAM is parsed but deliberately not read into it until a citable field layout lands (#3190)"),
        ("BSBound", "NIF-import-derived AABB, debug-inspection only, never mutated"),
        ("BSXFlags", "write-once from NIF root extra data, no runtime mutator"),
        ("Camera", "only field ever runtime-mutated (aspect) is re-derived from the live window size on resize"),
        ("CellFormId", "set once per cell load straight from the ESM CELL record's FormID, no runtime mutator"),
        ("CellRoot", "stamped once per entity at cell load, idempotent across reloads of the same cell"),
        ("ChildAttachConnections", "write-once NIF-import data, no runtime mutator, same file/pattern as AttachPoints"),
        ("CollisionShape", "NIF/Havok-derived static geometry, only read by physics sync to register Rapier colliders"),
        ("CombustionState", "one-shot cosmetic VFX clock keyed on ABSOLUTE engine time (start_time_seconds); the volume it times out-lives its authored lifetime by under a few seconds, and a restored start stamp would read as already-expired or replay a fireball against an unrelated clock — same posture as ParticleEmitter"),
        ("EscortBehavior","active-package-derived config rebuilt at spawn and replaced by ambient_ai_package_system; mutable companion EscortState is registered"),
        ("CreatureAttack", "already covered by the NPC-spawn-stamped guard's own REDERIVED_NOT_SAVED list above (#3762) — write-once at NPC spawn from the CREA record's DATA.Damage, no runtime mutator"),
        ("FactionRanks", "already covered by the NPC-spawn-stamped guard's own REDERIVED_NOT_SAVED list above (#1835)"),
        ("FogVolume", "converted once at cell/scene load from static XCLL/WTHR/NiFogProperty data, only read by the froxel injector"),
        ("FollowBehavior", "active-package-derived config rebuilt at spawn and replaced by ambient_ai_package_system; mutable companion FollowState is registered"),
        ("Furniture", "write-once at NIF import (BSFurnitureMarker), only ever read, never query_mut'd"),
        ("GlobalTransform", "recomputed every frame from saved Transform + Parent by transform_propagation_system; its own doc says \"never written by user code\""),
        ("GroundCoverPalette", "EXAL-derived (#2369): re-resolved from the worldspace identity + its GRAS records at every worldspace entry by install_ground_cover; saving it would pin a stale palette across a plugin change"),
        ("GuardBehavior", "active-package-derived config rebuilt at spawn and replaced by ambient_ai_package_system; mutable companion GuardState is registered"),
        ("LocalBound", "write-once at NIF import/mesh spawn, read-only thereafter to derive WorldBound"),
        // Material: FIXED — registered (#2378 / SAVE-D1-13), no longer
        // allowlisted here.
        ("MaterialTextureDebugInfo", "cold-path material provenance used only by mat.dump; deterministically rebuilt from NIF/TXST translation at mesh spawn"),
        ("MeshHandle", "GPU MeshRegistry index, explicitly named in this file's own exclusion doc, rebuilt from the mesh path every reload"),
        ("ParticleEmitter", "per-particle simulation state (positions/velocities/ages) is purely cosmetic VFX with no gameplay/script hooks; re-seeds from static rate/shape config within under a second of reload"),
        ("PatrolBehavior", "active-package-derived config rebuilt at spawn and replaced by ambient_ai_package_system; mutable companion PatrolState is registered"),
        ("PerkList", "zero production write sites exist anywhere (only #[cfg(test)]); do not confuse with the unrelated, already-tracked Perks character component"),
        ("PhysicsSourceForm", "write-once at bhk-shape spawn; its own doc says \"read only by diagnostics\", never mutated"),
        ("PrecombinedMesh", "write-once at precombine spawn (EX-15 / #2369), idempotent across reloads of the same cell — same category as CellRoot/RenderLayer, split out purely so world.owners can track precombine geometry as its own reclaim class"),
        ("RenderDebugControl", "operator-only renderer diagnostics and one-frame probe handoff state; deliberately resets to the default view on process start/load"),
        ("RenderLayer", "every insert site is one-shot at cell-load/NPC-spawn via pure classifier functions, no runtime mutator"),
        // RigidBodyData: FIXED — registered (#2379 / SAVE-D1-14), no
        // longer allowlisted here.
        ("SandboxBehavior", "active-package-derived config rebuilt at spawn and replaced by ambient_ai_package_system; only read by sandbox_seat_system"),
        ("SceneFlags", "write-once at NIF import/cell spawn; its one mutator method (set_culled) is unused in production"),
        ("SkinnedMesh", "GPU skeleton-binding handle, same exclusion class as MeshHandle, rebuilt from skeleton resolution every import"),
        ("SubmersionState", "fully recomputed every frame from saved Transform + WaterPlane/WaterVolume by submersion_system"),
        ("TextureHandle", "GPU TextureRegistry index, explicitly named in this file's own exclusion doc, re-resolved by path every load"),
        ("TravelBehavior", "active-package-derived config rebuilt at spawn and replaced by ambient_ai_package_system; mutable companion TravelState is registered"),
        ("WanderBehavior", "active-package-derived config rebuilt at spawn and replaced by ambient_ai_package_system; mutable companion WanderState is registered"),
        ("WaterContact", "per-tick physics-derived output recomputed from body pose + WaterVolume; persistent breath/drowning carry lives in the saved CharacterController"),
        ("WaterCurrentVolume", "static REFR.XWCU + XPRM-derived current volume rebuilt during cell reference load, never runtime-mutated"),
        ("WaterFlow", "static per-cell flow vector set once from WATR wind_direction or authored NAM0 velocity at cell load, no runtime mutator"),
        ("WaterPlane", "static per-cell water geometry+material set once from XCWT/WATR at cell load, no runtime mutator"),
        ("WaterVolume", "static per-cell AABB set once from XCLW/cell floor data at cell load, no runtime mutator"),
        ("WaterLodInfo", "diagnostic-only worldspace LOD provenance rebuilt with the render-only LOD entity and removed on unload"),
        ("WaterNoiseMapHandles", "GPU bindless noise-texture handles rebuilt from WATR NAM2/NAM3/NAM4 paths at cell load; released on unload and never save-relevant"),
        ("WindField", "EXAL-derived (#2369): re-translated from the live WeatherDataRes wind byte at every worldspace entry; the saved WTHR state it derives from is what carries forward"),
        ("WorldBound", "per-frame bound recomputed from saved LocalBound + GlobalTransform, same exclusion class as GlobalTransform"),
        // ── crates/scripting/src/ ────────────────────────────────────
        ("ActivateEvent", "one-shot event marker drained every frame by event_cleanup_system"),
        // ActorCinematicState: FIXED — registered (#2380 / SAVE-D1-15),
        // no longer allowlisted here.
        ("ActorStats", "forward-latent — no live production insert site exists outside tests"),
        ("AnimationTextKeyEvents", "one-shot event marker drained every frame by event_cleanup_system"),
        ("CameraShakeCommand", "one-shot command marker drained every frame by event_cleanup_system"),
        // CinematicPresentationState: FIXED — registered (#2380 /
        // SAVE-D1-15), no longer allowlisted here.
        ("CompatibilityRegistry", "session diagnostic aggregate rebuilt from compatibility reports while PEX content is scanned; no gameplay authority or mutable script state"),
        ("ControllerRumbleCommand", "one-shot command marker drained every frame by event_cleanup_system"),
        ("DialogueLineCompletionBatch", "one-shot presentation-ingress batch, snapshotted+drained every tick"),
        ("DialoguePlayback", "documented #1696-style rationale on the type itself (#2294)"),
        ("DialoguePresentationEventBatch", "one-shot presentation batch, drained at the start of every tick before being repopulated the same tick"),
        ("DialogueRegistry", "populated once from parsed DIAL/INFO ESM records, only ever read afterward"),
        ("Dlc2Ttr4aPlayerScript", "forward-latent — no live production spawn site exists outside tests/examples"),
        ("EquipItemCatalog", "populated once at cell/plugin load, only ever read afterward"),
        ("EvaluatePackageRequest", "one-shot ingress marker, drained every tick by scene_package_system"),
        ("ExtensionScriptFunctionInvoker", "process-local authenticated host callback republished after extension activation; Arc function pointers are neither portable nor save state"),
        // FragmentExecutionQueue: FIXED — registered (#2381 /
        // SAVE-D1-16), no longer allowlisted here.
        ("HitEvent", "one-shot event marker drained every frame by event_cleanup_system"),
        ("SplashEvent", "one-shot water-surface event marker drained every frame by event_cleanup_system"),
        ("RippleEvent", "one-frame water-surface event marker drained every frame by event_cleanup_system"),
        // HorseTetherState: FIXED — registered (#2380 / SAVE-D1-15), no
        // longer allowlisted here.
        ("KeystoneInventory", "forward-latent — its only mutator only fires for MG07LabyrinthianDoor entities, which have no live production spawn site"),
        ("MG07LabyrinthianDoor", "forward-latent — no live production spawn site exists outside tests, despite its systems being scheduler-wired"),
        ("MotionTypeChangeRequest", "one-shot request, applied and drained same-tick by its own consumer system"),
        ("OnCellLoadEvent", "one-shot event marker drained every frame by event_cleanup_system"),
        ("OnInitEvent", "one-shot script-attachment marker drained every frame by event_cleanup_system"),
        ("EquipmentEventBatch", "one-shot equipment transition batch drained every frame by event_cleanup_system"),
        ("OnTriggerEnterEvent", "one-shot event marker drained every frame by event_cleanup_system"),
        ("OnUpdateEvent", "one-shot event marker drained every frame by event_cleanup_system"),
        ("PapyrusProviderProgram", "write-once source/PEX translation attached from static script content on load; only suspended mutable tails live in the saved PapyrusProviderContinuationQueue"),
        ("PapyrusModEventRuntime", "transient per-instance extender-compatible registrations and same-frame delivery queue; scripts refresh registrations from OnInit/OnLoad after world replacement, and raw EntityIds must not cross saves"),
        ("PapyrusProviderRuntime", "process-local live provider catalog plus authenticated host callback rebuilt after extension activation; callback pointers are not save state"),
        ("PendingFragmentActivations", "one-frame handoff queue (#2654) drained every frame by fragment_activation_flush_system; deliberately transient for the same reason ActivateEvent above is — it holds raw EntityIds, which a live in-session reload churns (the SAVE-D6-01 / #1696 / #2380 hazard class), and persisting a queue whose only output marker is itself unsaved would be incoherent. Worst case is one queued scripted activation lost if a save lands in the single frame between fragment dispatch and the next frame's flush"),
        ("PackageRegistry", "populated once from parsed PACK records, only ever read afterward"),
        ("PackageTargetRegistry", "populated once from placed-REFR positions, only ever read afterward"),
        ("QuestAdvanceOnActivate", "write-once static config from decompiled-script data; only read to decide whether to write the already-saved QuestStageState"),
        ("QuestTriggerApproachRegistry", "process-lifetime actor-trigger metadata rebuilt from parsed REFR VMAD and PEX data on every content load"),
        ("QuestAliasInjectedOverlays", "derived QUST alias injection metadata, rebuilt whenever SceneActorBindings is marked dirty"),
        ("QuestAliasRuntimeOverlays", "derived full QUST alias metadata, rebuilt whenever SceneActorBindings is marked dirty"),
        ("QuestDefinitionRegistry", "populated once from parsed QUST records, only ever read afterward"),
        ("QuestStageAdvancedBatch", "one-shot batch drained every frame by event_cleanup_system"),
        ("QuestStageFragments", "populated once at cell load from decoded VMAD/PEX, only ever read afterward"),
        ("RecurringUpdate", "forward-latent — its only writer is unreachable in production today (no live Dlc2Ttr4aPlayerScript spawn site); re-evaluate the moment a real RegisterForUpdate recognizer lands"),
        // RumbleOnActivate: FIXED — registered (#2382 / SAVE-D1-17), no
        // longer allowlisted here.
        ("SceneActionCompletionBatch", "one-shot batch drained every tick by scene_playback_system"),
        ("SceneActorBindings", "fully computed/cached resource, rebuilt from scratch off static registries whenever marked dirty"),
        ("SceneAliasCandidate", "write-once at REFR-spawn time from static reference/base-record identity, re-derived identically every reload"),
        ("RemoteSceneActorStub", "derived marker attached only to synthetic offscreen scene actors; remote aliases and their stubs are reconstructed from static QUST/SCEN identity on reload"),
        ("SceneEventBatch", "one-shot batch drained every tick"),
        ("SceneFragmentInvocationBatch", "one-shot batch drained every tick"),
        ("ScenePackageCompletionBatch", "one-shot batch drained every tick by scene_package_system"),
        ("ScenePackageEventBatch", "one-shot batch drained every tick by scene_package_system"),
        ("ScenePackagePlayback", "documented #1696-style rationale on the type itself (#2294)"),
        ("ScenePlayer", "documented #1696-style rationale on the type itself (#2294)"),
        ("SceneQuestAliasRegistry", "populated once from parsed QUST alias definitions, only ever read afterward"),
        ("SceneRegistry", "populated once per plugin/cell install from static SCEN records, only ever read afterward"),
        ("SceneStartRequest", "one-shot request drained every tick"),
        ("SceneStopRequest", "one-shot request drained every tick"),
        ("LegacyObscriptContentCatalog", "immutable load-order snapshot republished from GlobalFormIdResolver before every legacy-script tick"),
        ("LegacyObscriptProgram", "write-once source translation attached from static SCPT/SCTX data on every content load; mutable numeric locals live in the separately saved ScriptVariables component"),
        ("ScriptRegistry", "static editor_id-to-function-pointer map populated only by explicit .register() calls at boot; function pointers aren't meaningfully serializable"),
        ("StartGameQuestRegistry", "populated once from ESM QUST records; its own doc states repeated cell loads are idempotent by design"),
        ("TimerExpired", "one-shot event marker drained every frame by event_cleanup_system"),
        ("TriggerVolume", "occupancy is engineered (fix #1817) to self-correct via a None-sentinel cold-start re-seed with zero observable difference"),
        ("TwoStateTransitionBatch", "one-shot presentation batch drained every tick; the state it summarizes (TwoStateActivator) is already registered"),
        ("UiMessageCommand", "one-shot command marker drained every frame; its only writer is unreachable in production today (same reason as MG07LabyrinthianDoor)"),
        // ── crates/physics/src/ ──────────────────────────────────────
        ("ActorBoneCollider", "derived label, not state: re-applied to every skeleton bone by keyframe_live_ragdoll_bones on each NPC spawn, which a load re-runs (#2873)"),
        ("ActorColliderOwner", "derived skeleton-bone to placement-root link rebuilt by keyframe_live_ragdoll_bones on every NPC spawn"),
        ("ContactConfig", "boot-time tunable resource, no runtime mutator (no resource_mut call exists outside tests)"),
        ("PhysicsWaterConstants", "boot-time tunable resource, no runtime mutator (no resource_mut call exists outside tests)"),
        ("WaterContactScratch", "transient per-tick buoyancy staging buffers whose capacities are reused; surfaces, currents, targets, and writes are derived from live ECS/physics state, and the #3268 in-current latch is re-derived by the first scan after a load (worst case one redundant wake)"),
        ("PhysicsWorld", "owns live Rapier handle sets, architecturally rebuilt from cell data (CollisionShape/RigidBodyData/Transform) every load, not snapshot-restored"),
        ("Ragdoll", "handle bookkeeping only, no live inserter exists yet (debug console command only) — same posture as Dead (#2293)"),
        ("RapierHandles", "self-healing generational index; its own doc states absence is the signal to re-derive it, and physics_sync_system does so automatically"),
        // ── byroredux/src/ ────────────────────────────────────────────
        // #2536 / SAVE-D1-18 — added alongside the new `SCAN_ROOTS`
        // entry below; classifies every `impl Component`/`impl
        // Resource` under the binary crate not already registered in
        // `build_save_registry` (`ActorCinematicState`,
        // `HorseTetherState`, `ActorControlState`, `RigidBodyData`,
        // `Material`, `RumbleOnActivate`, `CurrentCellContext`,
        // `PlayerPose`, `GameTimeRes` all already are). Verified
        // 2026-08-08 against real (non-`#[cfg(test)]`) insertion sites
        // for every entry, same bar as the rest of this list.
        ("ActionBindings", "boot-time input configuration; rebuilt from defaults/settings rather than persisted as gameplay state"),
        ("ActionState", "per-frame held/pressed/released action edges derived entirely from InputState"),
        ("AlphaBlend", "spawn-time classification extracted from NiAlphaProperty flags at import, rederived identically every load"),
        ("AmbientPackageRuntime", "NPC_.PKID candidate state rebuilt at spawn and re-evaluated on the first tick against restored clock/CTDA resources"),
        ("CellLightingRes", "WTHR ambient/directional CPU-side mirror, re-flowed from the plugin's parsed lighting record every cell load"),
        ("CellRootIndex", "inverted CellRoot->owned-entities index, repopulated by cell_loader::stamp_cell_root every cell load (#791)"),
        ("CellRootRefIndex", "lazily-rebuilt FormId->Entity cache scoped to a caller-named ordinary CellRoot (EX-14/15/#2369, EX-16/#2372), repopulated on demand by cell_loader::cell_root_ref_index — sibling of PersistentRefIndex, same posture"),
        ("CloudSimState", "cloud-scroll accumulator, seeded at [0,0] only when absent — both apply_worldspace_weather branches use an is_none() guard so the accumulator survives interior visits and only a fresh session (or save/load round-trip, which does not snapshot it) resets it to [0,0] (see its own #803 doc)"),
        ("WeatherSurfaceState", "history-dependent exterior rain-film and snow-coverage state; session/worldspace simulation state is rebuilt dry rather than serialized until per-cell exposure persistence exists"),
        ("CombatState", "session-local attack timing and smoke telemetry; canonical Health/Dead/EquippedWeapon state is saved separately"),
        ("CurrentCellRoot", "tracks the interior placement-root entity, set fresh by load_cell_with_masters and cleared by execute_pending before each cell load"),
        ("DebugLoadArchiveSet", "debug cell.load console-command bookkeeping (#2078), outside the normal single-launch CLI path"),
        ("DoorTeleport", "XTEL destination data, rederived identically from the plugin's parsed REFR every cell load"),
        ("FootstepConfig", "engine-wide footstep sound configuration loaded once at startup from a vanilla BSA"),
        ("WaterAudioConfig", "engine-wide water sound configuration loaded from the archive at startup; audio assets are re-resolved, not gameplay save state"),
        ("WaterAudioState", "per-frame ripple-audio cooldown derived from transient SplashEvent/RippleEvent markers"),
        ("WaterDisturbanceScratch", "per-frame scratch buffer for submersion_system's collect-then-publish pattern (#3257) — capacity-only persistence, no gameplay state"),
        ("FootstepEmitter", "per-frame position/stride accumulator mutated by footstep_system every tick — ephemeral audio-cue bookkeeping, not gameplay state"),
        ("FootstepScratch", "per-frame scratch buffer for footstep_system's two-phase collect/drain pattern (#932) — capacity-only persistence, no gameplay state"),
        ("HavokAnimationTarget", "skeleton_root + consumed_idle_serial are both spawn-time-resolved (serial always starts at 0), rederived identically every load"),
        ("HavokIdleCatalog", "process-lifetime IDLE FormID -> animation handle mapping, populated once and read-only afterward — same posture as AnimationClipRegistry"),
        ("InjectedKeyPulse", "one-frame debug/smoke input ingress drained by refresh_action_state; never canonical gameplay state"),
        ("InjectedKeyHold", "bounded debug/smoke input ingress drained by refresh_action_state; never canonical gameplay state"),
        ("InputState", "live keyboard/mouse state for the fly camera, inherently process-session-local"),
        ("InventoryCatalog", "read-only item presentation metadata rebuilt from the resolved plugin index on every content load"),
        ("InteractionCandidateScratch", "per-frame scratch buffer for collect_candidates's take/restore reuse pattern (#3059) — capacity-only persistence, no gameplay state"),
        ("InteractionState", "camera-forward target derived from live transforms and interactable components every frame"),
        ("InteractionTrace", "session-local interaction diagnostics retained across a cell transition for smoke observability, not gameplay state"),
        ("IsDecalMesh", "spawn-time classification per FO4 BGSM decal semantics, rederived identically every load"),
        ("IsFxMesh", "spawn-time classification lifted from a per-frame material-path scan (PERF-D3-NEW-02/#1136), rederived identically every load"),
        ("IsLodTerrain", "spawn-time classification set only by terrain_lod::spawn_lod_block, rederived identically every load"),
        ("LightTuning", "live-tuning resource mutated only by the light.atten debug console command, for A/B comparison — not gameplay state"),
        ("Locked", "XLOC lock data, rederived identically from the plugin's parsed REFR every cell load (#3098) — becomes save-relevant once a lockpicking/unlock system exists"),
        ("LoadedCellIndex", "read-only parsed-ESM cell index, Arc-shared scene metadata rebuilt every cell load"),
        ("LoadedPluginSet", "boot-time CLI --esm/--master capture, reused only to re-invoke load_cell_with_masters — not gameplay state"),
        ("MaterialTextureHandles", "bindless GPU texture handle set, rebuilt by the texture-upload path every load — handles aren't stable across process restarts"),
        ("MetricsState", "process-diagnostics sampler holding a live sysinfo::System handle, not gameplay state"),
        ("NameIndex", "lazily-rebuilt Name->EntityId cache, invalidated by its own generation counter (#249)"),
        ("NavPath", "cached single-tile NAVM waypoint path (EX-16 item 3 Phase 3 / #2372), rederived on demand from resident NavmeshTile data by navmesh_path::path_from_resident_tiles — same posture as NavmeshTile itself, never lossy gameplay state"),
        ("NavmeshTile", "NAVM geometry residency plumbing (EX-16 item 2 / #2372), rederived identically from the plugin's parsed NavmRecord every cell/tile load — same posture as DoorTeleport/Locked"),
        ("NifImportRegistry", "process-lifetime parsed-NIF LRU cache, keyed by model path — re-populated on demand, never save-relevant"),
        ("NoSorter", "marker for NiAlphaProperty.flags bit 13 (\"No Sorter\", #3797), rederived identically from NIF material data every load — same posture as TwoSided"),
        ("NormalMapHandle", "bindless GPU texture handle for the water normal-map path, rebuilt by the texture-upload path every load"),
        ("PendingCellTransitionSlot", "one-shot queued-transition slot, always present but empty except mid-transition"),
        ("PendingDeathReconciliations", "same-frame death handoff queue drained by the late exclusive reconciliation sink; canonical Dead state is saved separately"),
        ("PendingPlayerSaveActions", "one-shot player save/load requests drained after the scheduler's parallel batch joins (#3113)"),
        ("PendingSaveLoadSlot", "one-shot queued-load slot (#1848/SAVE-05), empty except mid-drain — save/load plumbing itself, not save-worthy state"),
        ("SaveLoadNotifications", "one-shot player-feedback queue drained into the HUD/console before the next frame"),
        ("PersistentRefIndex", "lazily-rebuilt FormId->Entity cache scoped to the resident persistent CELL (EX-09/#2370), repopulated on demand by cell_loader::persistent_ref_index — same posture as CellRootIndex/NameIndex"),
        ("PlayerEntity", "points to the process-lifetime player body, which deliberately outlives cell unload; the entity remains valid across live reload and the resource is process-local identity, not gameplay state"),
        ("PlayerInventoryTemplate", "read-only starting loadout rebuilt from the master Player NPC record; live Inventory/EquipmentSlots are saved separately"),
        ("PlayerMode", "engine-wide FlyCam/Character flag set at scene-setup from CLI flags + scene type, not gameplay state"),
        ("RagdollActive", "marker for live ragdoll simulation, same physics-rebuild posture as PhysicsWorld above — not snapshot-restored"),
        ("RagdollTemplate", "per-actor ragdoll blueprint resolved at spawn against the loaded skeleton, rederived identically every load — same posture as PhysicsWorld"),
        ("RegionAmbientRes", "resolved REGN Sound-entry FormIDs for the resident cell/tile (EX-16 item 1 / #2372), rederived identically from the cell's XCLR list + the plugin's parsed REGN map every cell load — same posture as CellLightingRes/NavmeshTile"),
        ("SandboxSitClip", "resolved once at cell load from the archive provider, read-only afterward"),
        ("SaveState", "save-slot directory + ring cursor, resumed from disk at startup (SaveState::new) — save/load plumbing itself, not part of the world snapshot"),
        ("SceneImportCache", "process-lifetime parsed-scene cache wrapper around the same ParsedNifCache core as NifImportRegistry"),
        ("SeatReservations", "derived sandbox occupancy, pruned on cell-reference load against live Furniture + claimant Seated state — see its own doc"),
        ("SettingsPersistence", "process-local user-config path; preferences are independently persisted in settings.toml, never inside a gameplay save"),
        ("SkyParamsRes", "WTHR sky rendering parameters, rebuilt from the parsed record every exterior cell load"),
        ("SoundArchiveProvider", "engine-wide --sounds-bsa archive handle(s) opened once at startup (EX-16 item 5 / #2372), same posture as FootstepConfig/WaterAudioConfig/ScriptProvider — audio assets are re-resolved, not gameplay save state"),
        ("Spinning", "demo-scene marker component, not present on any real gameplay content"),
        ("StudioSession", "editor-mode state for the `--studio` asset-preview/inspection host (SDK v0.1); ObjectId bindings and undo transforms describe a tooling session over loose NIF/asset content, never gameplay in a player save"),
        ("SubtreeCache", "lazily-rebuilt animation subtree cache, invalidated alongside NameIndex (#278)"),
        ("TerrainTileSlot", "index into the renderer's per-frame GpuTerrainTile SSBO, rebuilt by the terrain-spawn path every load (#470)"),
        ("TwoSided", "marker for backface-culling state, rederived identically from NIF material data every load"),
        ("VisibleWhenDistant", "spawn-time classification derived from the streaming-ring/LOD-radius relationship (#1889), rederived identically every load"),
        ("WaterDrawIndexScratch", "per-frame render scratch whose map is cleared and rebuilt from the current sorted draw list; only allocation capacity persists"),
        ("WeatherDataRes", "WTHR NAM0 sky-color table, rebuilt from the parsed record whenever weather is (re)applied"),
        ("WeatherTransitionRes", "one-shot weather-blend accumulator, present only mid-transition and removed on completion"),
        // #3497 — surfaced by discover_scan_roots scanning crates/renderer,
        // crates/debug-ui and crates/save for the first time. None are
        // gameplay state; all predate this guard, which simply never
        // looked at their crates before.
        ("AllocatorResource", "newtype around the renderer's SharedAllocator (GPU device handle), inserted once at renderer init — process/device infrastructure, not gameplay state"),
        ("GpuMemoryBudget", "constant-after-device-selection VRAM capacity snapshot sampled from vkGetPhysicalDeviceMemoryProperties at renderer init; re-sampled identically on every launch"),
        ("DebugUiState", "the egui debug/console overlay's own UI state (console history, panel visibility, input buffers) — operator tooling, never gameplay in a player save"),
        ("SaveRegistry", "the type-erased save/load driver table itself, built once at startup from the curated component/resource type set — save-system infrastructure, not the gameplay state it describes"),
    ];

    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let scan_roots = discover_scan_roots(manifest);
    let mut files = Vec::new();
    for root in &scan_roots {
        collect_rs_files(root, &mut files);
    }

    let mut found: Vec<(String, String)> = Vec::new();
    for path in &files {
        let src = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("SAVE-D1-12 guard can't read {}: {e}", path.display()));
        // Test modules commonly declare fixture Component/Resource types in
        // production files. They are not live ECS state and must not pollute
        // the persistence ledger. Repository convention keeps cfg(test)
        // modules at file tails; standalone *_tests.rs files are all-test.
        if path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .is_some_and(|stem| stem.ends_with("_tests"))
            || path.components().any(|part| part.as_os_str() == "tests")
        {
            continue;
        }
        let production_src = src.split("#[cfg(test)]").next().unwrap_or(&src);
        for (i, line) in production_src.lines().enumerate() {
            if let Some(name) = impl_target_type(line) {
                found.push((name.to_string(), format!("{}:{}", path.display(), i + 1)));
            }
        }
    }
    assert!(
        !found.is_empty(),
        "SAVE-D1-12 guard found zero impl Component/Resource lines under \
         {scan_roots:?} — the scan itself is broken (wrong roots, or the \
         `impl Component for X` / `impl Resource for X` line shape changed).",
    );

    let registry = build_save_registry();
    let registered: std::collections::HashSet<&str> = registry
        .component_names()
        .chain(registry.resource_names())
        .collect();
    let allowlisted: std::collections::HashSet<&str> =
        NOT_SAVED_BY_DESIGN.iter().map(|(name, _)| *name).collect();

    let mut offenders = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (name, loc) in &found {
        if !seen.insert(name.clone()) {
            continue;
        }
        let saved = registered.contains(name.as_str());
        let allowed = allowlisted.contains(name.as_str());
        if saved && allowed {
            offenders.push(format!(
                "{name} ({loc}): registered in build_save_registry AND in \
                 NOT_SAVED_BY_DESIGN — pick one"
            ));
        } else if !saved && !allowed {
            offenders.push(format!(
                "{name} ({loc}): neither registered in build_save_registry \
                 nor in NOT_SAVED_BY_DESIGN — classify it (see the guard's \
                 doc comment for the categories) before landing this type"
            ));
        }
    }
    assert!(
        offenders.is_empty(),
        "SAVE-D1-12 (#2295/#3166): every production Component/Resource impl under \
         the configured core, scripting, physics, audio, plugin, and binary roots \
         must be registered XOR allowlisted. \
         Offenders: {offenders:#?}",
    );
}
