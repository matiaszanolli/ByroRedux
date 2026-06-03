---
description: "Deep audit of the M44 audio subsystem — kira backend, spatial sub-tracks, listener pose, reverb send, streaming music"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Audio Subsystem Audit (M44)

Audit the `byroredux-audio` crate (M44) for correctness across the kira backend integration: graceful-degradation manager, listener pose sync, spatial sub-track lifecycle, one-shot dispatch (entity + queue paths), looping emitter sustain, streaming music, global reverb send, and ECS lifecycle of audio components across cell streaming.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, methodology, deduplication, context rules, and finding format.

## Scope

**Crate**: `crates/audio/src/lib.rs` (M44 — single-file crate today, with `crates/audio/src/tests.rs` split out post-Session-34; if `lib.rs` splits further into submodules update entry points before launching).

**Engine-side consumers** (Dimension 7 — outside the crate): `byroredux/src/systems/audio.rs` (`footstep_system` + `reverb_zone_system`) and `byroredux/src/components.rs` (`FootstepEmitter` / `FootstepConfig` / `FootstepScratch`). These are the only live callers of `play_oneshot` / `set_reverb_send_db`, so the crate API audit is incomplete without them.

**M44 phases shipped (ground truth — read crate docstring before audit)**:
- Phase 1 — `AudioWorld` resource (graceful-degradation `Option<AudioManager>`), `AudioListener` / `AudioEmitter` / `OneShotSound` components, `audio_system` skeleton
- Phase 2 — `load_sound_from_bytes` (symphonia decode of BSA-extracted blobs), `SoundCache` (path-keyed `Arc<StaticSoundData>` cache)
- Phase 3 — Real spatial playback through kira's spatial sub-track model + `spawn_oneshot_at` helper
- Phase 3.5 — `AudioWorld::play_oneshot` queue API (System-friendly, no entity allocation)
- Phase 4 — `AudioEmitter.looping` honored via kira `loop_region`; tweened stop on emitter-component removal
- Phase 5 — `load_streaming_sound_from_bytes` / `load_streaming_sound_from_file`, `play_music` / `stop_music` (single-slot, non-spatial main track)
- Phase 6 — `AudioWorld::set_reverb_send_db` (one global send track with `ReverbBuilder`, per-new-track opt-in via `with_send`; default `f32::NEG_INFINITY` = silent)

**Future phases (NOT yet shipped — do not flag as missing unless audit scope includes them)**: Phase 3.5b FOOT records → per-material sound, REGN ambient soundscapes, MUSC + hardcoded music routing, reverb zones keyed off cell acoustics, raycast occlusion attenuation.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3,5`). Default: all 7.
- `--depth shallow|deep`: `shallow` = check API contracts; `deep` = trace per-frame data flow + lifecycle. Default: `deep`.

## Extra Per-Finding Fields

- **Dimension**: Manager Lifecycle | Listener Pose Sync | Spatial Sub-Track Dispatch | Looping & Streaming | Reverb Send & Routing | ECS Lifecycle & Cell Streaming | Gameplay Audio Wiring

## Phase 1: Setup

1. Parse `$ARGUMENTS` for `--focus`, `--depth`
2. `mkdir -p /tmp/audit/audio`
3. Fetch dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/audio/issues.json`
4. Scan `docs/audits/` for prior audio reports (none expected — this is M44's first audit pass)
5. Read the `crates/audio/src/lib.rs` module docstring to confirm which phases are actually shipped vs. inferred from `HISTORY.md`. If the docstring drifts from the user-visible API, that's a finding in itself.

## Phase 2: Launch Dimension Agents

### Dimension 1: Manager Lifecycle & Graceful Degradation
**Entry points**: `crates/audio/src/lib.rs` — `AudioWorld::new`, `AudioWorld::default`, `is_active`, `Drop` (implicit via field drop order)
**Checklist**:
- `AudioManager::new(AudioManagerSettings::default())` failure path: `Option<AudioManager>` left as `None`, no panic. Booting on a headless server / CI / broken sound driver MUST succeed
- Every public API on `AudioWorld` (`play_oneshot`, `play_music`, `stop_music`, `set_reverb_send_db`, …) checks `manager.is_some()` (or equivalent `is_active()` gate) and no-ops cleanly when inactive — no `unwrap()` on `Option<AudioManager>`
- Field-drop order: `active_sounds` (owns `SpatialTrackHandle`s) → `pending_oneshots` → `music` → `reverb_send` → `listener` → `manager`. Rust drops struct fields in declaration order; the declaration order in `pub struct AudioWorld` MUST match the dependency order (track handles before listener before manager), or kira will assert-fail in Drop. Verify the declaration order has not been re-ordered "for readability"
- `AudioWorld` is `Resource` (interior mutability via `&self` resource access); audit any `&mut World` requirement that snuck back in
- `kira::AudioManagerSettings::default()` choice — verify default capacity bounds (`sound_capacity`, `sub_track_capacity`) are sufficient for cell-load bursts; document the cap
- Re-init path: `AudioWorld::new()` is documented to be called once at engine boot. Audit any code that calls it on cell transition or window resize — the manager owns the OS audio device, re-acquiring is expensive and may fail
**Output**: `/tmp/audit/audio/dim_1.md`

### Dimension 2: Listener Pose Sync
**Entry points**: `crates/audio/src/lib.rs` — `sync_listener_pose`, `AudioListener` component
**Checklist**:
- Listener is created **lazily** on the first frame an `AudioListener`-tagged entity has a resolved `GlobalTransform`. Before that, the audio system early-returns. Verify the lazy-creation path does NOT panic if the entity exists but `GlobalTransform` hasn't propagated yet (frame-1 cold-start race)
- Listener-entity selection: query `AudioListener` returns the FIRST entity in iteration order. If multiple `AudioListener` components exist (mod scenario, debug fly-cam swap), behavior is "whichever wins iteration." Audit whether the system warns once when count > 1, or silently picks. Either is defensible; not warning when count > 1 is the audit hazard
- `set_position` / `set_orientation` use `Tween::default()` every frame — this is correct for camera follow (smooth interpolation). Audit any path that switches to `Tween::IMMEDIATE` (would cause audible spatial jumps on warp / fast-travel)
- Listener orientation: kira expects a `Quat` for orientation; verify the `GlobalTransform.rotation` semantic matches kira's "forward = -Z, up = +Y" (or whatever the active kira contract is). Mismatch is left/right channel inversion across the whole soundscape — subtle, lethal
- `add_listener` failure (kira returns `Err`): logged as `warn!` and listener stays `None`. Verify the next frame retries (lazy create is idempotent on `None`) — without retry, a transient init failure permanently breaks audio for the session
- Listener handle drops BEFORE manager drops (covered in Dim 1 — note here as cross-dim invariant)
- `EntityId` of the listener is NOT cached across frames — re-resolved per tick. Verify, because caching would silently break debug fly-cam swaps
**Output**: `/tmp/audit/audio/dim_2.md`

### Dimension 3: Spatial Sub-Track Dispatch (One-Shot + Emitter Paths)
**Entry points**: `crates/audio/src/lib.rs` — `dispatch_new_oneshots`, `drain_pending_oneshots`, `spawn_oneshot_at`, `AudioWorld::play_oneshot`, `ActiveSound`, `PendingOneShot`
**Checklist**:
- Two dispatch paths exist and MUST stay observably equivalent:
  - **Entity path**: `OneShotSound + AudioEmitter` on an entity → `dispatch_new_oneshots` builds a per-emitter `SpatialTrackHandle`, plays the sound, removes `OneShotSound` (keeps `AudioEmitter` so callers can query active state)
  - **Queue path**: `AudioWorld::play_oneshot(sound, position, attenuation, volume)` enqueues `PendingOneShot` → drained at frame start by `drain_pending_oneshots` through the same spatial-sub-track shape, but `entity: None` on the resulting `ActiveSound`
- Both paths MUST route through a `SpatialTrackBuilder` anchored at the listener (not at the world origin) — the `listener_id` is required for kira's spatial scene to attach the sub-track. Verify each dispatch path queries `listener_id` and early-returns if absent (`crates/audio/src/lib.rs::drain_pending_oneshots` and `crates/audio/src/lib.rs::dispatch_new_oneshots` both gate on it)
- Per-sub-track reverb send: each `SpatialTrackBuilder` MUST opt into the global send via `.with_send(reverb_send_id, reverb_send_db)` at construction, NOT after — kira does not allow retroactive send level changes on a built track. Audit any path that builds a track without `with_send` (will not pick up reverb)
- `ActiveSound._track: SpatialTrackHandle` — held for Drop side effect. Removing the underscore prefix or accidentally `drop()`ing it tears down playback even while the `handle` still ticks. Verify the field is named `_track` and not just `track`
- `looping: bool` (Phase 4): when `true`, kira's `StaticSoundData::loop_region(..)` is applied at dispatch time. The prune sweep must distinguish `Stopped` for one-shots (natural termination → drop entry) vs `Stopped` for loops (caller-driven stop, e.g. cell unload → already-tweened-stop has completed → drop entry). A loop that reports `Stopped` without a prior caller-driven `stop()` is a bug — log as warn
- Drain cap: `crates/audio/src/lib.rs::drain_pending_oneshots` warns at WARN if a single tick drains > 32 items. Verify the threshold is meaningful for footstep tempo; a per-frame retrigger is the regression class to detect
- `Arc<StaticSoundData>` clone semantics: the same `Arc` can back many emitters without re-decoding (`SoundCache` invariant). Audit any path that clones `StaticSoundData` instead of cloning the `Arc`
- Source-position resolution: queue path takes `position: Vec3` directly (no entity lookup); entity path reads `GlobalTransform.translation`. Verify the entity path does NOT also accept a fallback `Transform` (interpolation-state mismatch — must use the post-propagation `GlobalTransform`)
**Output**: `/tmp/audit/audio/dim_3.md`

### Dimension 4: Looping Emitter Sustain & Streaming Music
**Entry points**: `crates/audio/src/lib.rs` — `prune_stopped_sounds`, `play_music`, `stop_music`, `load_streaming_sound_from_bytes`, `load_streaming_sound_from_file`
**Checklist**:
- Phase 4 looping: `AudioEmitter.looping = true` causes the prune sweep to issue a tweened `stop()` on the kira handle when the source entity has lost its `AudioEmitter` component (despawn-by-cell-unload, or explicit removal). Verify the tween duration is non-zero (instantaneous stop = audible click) AND that the next prune tick observes `Stopped` and drops the entry. Without the second tick, looping entries leak into the active list forever
- Single-slot music dispatch (Phase 5): `play_music` overwrites any currently-playing track with a tweened crossfade. Verify there is exactly ONE music slot — multi-slot music is not in scope for M44, but a regression that adds a second slot would not fail any test today
- **MUSC parse→play gap (re-scope, do NOT audit as a live path)**: cell-music FormIDs ARE parsed — `default_music` (ZNAM, `crates/plugin/src/esm/cell/wrld.rs:125`; field at `crates/plugin/src/esm/cell/mod.rs:744`) and `music_type_form` (XCMO, `wrld.rs:307`; field at `cell/mod.rs:169`) — but NO caller invokes `AudioWorld::play_music` (grep `play_music` across `byroredux/` returns zero hits; it is defined only at `crates/audio/src/lib.rs:416`). Treat cross-cell music continuity below as an explicit future-phase gap (MUSC routing on the FUTURE list), not a reachable path. The audit task is to confirm the parse→play wiring is absent (so the single-slot / main-track invariants are pinned for the eventual caller), NOT to trace a flow that has no producer
- Music routes through the **main track** (non-spatial), NOT a spatial sub-track. Audit any path that puts music into the spatial scene — it would attenuate with listener distance, which is wrong for non-diegetic music
- `StreamingSoundData` (kira) does NOT buffer the full decompressed PCM. Verify `load_streaming_sound_from_bytes` uses `StreamingSoundData::from_cursor`, not `StaticSoundData::from_cursor` — the latter buffers everything, OOM risk on multi-minute music tracks
- File overload (`load_streaming_sound_from_file`): verify it resolves loose `Data/Music/*.mp3` / `*.wav` paths through the same path-resolution discipline as the BSA extractor (case-insensitive on Windows-derived game data, etc.)
- `stop_music` fade duration: configurable. Audit default value; instant stop is a regression
- Music handle field-drop: `music: Option<StreamingSoundHandle<...>>` drops before `manager` drops (Dim 1 invariant)
**Output**: `/tmp/audit/audio/dim_4.md`

### Dimension 5: Reverb Send & Routing (Phase 6)
**Entry points**: `crates/audio/src/lib.rs` — `AudioWorld::new` (send track creation), `set_reverb_send_db`, every `SpatialTrackBuilder` site (`with_send` opt-in)
**Checklist**:
- One global send track is created at manager init with `SendTrackBuilder` + `ReverbBuilder` effect at full-wet output. `reverb_send: Option<SendTrackHandle>` is `None` if creation failed (e.g. manager itself was inactive) — verify this fallback path does NOT cascade into any later `unwrap()`
- Default `reverb_send_db = f32::NEG_INFINITY` ("silent / reverb off"). Engine boots with NO audible reverb. Audit any path that initialises to a finite default (would mean every cell sounds wet from frame 1)
- Per-new-track send level: applied at `with_send` construction time. **Already-playing sounds keep their construction-time level** — this is documented in the crate docstring (lib.rs:101-105). For short SFX (footsteps, gunshots) the level naturally refreshes as new sounds replace old ones; for long sustains (looping ambients) a level change won't apply until the loop is restarted. Audit findings claiming "reverb level changes don't take effect immediately" are stale premise — verify they actually need immediate application, then propose per-track level injection (not in M44 scope)
- Cell-load → reverb level toggle: the interior detector is `byroredux/src/systems/audio.rs::reverb_zone_system` (#846). It watches `CellLightingRes::is_interior` and flips `set_reverb_send_db(INTERIOR_REVERB_SEND_DB = -12.0)` for interiors, back to `EXTERIOR_REVERB_SEND_DB = f32::NEG_INFINITY` for exteriors (both `const` local to the fn). Confirm: (a) the transition is bit-equality gated (`reverb_send_db().to_bits() == target_db.to_bits()` short-circuits the no-op tick — verify the gate stays bit-equal so `NEG_INFINITY → NEG_INFINITY` never re-touches the field); (b) the system no-ops safely when `CellLightingRes` is absent (engine boot pre-cell-load) AND when `AudioWorld` is absent (headless); (c) it is registered to write `AudioWorld` BEFORE `audio_system` constructs any new spatial track this frame (it runs exclusive at `Stage::Late`, ahead of `audio_system` — see `byroredux/src/main.rs:779` vs `:801`). Without ordering, a sound built this tick picks up last tick's send level. See Dimension 7 for the full detector audit
- `set_reverb_send_db(f32::NEG_INFINITY)` should disable routing for new tracks (kira's send semantic: -∞ dB = silent). Audit whether kira clamps `-inf` to a finite floor (some send-bus implementations do); if so, the silent default is leaky
- Reverb-send field-drop ordering: drops between `music` and `listener` (per Dim 1). Verify
**Output**: `/tmp/audit/audio/dim_5.md`

### Dimension 6: ECS Lifecycle & Cell Streaming (M40 Interaction)
**Entry points**: `crates/audio/src/lib.rs` (+ `crates/audio/src/tests.rs` post-Session-34 split) — `audio_system`, `prune_stopped_sounds`, `OneShotSound`, `AudioEmitter`; cross-cut: `byroredux/src/streaming.rs` (cell unload), `byroredux/src/cell_loader/{load,unload}.rs` (cell load/unload — was monolithic cell_loader.rs pre-Session-34)
**Checklist**:
- `audio_system` is documented to run at `Stage::Late` (after transform propagation produces final world poses). Verify the Schedule registration matches (`byroredux/src/main.rs:801`, `scheduler.add_exclusive(Stage::Late, byroredux_audio::audio_system)`); running before transform propagation reads stale `GlobalTransform` for the listener and emitters
- **Cross-stage producer→consumer ordering (footstep → audio)**: `footstep_system` is registered exclusive at `Stage::PostUpdate` (`byroredux/src/main.rs:718`) and ENQUEUES `PendingOneShot` via `AudioWorld::play_oneshot`; `audio_system` DRAINS the queue at `Stage::Late` (`byroredux/src/main.rs:801`), strictly later the SAME frame. Verify `PostUpdate` precedes `Late` in the stage order so a footstep emitted in PostUpdate is heard the same tick — never lags a frame and never silently drops on a stage reorder. `footstep_system` is also exclusive specifically so it sequences AFTER `make_transform_propagation_system` produces the final pose (pre-#848 it ran in `Stage::Update` ahead of propagation → footstep landed at last-frame's pose). Audit any move of either system out of its stage
- `OneShotSound` marker lifecycle: removed by `dispatch_new_oneshots` after dispatch (single-frame). Audit any path that retains the marker across frames (would re-trigger the sound every frame)
- `AudioEmitter` lifecycle: stays on the entity for the duration of playback (Phase 3 contract — callers can query "is this entity still playing?"). When the playback handle reports `Stopped`, the prune pass removes `AudioEmitter`. Verify a downstream cleanup system can despawn the now-empty entity without coupling to audio state
- Cell unload (M40 streaming): when a cell unloads, the entities carrying `AudioEmitter` are despawned. The prune pass MUST observe loops still attached to despawned entities and issue tweened stops, NOT leave orphan kira handles ticking. The `entity: Option<EntityId>` field on `ActiveSound` is the link — verify the prune pass checks entity-still-alive AND component-still-present (despawn drops both, but a partial-despawn — emitter removed but entity alive — is the corner case)
- Cross-cell-load music continuity: music does NOT despawn on cell transition (single-slot, lives in `AudioWorld` resource). The "gate `play_music` on is-the-new-cell's-music-different" invariant is a FUTURE-phase contract — no cell-load caller reaches `play_music` today (see Dim 4 MUSC parse→play gap). Audit this as "the eventual MUSC caller MUST gate on FormID equality" — re-loading the same `StreamingSoundHandle` causes a re-decode + re-stream — rather than as a present regression surface
- Listener entity despawn: if the entity carrying `AudioListener` is despawned (debug fly-cam destroy, world reset), the next `sync_listener_pose` early-returns. Verify the listener handle is cleaned up — it should drop with `AudioWorld` at engine shutdown, but a despawn-then-respawn cycle should not leak listener handles
- `SoundCache` (Resource): process-lifetime path-keyed cache of decoded `Arc<StaticSoundData>`. Audit growth bound — should NOT grow per cell load (interning by lowercased path is the de-dupe mechanism, mirroring `AnimationClipRegistry` #790). If a cache `clear()` ever lands, verify it does NOT invalidate `Arc`s currently held by `ActiveSound` entries
- Send-track lifetime: outlives every spatial sub-track that opted into it. Verify `reverb_send` is dropped strictly after `active_sounds` (Dim 1 invariant)
- Future phase guard: when REGN ambient soundscapes (Phase 4 of the future-phases list) lands, expect a per-region `AudioEmitter` spawn-on-cell-enter / despawn-on-cell-leave pattern — this dimension's lifecycle invariants are the contract that will be tested then
**Output**: `/tmp/audit/audio/dim_6.md`

### Dimension 7: Gameplay Audio Wiring (Engine-Side Consumers)
**Entry points**: `byroredux/src/systems/audio.rs` — `footstep_system`, `reverb_zone_system`; `byroredux/src/components.rs` — `FootstepEmitter`, `FootstepConfig`, `FootstepScratch`; `byroredux/src/scene.rs` (camera opt-in), `byroredux/src/asset_provider.rs` (`FootstepConfig.default_sound` population)
**Why this dimension**: the M44 audio CRATE (Dims 1–6) is the producer of the `play_oneshot` / reverb API; the consumer that actually DRIVES it lives outside the crate, in `byroredux/src/systems/audio.rs`. `footstep_system` is the ONLY live `play_oneshot` caller in the engine, and `reverb_zone_system` is the ONLY caller of `set_reverb_send_db` on cell load. Neither was covered by the crate-scoped dimensions.
**Checklist**:
- **Footstep stride accumulation**: `footstep_system` walks every `FootstepEmitter`, accumulates XZ-plane (horizontal only — Y motion is not a step) movement against `FootstepEmitter.stride_threshold`, and fires one `play_oneshot` per threshold crossing. Verify only horizontal distance accumulates and `accumulated_stride` resets to `0.0` on fire (not subtracts — a subtract would carry over fractional stride and double-fire on a single large jump)
- **First-tick seed (#848)**: on the first tick an emitter is seen (`!fs.initialised`), the system seeds `last_position = pos`, sets `initialised = true`, and `continue`s WITHOUT accumulating — otherwise the cold-start delta from origin to spawn would fire a phantom footstep. Verify the seed path exists and the seeded frame produces zero triggers
- **`FootstepScratch` Vec reuse (#932)**: the per-tick trigger buffer lives in the `FootstepScratch` Resource (`triggers: Vec<Vec3>`, `Vec::with_capacity(32)` default) and is `clear()`-reused + `std::mem::take`-drained, NOT freshly allocated each frame. Verify the buffer's heap allocation is restored to the resource after dispatch (the `try_resource_mut::<FootstepScratch>()` re-acquire at fn end) on BOTH the success path and the `AudioWorld`-absent bail path — dropping the moved-out `Vec` would strand the capacity and re-allocate next frame
- **Lock-drop ordering (query_mut → play_oneshot)**: Phase 1 holds `GlobalTransform` (read) + `FootstepEmitter` (write) concurrently, then RELEASES both before Phase 2 touches `AudioWorld`. The `FootstepScratch` mut-lock is also dropped (`drop(scratch)`) BEFORE `AudioWorld` is acquired — holding two resource-mut locks at once would force the TypeId-sorted acquisition contract. Verify no path holds a component query lock across a `play_oneshot` call (would serialise audio dispatch against the propagation read set)
- **Footstep attenuation**: each footstep `play_oneshot` uses a tight `Attenuation { min_distance: 0.5, max_distance: 12.0 }` (tighter than the engine default — footsteps drop off fast). Audit any change to a wider falloff (would make distant NPC footsteps audible across a whole interior)
- **Silent no-op contracts**: `footstep_system` returns early and silently when `FootstepConfig` is absent, when `FootstepConfig.default_sound` is `None` (no decoded footstep sound resolved — see `byroredux/src/asset_provider.rs`), when `FootstepScratch` is absent, when the emitter/transform queries fail, or when `AudioWorld` is absent. Verify NONE of these paths panic or log per-frame spam — the engine must boot and run with audio off
- **Camera opt-in**: the player/fly-cam entity is given `FootstepEmitter` in `byroredux/src/scene.rs` (`world.insert(cam, crate::components::FootstepEmitter::new())`). Verify the opt-in is component-driven (no hardcoded camera-entity assumption inside the system), so NPCs can carry `FootstepEmitter` under future REGN/AI work
- **`reverb_zone_system` (#846)** — the interior-reverb detector (full pins in Dim 5): re-confirm here as a gameplay-wiring consumer that `INTERIOR_REVERB_SEND_DB = -12.0` / `EXTERIOR_REVERB_SEND_DB = f32::NEG_INFINITY`, the bit-equality transition gate, the `CellLightingRes`-absent and `AudioWorld`-absent safe no-ops, and the `Stage::Late` registration ahead of `audio_system`
**Output**: `/tmp/audit/audio/dim_7.md`

## Phase 3: Merge

1. Read all `/tmp/audit/audio/dim_*.md` files
2. Combine into `docs/audits/AUDIT_AUDIO_<TODAY>.md` with structure:
   - **Executive Summary** — Crate phases shipped (1–6) + engine-side consumers shipped (Phase 3.5 footstep gameplay loop via `footstep_system`, Phase 6 reverb detector via `reverb_zone_system`) vs phases stubbed (3.5b, REGN, MUSC routing). Note the MUSC parse→play gap explicitly (FormIDs parsed, no `play_music` caller). Findings count by severity. Headless-mode boot status (must be PASS).
   - **Lifecycle Invariant Matrix** — Field-drop order × verified / drifted, per-handle owner × renderer/audio crate.
   - **Findings** — Grouped by severity (CRITICAL first), deduplicated.
   - **Future-Phase Readiness** — What invariants this audit pinned that the next phase (FOOT/3.5b material sounds / REGN / MUSC / occlusion) will rely on.
3. Remove cross-dimension duplicates (Dim 1's field-drop order shows up in Dims 4 + 5 + 6 — collapse into Dim 1's row; `reverb_zone_system` is pinned in both Dim 5 and Dim 7 — keep the full detector audit in Dim 7, leave Dim 5's pointer to it)

## Phase 4: Cleanup

1. `rm -rf /tmp/audit/audio`
2. Inform user the report is ready
3. Suggest: `/audit-publish docs/audits/AUDIT_AUDIO_<TODAY>.md`
