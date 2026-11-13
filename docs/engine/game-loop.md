# Game Loop

The game loop lives in the binary crate at [`byroredux/src/main.rs`](../../byroredux/src/main.rs).
It uses winit's `ApplicationHandler` trait for event-driven execution. CLI
arguments choose between several scene-loading entry points; the default
is the spinning-cube demo. Once a scene is loaded, the per-frame tick runs
the same ECS schedule regardless of how the scene was created.

> **Currency note.** Reconciled 2026-08-25 against the current tree
> (the `App`/scheduler wiring shifted substantially after the
> 2026-04-07 version of this doc: `systems.rs` was split into a
> `systems/` submodule tree in Session 34, the M27 declared-access
> scheduler landed, the render path moved off `RedrawRequested` into
> `about_to_wait` (Phase 14), and an egui debug-UI overlay + metrics
> sampling were added). Function/struct/path names below are checked
> against `main.rs`, `byroredux/src/systems/`, and
> [`crates/core/src/ecs/scheduler.rs`](../../crates/core/src/ecs/scheduler.rs).

## CLI entry points

CLI args are parsed inline in [`byroredux/src/main.rs`](../../byroredux/src/main.rs)
(scanning `effective_args()`, not a derive-macro `Args` type) and in
[`byroredux/src/scene.rs`](../../byroredux/src/scene.rs)'s `setup_scene`.
The thin parser helpers (`parse_string_arg`, `parse_vec3_arg`,
`effective_args`, `set_effective_args`) live in
[`byroredux/src/cli_args.rs`](../../byroredux/src/cli_args.rs).

| Args | Scene loaded |
|---|---|
| (none) | Spinning cube demo |
| `path/to/mesh.nif` | Loose NIF file |
| `path/to/mesh.nif --kf path/to/anim.kf` | NIF + KF animation playback |
| `--bsa path.bsa --mesh meshes\foo.nif` | Single NIF extracted from a BSA |
| `--bsa meshes.bsa --mesh meshes\foo.nif --textures-bsa textures.bsa` | + textures |
| `--bsa meshes.bsa --tree trees\joshua01.spt --textures-bsa textures.bsa` | SpeedTree `.spt` placeholder billboard |
| `--esm FalloutNV.esm --cell CellID --bsa "Fallout - Meshes.bsa" --textures-bsa "Fallout - Textures.bsa"` | Interior cell from an ESM (vanilla FNV archive names — `--bsa` opens the literal path, there is no bare `Meshes.bsa`; `Fallout - Textures2.bsa` auto-loads as a `<stem>N.bsa` sibling, no third `--textures-bsa` needed) |
| `--master Skyrim.esm --esm Dawnguard.esm --cell ... --bsa ...` | DLC interior (repeatable `--master`) |
| `--esm FalloutNV.esm --grid 0,0 --radius 3 --bsa ...` | Exterior grid (radius 1..=7, default 3) |
| `--swf path/to/menu.swf` | Scaleform SWF menu overlay |
| `--game <key>` | Profile-driven archive/ESM expansion (Phase 20) — **recommended over hand-typing `--esm`/`--bsa`/`--textures-bsa`**: expands to absolute paths from `assets/debug_profiles.toml`, so it is CWD-independent and cannot mistype an archive name, e.g. `--game fnv --cell GSProspectorSaloonInterior` |

Selected modifier / diagnostic flags (all read from `effective_args()`):

| Flag | Effect |
|---|---|
| `--debug` | Force `RUST_LOG=debug` + per-frame stats in the window title |
| `--cmd "console command"` | Headless: build an empty `World`, run one console command, exit (rejects cell/asset flags — #637) |
| `--bench-frames N` | Run N frames, print one `bench:` summary line, exit |
| `--bench-mode MODE` | Complete clock/camera contract: `renderer-static`, `renderer-stepped`, or `system-live` |
| `--bench-camera PATH` | Non-static frame-indexed path required by `renderer-stepped` (`pan`, `orbit`, `dolly`, `grid-cross`, `cut`) |
| `--bench-hold` | Keep running after the bench summary so `byro-dbg` can attach |
| `--screenshot PATH` | Capture a PNG on the bench-exit frame (requires `--bench-frames`) |
| `--bench-capture-every N` / `--bench-capture-dir DIR` | Write `DIR/frame_NNNNN.png` for every Nth rendered frame (N ≥ 3; needs `--bench-frames`; the last 3 frames are left to `--screenshot`). Byte-identical across runs under `renderer-stepped`. A capture mode: screenshots stall their frame, so take `bench:` timings from a separate run |
| `--render-debug-mode MODE` | Seed a named renderer correctness view before the first frame |
| `--camera-pos x,y,z` / `--camera-forward x,y,z` | Override the auto-framed initial camera pose |
| `--fly` / `--player` | Start in fly-camera vs kinematic-character mode (M28.5) |
| `--sounds-bsa PATH` | Sound archive for footstep/splash defaults + REGN ambient dispatch (M44). Repeatable — later archives override earlier ones: last-listed wins, so list overrides AFTER the vanilla one (#3776 wording; #3917 flipped the lookup; `--scripts-bsa` is the first-wins exception) |
| `--materials-ba2 PATH` | BGSM/BGEM material archive (FO4+; usually synthesized by `--game`) |
| `--sf-smoke <CELL_EDID>` | Headless Starfield ESM resolve-rate smoke test (requires `--esm`) |
| `--rotation-mode 0..=3` | Diagnostic switch for the REFR Euler→Y-up conversion |
| `--games-root PATH` | Base directory for `--game` profile expansion |

`--game <key>` is expanded **before** anything else reads argv:
`expand_game_profile_args` rewrites it into the underlying
`--esm` / `--bsa` / `--textures-bsa` / `--materials-ba2` set. The
expanded list is then frozen into a process-wide
`effective_args()` singleton (`cli_args::set_effective_args`) so every
downstream reader (scene setup, the NIF loader, debug-server loads,
cell-transition rebuilds) sees the post-expansion flags instead of
re-reading raw `std::env::args()`.

Each entry point lives in a small module
([`scene.rs`](../../byroredux/src/scene.rs) / its
[`scene/`](../../byroredux/src/scene) submodules,
[`cell_loader.rs`](../../byroredux/src/cell_loader.rs) / its
[`cell_loader/`](../../byroredux/src/cell_loader) submodules) that builds
the ECS world. Note that GPU-touching scene setup (`setup_scene`) needs the
renderer, so it runs in `resumed` — not at startup — once `VulkanContext`
exists.

## Startup Sequence

```
1. expand_game_profile_args(argv)  → effective_args() singleton seeded
2. env_logger from RUST_LOG (--debug forces "debug"); init_tracing()
3. Verify C++ bridge (byroredux_cxx_bridge::ffi::native_hello())
4. Detect early-exit flags (after logging/tracing/bridge-verify):
     --sf-smoke   → sf_smoke::run(), return
     --cmd        → headless command run (rejects cell/asset flags), return
5. App::new(debug_mode, &args):
     - Create World, insert the built-in resources (see below)
     - Register scripting + physics component storages
     - Build the ScriptRegistry (papyrus_demo spawners — M47.0)
     - Build the Scheduler (M27 declared-access schedule — see below)
     - Snapshot SystemList + SchedulerAccessReport into the World
     - Build the console CommandRegistry resource
     - Start the debug server (feature "debug-server", port 9876 / BYRO_DEBUG_PORT)
6. Stash bench/screenshot/camera-override args onto the App
7. event_loop.run_app(&mut app)  → enters winit's ApplicationHandler
```

`App::new` inserts a large built-in resource set into the `World`,
including (non-exhaustive): `DeltaTime(0.0)`, `TotalTime(0.0)`,
`EngineConfig`, `DebugStats`, `ScratchTelemetry`, `SkinCoverageStats`,
`CpuFrameTimings`, `SchedulerSystemTimings`, `MetricsState` +
`MetricsSnapshot` (debug-UI sampling), `PendingDebugLoadSlot`,
`SelectedRef`, `InputState`, `StringPool`, `FormIdPool`,
`AnimationClipRegistry`, `NameIndex`, `SubtreeCache`, `CellRootIndex`,
`PhysicsWorld`, `ContactConfig`, `AudioWorld`, `FootstepConfig` +
`FootstepScratch`, the parsed-NIF / scene-import caches, the
`game_profiles` registry, and the `ScriptRegistry`. The `Scheduler`,
`World`, and console `CommandRegistry` are all constructed here, before
the event loop and before the renderer exists.

## The system schedule (M27 declared-access)

Systems are grouped into **stages** that run sequentially in a fixed
order, defined by the `Stage` enum in
[`crates/core/src/ecs/scheduler.rs`](../../crates/core/src/ecs/scheduler.rs):

```
Early = 0  →  Update = 1  →  PostUpdate = 2  →  Physics = 3  →  Late = 4
```

Within a stage, non-exclusive systems run in **parallel** (rayon), and
**exclusive** systems (`add_exclusive`) run alone *after* the stage's
parallel batch. Parallel systems can declare their data access via
`add_to_with_access(stage, system, Access::new()...)`; the scheduler's
analyzer pairs declared systems and reports any read/write or write/write
conflicts (surfaced to the operator via the `sys.accesses` console
command, snapshotted into the `SchedulerAccessReport` resource at build
time). M27 Phase 3 reached **0 unknown / 0 conflicts** by either
declaring access on parallel systems or demoting genuinely
serial-but-disjoint systems (e.g. `spin_system`, `animate_lights_system`,
`audio_system`) to exclusive.

The live schedule built in `App::new` is:

| Stage | System | Mode | Notes |
|---|---|---|---|
| `Early` | `player_controller_system` | declared | Dispatches to `fly_camera_system` **or** `character_controller_system` by `PlayerMode`; access is the union of both inner systems (M28.5 + M27 Phase 3) |
| `Early` | `weather_system` | exclusive + declared | Weather/TOD/sky/cloud sim; serialized after the player-controller batch so `WindField` has one stable writer/reader order |
| `Early` | `timer_tick_system` | declared | Advances `ScriptTimer`, fires `TimerExpired` — dormant: no producer starts a timer today (#5608); the M47.2 update registrations tick in `script_update_system` |
| `Early` | `vats_mode_system` | exclusive + declared | VATS enter/exit on the `V` key edge (#6350); exclusive because the `VatsState` mode is the freeze switch the whole frame reads and `player_controller_system`'s VATS suppression gates read it too. Runs after the parallel Early batch so this frame's `ActionState` edges exist |
| `Update` | vitals upkeep (`restoration_system`, `bleedout_system`, `encumbrance_system`) | exclusive + declared | First Update exclusives (`update/interaction_and_crime.rs`): `TimedRestorations` onto `ActorValues`, essential actors back up when their bleedout runs out (M47.2), the player's carried weight against Carry Weight (the movement effect reads it next frame) |
| `Update` | activation producers (`interaction_system`, `fragment_activation_flush_system`, `door_activation_system`, `container_loot_system`) | exclusive | Emits canonical activation/loot state before scripting consumers; the flush delivers fragment-queued `Activate` targets the following frame (#2654), ahead of the loot consumer (#4712); a scripted or relayed activation of a load door opens it (#5868) |
| `Update` | jail, seats and evidence (`jail_bed_system`, `player_seat_system`, `evidence_deposit_system`) | exclusive + declared | Resting in a jail cell's bed ends the sentence (after the activation flush, like every consumer); a seat the player activated seats them (`Seated`, read by `IsCurrentFurnitureRef`) until they move; confiscated goods reach the jail's `StolenGoods` chest, loaded or parked |
| `Update` | combat chain (`combat_input_system` → `make_faction_hostility_system` → `make_guard_rush_system` → `make_npc_combat_ai_system`) | exclusive + declared | `update/combat.rs`: player attack input, ambient faction hostility (#4414), guards who see a wanted player stamp the force-greet directive (before the force-greet system), then NPC combat-AI chase/strike. These are `HitEvent` producers; `combat_damage_system` resolves them at the head of the player/animation segment below |
| `Update` | Papyrus-demo and trigger dispatchers (`rumble_on_activate_dispatch`, `rumble_tick_system`, `trigger_detection_dispatch`, `make_scene_trigger_actor_approach_system`, `quest_advance_dispatch`) | exclusive | `update/quest_scene.rs`: the M47.0 R5 demo dispatchers (event-driven, idle without scripted activity) and trigger detection, ahead of `quest_advance_dispatch` so an `OnTriggerEnter` raised this frame (offscreen scene actors included) is consumed this frame |
| `Update` | quest start and the Story Manager (`quest_startup_system`, `story_change_location_dispatch`, the story producers below, `story_manager_dispatch`) | exclusive | Start Game Enabled quests bootstrap, `CLOC` is raised on a location change, and the dispatch walks every raised event the same frame, before the alias refresh, so an SM-started quest looks like a Start Game Enabled one downstream (#5366) |
| `Update` | M43 story producers (`dropped_item_story_event_system`, `dead_body_discovery_system`, `iron_sights_system`, `mine_system`, `attraction_system`, `random_conversation_system`, `make_conversation_hold_system`) | exclusive | Raise `REMP`, `DEAD`, `IRON`, `TMEE`, `AOBJ` and `ADIA` between the `CLOC` producer and `story_manager_dispatch`, so each is walked the frame it is raised; each idles without its SM root. `BYRO_NO_RANDOM_CONVERSATIONS=1` leaves the two conversation systems out, and the hold honours `BYRO_NO_AI_LOCOMOTION`. `mine_system` is also a `HitEvent` producer |
| `Update` | aliases and scenes (`quest_alias_refresh_system`, `quest_alias_readiness_stage_system`, `scene_playback_system`, `scene_fragment_dispatch_system`) | exclusive | Alias refresh after startup and cell-candidate changes; SCEN playback observes the quest-start batch before its begin/end/phase fragments run |
| `Update` | packages and their fragments (`ambient_ai_package_system`, `package_fragment_system`, `perk_body_system`, `perk_fragment_system`, `terminal_fragment_system`) | exclusive | M42 ambient package evaluation (time checks bounded to one pass per in-game minute), then the `PF_` / `PRKF_` / `TERM_` fragments of the transitions the host reported (M47.2); `perk_body_system` keeps each perk holder's Ability spells and the player's Quest entries in line with its `Perks` (PERK-BODY) |
| `Update` | force-greet (`scripted_conversation_system`, `make_forcegreet_system`) | exclusive | `StartConversation` requests become directives; Dialogue-procedure packages walk their NPCs to the player and open the conversation (#5367; the approach honours `BYRO_NO_AI_LOCOMOTION`) |
| `Update` | ObScript quests (`obscript_quest_tick_system`, `obscript_quest_menu_mode_system`, `obscript_stage_result_system`) | exclusive | M47.3: running legacy quest scripts' `GameMode` blocks on the vanilla 5 s cadence, their `Begin MenuMode [code]` blocks on the frames a matching menu is open (GUI-EVT-006; the paused frames of a native page run this one and the stage results by hand), then the result scripts of the stages set since the last run |
| `Update` | scene packages and dialogue (`scene_package_system`, `two_state_activator_system`, `scene_dialogue_system`) | exclusive | PACK actions resolve and move actors and queue Done completions; the two-state activator consumes the flushed `OnActivate` (#3936); dialogue consumes the ActionStarted batch and exposes INFO subtitle state for the rest of the frame |
| `Update` | ObScript objects and speech (`mark_cell_reset`, `obscript_object_system`, `scripted_line_voice_system`, `obscript_speech_system`) | exclusive | M47.3: `OnReset` for a cell that reset on entry, the placed references' `SCRI` `GameMode` / `OnLoad` / `OnDeath` blocks (before `reference_script_event_system` drains `OnDeathEvent`), then `Say` / `SayTo` voice and their `SayToDone` blocks |
| `Update` | reference and alias scripts (`check_alias_scripts_dispatch`, `alias_script_event_system`, `reference_script_event_system`, `activate_parent_relay_system`, `animation_event_system`, `scripted_sound_system`) | exclusive | `update/script_runtime.rs`, M47.2: alias scripts start once their aliases fill and stop with their quest; reference and alias scripts receive this frame's events; activate parents relay to their children; object-graph events reach `OnAnimationEvent`; `Sound.Play` requests play |
| `Update` | quest fragments and latent clocks (`quest_fragment_dispatch`, `quest_terminal_stage_system`, `fragment_continuation_system`, `script_update_dispatch`, `distance_event_system`) | exclusive | Quest fragments right after the advance that emits `QuestStageAdvanced` (before cleanup drains it), then QSDT complete/fail flags and successor quests; `Utility.Wait` tails resume; `OnUpdate` / `OnUpdateGameTime`; distance events |
| `Update` | idles and scripted motion (`idle_clip_playback_system`, `idle_resume_system`, `pick_idle_system`, `scripted_motion_type_system`, `scripted_havok_impulse_system`, `archetype_idle_system`) | exclusive | `PlayIdle` requests become decoded HKX players (the animation batch sees them no later than next frame), a finished one-shot idle hands back the earlier animation, `PickIdle`; `SetMotionType` then `ApplyHavokImpulse` before Physics; Fallout 4 archetype idles |
| `Update` | the last hit producers, then the script hit consumers (`trap_contact_system`, `projectile_launch_system`, `projectile_flight_system`, `pending_trap_hit_flush_system`, `reference_script_hit_system`, `hit_event_system`, `obscript_hit_system`, `obscript_combat_system`) | exclusive | Trap contact, `Weapon.Fire` projectiles and the `ProcessTrapHit` flush (#5872) land their `HitEvent`s, then reference/alias `OnHit` (#5991), `RegisterForHitEvent`, and the ObScript `OnHit` / `OnHitWith` blocks and combat transitions read them. Pinned by `hit_event_consumers_run_after_every_producer` |
| `Update` | magic and revival (`magic_effect_system`, `magic_hit_system`, `magic_shader_clock_system`, `magic_shader_tint_system`, `obscript_destruction_system`, `revive_system`) | exclusive | M47.3: magic effects (`ScriptEffectStart` / `Update` / `Finish`, `OnMagicEffectHit`) after the ObScript `Cast` that starts them; shader clocks, then surface tint; `OnDestructionStageChange`; `Resurrect` |
| `Update` | script requests (`push_actor_system`, `get_up_system`, `flee_system`, `use_weapon_package_system` → `use_weapon_system`, `reset_inventory_system`, `sleep_system`, `music_system`, `reload_system`, `chargen_catch_up_system`, `script_ui_request_system`, `actor_alpha_system`, `door_fade_system`, `challenge_system`) | exclusive | The app's half of the commands that raise a request: `PushActorAway` knockdown and get-up, `ForceFlee`, `UseWeapon` (the M42 UseWeapon PACK procedure raises its request first, so the volley starts the same frame), `ResetInventory`, `SetPCSleepHours`, `PlayMusic`, `EssentialDeathReload`, chargen catch-up, script UI requests and actor alpha, and the load-door fade of a traveling actor (XCT-ROUTE-008); then New Vegas challenges complete |
| `Update` | `combat_damage_system`, `vats_sequence_end_system` | exclusive + declared / exclusive | `update/player_animation.rs`: resolves every `HitEvent` into health and death, after every producer above (the consumer end of the hit contract); then a confirmed VATS sequence is settled — a kill restores the Player Kill AP Reward (PERK-HOOK-004) |
| `Update` | graphs, vehicles and the recurring tick (`object_graph_step_system`, `cinematic_horse_route_system`, `vehicle_attachment_system`, `dlc2_ttr4a_on_init_dispatch`, `recurring_update_tick_system`) | exclusive | Animated props' behavior graphs step; cart tethers follow the horse's `XLKR` chain, then horse → cart → rider poses resolve before PostUpdate propagation. The demo dispatchers and the `RecurringUpdate` tick do nothing in production; they stay wired so the demos' fixture tests drive the real schedule (#5842) |
| `Update` | `player_identity_sync_system` + `player_derived_stats_system` + `pool_regen_tick_system` | exclusive + declared | Puts the race chosen in character creation (`CharacterIdentity`) back on the player's `Background`, refreshes the player's stamped PlayerOnly pools (#5039), then fixed-step character-pool regeneration when the per-game ruleset enables it |
| `Update` | demo dispatchers (`dlc2_ttr4a_on_update_dispatch`, `mg07_on_load_dispatch`, `mg07_on_activate_dispatch`, `mg07_tick_system`) | exclusive | R5 demo surface, idle in production (#5842) |
| `Update` | `make_animation_system` | declared | Advances `AnimationPlayer`/`AnimationStack`, writes `Transform` + all animated-channel storages |
| `Update` | cinematics (`cinematic_root_motion_system`, `graph_root_motion_system`, `cinematic_animation_event_system`, `image_space_modifier_system`) | exclusive (+ declared) | Clip text keys become behavior/Papyrus completion notifications before Late drains them; a graph-driven actor is carried by its clips' root motion; the player's cinematic animation events; freshly delivered `ImageSpaceModifier.Apply` callbacks sample in this rendered frame |
| `Update` | `spin_system` + `animate_lights_system` | exclusive | Demo cube spin; procedural candle/chandelier flicker (Phase 17) |
| `Update` | `loading_model_turntable_system` | exclusive | Creation-era loading-screen turntable: spins the `LoadingModelStage` entity at `LOADING_TURNTABLE_RAD_PER_S` unless it opts out via `no_rotation` (e60911864); reads the `LoadingCoverClock` resource, not scheduler `dt` — the cover pins `dt` to 0.0 for exactly the frames the stage exists (#5306); same marker-gated single-entity shape as `spin_system`, sequenced after the Update batch |
| `Update` | `head_look_system` + `pending_group_swap_dispatch` | exclusive | M47.3 ObScript `Look` turns head bones on top of the animated pose; `PlayGroup <group> 0` starts the held sequence at the cycle boundary the animation batch just crossed (#4128) |
| `Update` | `player_body_facing_system` | exclusive + declared | Third-person body facing from the shared yaw accumulator (#4995); Update placement so PostUpdate propagation resolves it same-frame |
| `PostUpdate` | `make_transform_propagation_system()` | declared | Parent→child `GlobalTransform` (BFS) |
| `PostUpdate` | `particle_system` | exclusive + declared | Needs final emitter world origin (#401); consumes `ParticleEmitter::rate` one frame after Late's submersion write (#3653) |
| `PostUpdate` | AI procedures (`make_sandbox_seat_system()`, `make_wander_system()`, `make_travel_system()`, `make_follow_system()`, `make_escort_system()`, `make_guard_system()`, `make_flee_package_system()`, `make_find_package_system()`, `make_patrol_system()`, `make_eat_sleep_system()`), then `make_npc_walk_animation_system()`, `console_walk_system`, `graph_locomotion_system` | exclusive | **Default-on since M42.10** — `BYRO_NO_AI_LOCOMOTION=1` is the single kill-switch. Walk playback registers after the procedures because it classifies motion from this frame's final position; `console_walk_system` and `graph_locomotion_system` follow it for the actors it does not drive (behavior-graph `Speed` / `Direction`, Fallout 4) |
| `PostUpdate` | `make_combat_feedback_system()` | exclusive | Attack/hit/death clip takes + combat sounds; deliberately outside the kill-switch (#4709), after walk playback when registered |
| `PostUpdate` | `make_world_bound_propagation_system()` | exclusive + declared | Runs last so it sees final PostUpdate world transforms (#217); billboard rotations moved to Late (#3652) |
| `Physics` | `physics_sync_system` | declared | Rapier step → `Transform` writeback |
| `Late` | `camera_follow_system` | declared | M28.5 — runs after physics settle |
| `Late` | `make_billboard_system()` | exclusive + declared | Overwrites computed world rotation (#225); moved from PostUpdate (#3652) to read this frame's camera pose |
| `Late` | `footstep_system` + `impact_audio_system` | exclusive + declared | Fires stride-crossing footsteps from each emitter's propagated `GlobalTransform` (#848); since #5146 the character-mode emitter rides the player body, while the FlyCam camera emitter keeps the original mover role; moved from PostUpdate (#3652) for the same pose-staleness reason. Clutter impact sounds from the contact-force crossings Physics collected run right after, so the two audio consumers never interleave |
| `Late` | ragdoll, submersion, water and the death reconciler (`ragdoll_writeback_system`, `submersion_system`, `water_damage_system`, `scripted_experience_system`, `scripted_skill_system`, `scripted_kill_system`, `reconcile_pending_dead_actors_system`, `make_water_interaction_system()`, `water_audio_system`) | exclusive (+ declared) | Applies the final physics pose and bridges water contacts into gameplay/presentation events; script XP grants, `AdvanceSkill` and `Kill` requests (M47.2) land before the shared dead-actor reconciler that combat and save-load also use (#3119) |
| `Late` | `reverb_zone_system` | declared | Cell-acoustics → audio reverb send (M44 Phase 6) |
| `Late` | `audio_system` | exclusive | Listener pose / emitter update (M44) |
| `Late` | `log_stats_system` + `metrics_sample_system` | declared | Periodic stats log; ~2 Hz CPU/RAM/VRAM/GPU snapshot for the debug UI |
| `Late` | extension sync, script providers, first dispatchers (`extension_engine_settings_sync_system`, `extension_content_catalog_sync_system`, `extension_input_bindings_sync_system`, `extension_player_entity_sync_system`, `legacy_obscript_load_order_system`, `papyrus_provider_system`, `extension_custom_event_dispatch_system`, `extension_activation_dispatch_system`, `extension_cell_load_dispatch_system`) | exclusive + declared | Settings/catalog/input/player-entity sync, translated load-order ObScript, manifest Papyrus provider, custom-event/activation/cell-load delivery |
| `Late` | `equipment_appearance_system` + `make_npc_dialogue_selection_system()` | exclusive | P3 re-equip reconcile and P4 NPC activation → topic selection + INFO-fragment dispatch (#5152), before end-of-Late cleanup drains the markers |
| `Late` | remaining extension dispatchers (`extension_equipment_dispatch_system`, `extension_input_dispatch_system`, `extension_session_dispatch_system`, `extension_hit_dispatch_system`, `extension_update_dispatch_system`, `extension_setting_write_apply_system`) | exclusive | The rest of the `ExtensionHostSlot` dispatch family |
| `Late` | `obscript_item_system` | exclusive | M47.3: the scripts of inventory items hear this frame's item and equipment batches (`OnAdd` / `OnDrop` / `OnEquip` / `OnUnequip`) before cleanup drains them |
| `Late` | `event_cleanup_system` | exclusive | Drops transient marker components |

The debug server (feature-gated) may register additional systems via
`byroredux_debug_server::start(&mut scheduler, port)`.

The schedule is identical whether the scene is a single sweetroll or a
full FNV interior — it isn't customised per entry point. Systems whose
work is scene-dependent (animation with no clips, streaming, footsteps
with no emitters) early-return cheaply.

There is **no standalone *input_system***: raw input is captured directly
in `window_event` / `device_event` into the `InputState` resource, which
`fly_camera_system` (inside `player_controller_system`) consumes the next
frame.

## Event Flow

The `App` struct implements `winit::application::ApplicationHandler`, so
each event lands on a method.

### `resumed` (once, on first window creation)

```
1. Create the winit Window (via byroredux_platform::window::create_window)
2. Get raw display + window handles
3. VulkanContext::new(display, window_handle, [w, h])  (full init chain — see renderer.md)
4. Insert renderer-derived resources: ScreenshotBridge, AllocatorResource,
   GpuMemoryBudget
5. ctx.init_egui(...) + DebugUiState::new(...)  (debug-UI overlay)
6. setup_scene()  ← cell loader / NIF demo / SWF menu hook in here
7. scheduler.run(&world, 0.0)  ← prime transform/bound state before frame 0
8. last_frame = Instant::now()
```

Vulkan needs the window handles, so context creation has to wait until
`resumed`. Mesh and texture uploads also need the device, so any "load
this NIF and upload it to the GPU" work runs through `setup_scene` here,
not at startup. The ECS world itself is built in `App::new` before the
renderer exists, so non-GPU state is ready immediately. A single
`scheduler.run(&world, 0.0)` is fired at the end of `resumed` to prime
transform/bound propagation before the first rendered frame
(M41.0 Phase 1b.x).

### `about_to_wait` (every frame — the per-frame tick)

This is the heart of the game loop. Since Phase 14, **rendering is driven
from here**, not from `RedrawRequested` (on Wayland + winit 0.30 the
`request_redraw()` → compositor frame-callback → `RedrawRequested`
round-trip gated the loop at the compositor's pace; driving the draw from
`about_to_wait` uncaps it while MAILBOX present mode still vsyncs the
actual presentation).

```
1. now = Instant::now(); dt = selected bench mode (or wall-clock outside a bench)
     renderer-static=0; renderer-stepped=1/60; system-live=now-last_frame
2. world_resource_set::<DeltaTime>  / ::<TotalTime>   (interior mutability)
3. Refresh DebugStats (frame time, entity count, meshes/textures in use,
     registry counts, SkinSlotPool telemetry), ScratchTelemetry,
     SkinCoverageStats
4. step_bench_camera()              ← select the deterministic pose before systems
5. scheduler.run(&world, dt)        ← all ECS systems execute here unless simulation is paused
6. restore_bench_camera_pose()      ← undo character-camera replacement for deterministic benches
7. step_streaming()                 ← M40 exterior cell stream (no-op otherwise)
8. step_debug_loads()               ← drain debug-UI / debug-server load queue
9. step_upscaler_switch()           ← apply a queued TAA/FSR change at the frame boundary
10. capture_player_pose(); step_save_loads()
                                    ← snapshot current pose, then drain one queued live save/load
11. step_cell_transition()          ← drain door.teleport interior↔exterior swap
12. Update the window title (~4×/sec) if EngineConfig.debug_logging
13. render_one_frame(event_loop)    ← build render data + draw + present
14. --bench-frames: on the target frame, hash renderer-facing scene state,
      print the named-mode bench summary
      (and screenshot if requested), then exit unless --bench-hold
```

### `render_one_frame` (called from `about_to_wait`, Phase 14)

This is the former `RedrawRequested` body, pulled out as an inherent
method on `App` and bracketed into three phases (Phase 15) for the
debug-UI Metrics panel:

```
1. Build the debug-UI snapshot + run egui → PanelOutputs; apply outputs
2. ctx.submit_egui_frame(...) if the overlay produced a frame
3. build_render_data(&world, ...) — collect draw + water commands, lights,
     bone-world matrices, skin offsets, material table
4. Mirror material-table telemetry into ScratchTelemetry
5. Rebuild the geometry SSBO if dirty
6. Tick + render the UI overlay (Ruffle SWF player), upload its texture
7. ctx.draw_frame(...) — record + submit + present; capture FrameTimings
8. Feed CpuFrameTimings back into the World (fence wait / submit-present /
     between-frames)
```

`build_render_data` lives in [`byroredux/src/render/`](../../byroredux/src/render)
(the `render.rs` file was split into a `render/` module tree:
`static_meshes.rs`, `skinned.rs`, `particles.rs`, `lights.rs`, `water.rs`,
`sky.rs`, `camera.rs`, plus `mod.rs`). It walks the ECS once per frame to
collect visible meshes, their `MeshHandle`s, world transforms, materials,
lights, decal flags, water planes, and skinned-mesh bone data. The output
is plain owned data (`Vec<DrawCommand>`, `Vec<WaterDrawCommand>`,
`Vec<GpuLight>`, the bone-world matrix scratch, the `MaterialTable`), so
the renderer can chew on it without holding any ECS locks during command
recording. Those scratch buffers are owned on `App` and reused frame to
frame so their allocations persist (#243 / #253 / #509).

### `WindowEvent::RedrawRequested`

```
(empty — Phase 14)
```

The OS still fires this on window expose / resize / first paint, but the
body is intentionally bare: the next `about_to_wait` tick does the render
work. The arm is kept (not removed) only to keep the match exhaustive.

### `WindowEvent::Resized`

```
1. ctx.recreate_swapchain([w, h])
2. Update the active camera's aspect ratio
```

See [Vulkan Renderer — Resize](renderer.md#resize) for the atomic-handoff
details.

### `WindowEvent::CloseRequested`

```
1. Unload every streamed cell (cell_loader::unload_cell) + flush_pending_destroys
     (M40 — releases per-cell BLAS/mesh/texture refs before ctx teardown)
2. streaming.shutdown(1s timeout)  — joins the worker thread cleanly
3. Drop the VulkanContext (waits for device idle, tears down in reverse)
4. Drop the window
5. event_loop.exit()
```

The streaming-shutdown sweep is load-bearing: every streamed cell owns GPU
resources released only via `unload_cell`, and skipping it makes the
allocator find dangling refs at context destruction and SIGSEGV as the
orphaned device handles get reaped (#732 / #856).

### Input events

`KeyboardInput` arrives via `window_event`; `MouseMotion` arrives via
`device_event`. Both are forwarded into the `InputState` resource, which
`fly_camera_system` (gated behind `player_controller_system`) consumes on
the next frame. Mouse motion is only applied while the cursor is grabbed.
Notable keybinds handled in `window_event`:

- **Escape** — toggle mouse capture (cursor grab + hide).
- **F** — toggle Walk ↔ Fly player mode (`toggle_player_mode`, M28.5).
- **F3** — toggle the egui debug overlay (`DebugUiState::toggle`).

When the debug overlay is visible and egui consumes an event (e.g. a click
inside an egui window), the rest of the input dispatch is skipped so the
fly camera doesn't fight an egui slider drag — except `CloseRequested` /
`Resized`, which always run.

## Resource update pattern

`about_to_wait` only has `&mut self` on `App`, but `World` queries use
`&self`. The two-line helper in
[`byroredux/src/helpers.rs`](../../byroredux/src/helpers.rs) threads the
mutation through interior mutability:

```rust
pub(crate) fn world_resource_set<R: Resource>(world: &World, f: impl FnOnce(&mut R)) {
    let mut guard = world.resource_mut::<R>();
    f(&mut guard);
}

// Usage:
world_resource_set::<DeltaTime>(&self.world, |r| r.0 = dt);
world_resource_set::<TotalTime>(&self.world, |r| r.0 += dt);
```

## App Struct

The `App` struct has grown well past the four-field sketch the earlier
version of this doc carried. The structurally important fields:

```rust
struct App {
    window: Option<Window>,
    renderer: Option<VulkanContext>,
    world: World,
    scheduler: Scheduler,
    last_frame: Instant,
    ui_manager: Option<UiManager>,          // Ruffle SWF overlay
    ui_texture_handle: Option<u32>,
    draw_commands: Vec<DrawCommand>,         // reused per-frame scratch
    water_commands: Vec<WaterDrawCommand>,   // reused per-frame scratch
    gpu_lights: Vec<GpuLight>,               // reused per-frame scratch
    bone_world: Vec<[[f32; 4]; 4]>,          // M29.5/M29.6 skinning palette
    skin_slot_pool: SkinSlotPool,            // persistent per-entity bone slots
    skin_offsets: HashMap<EntityId, u32>,
    material_table: MaterialTable,           // R1 deduplicated material SSBO
    streaming: Option<WorldStreamingState>,  // M40 exterior cell streaming
    // bench_* fields: --bench-frames / --bench-hold / --screenshot plumbing
    // camera_pos_override / camera_forward_override
    #[cfg(feature = "debug-server")] debug_server: Option<DebugServerHandle>,
    debug_ui: Option<DebugUiState>,          // egui overlay state
    debug_ui_refresh_entities: bool,
    last_redraw_end: Option<Instant>,        // between-frames timing
    // ... bench accumulators, screenshot deadline counters, etc.
}
```

`window` and `renderer` are `Option` because they're created in `resumed`
(winit requires the event loop to be running before window creation).
`world`, `scheduler`, and the scratch buffers are constructed in
`App::new` before the event loop starts. Scene setup is no longer a
deferred closure stored on `App` — `resumed` calls `App::setup_scene`,
which delegates to `scene::setup_scene(&mut world, ctx, ...)` once the
renderer is up; that's where the cell loader, the NIF demo, and the SWF
menu loader all hook in.

The per-frame work split across helper methods on `App`:

- `render_one_frame` — build render data, run egui, draw + present.
- `step_streaming` — M40 exterior cell streaming: drain worker payloads,
  diff the loaded cell set against the player's grid, dispatch load/unload.
- `step_debug_loads` — drain the `PendingDebugLoadSlot` populated by the
  debug-server's `LoadNif` / `LoadInteriorCell` / `LoadExteriorCell`
  handlers.
- `step_cell_transition` — drain a queued `PendingCellTransition` (posted
  by the `door.teleport` console command) and run the interior↔exterior
  swap orchestrator (M40 Phase 2 Stage 3).

## Per-frame timing reference

Bench-of-record native-TAA numbers (RTX 4070 Ti, Ryzen 9 7950X), from the
75-run stepped-camera refresh at commit `d9a201c71` (2026-10-22). See
[ROADMAP.md](../../ROADMAP.md) for the authoritative, continuously
refreshed bench table:

| Cell | Entities | FPS | Frame | Fence | Draws |
|---|---|---|---|---|---|
| FNV Prospector Saloon (interior) | 2359 | 83.8 | 11.93 ms | 9.30 ms | 814 |
| Skyrim SE Whiterun Bannered Mare (interior) | 5853 | 83.7 | 11.94 ms | 9.17 ms | 1289 |
| FO4 MedTekResearch01 (interior) | 24923 | 35.4 | 28.22 ms | 17.54 ms | 8062 |
| FO4 Dugout Inn (interior) | 9254 | 66.1 | 15.12 ms | 12.30 ms | 1401 |

These are medians of three 300-frame repetitions under
`renderer-stepped/orbit`; the full record also covers Cornell plus all four
FSR presets. Every repetition passed the entity/TLAS/state fingerprint gate.
The refresh was run against a same-machine control of the previous record
(`a3a7a43b0`), which found HEAD 1–9% slower on every scene (ROADMAP's
R6a-regress-23); absolute figures here move with scene populations, so compare
across records only through such a control.

The CPU game-loop work itself (scheduler.run + build_render_data) stays
well under the GPU path. The debug-UI Metrics panel breaks the per-frame
wall time into `atw_pre` / `atw_scheduler` / `atw_post` and the
`render_one_frame` pre-draw / draw-call / post-draw brackets, with
per-system scheduler timings (`SchedulerSystemTimings`) and CPU frame
timings (`CpuFrameTimings`, including the GPU fence wait) for finding which
phase dominates a given frame.
