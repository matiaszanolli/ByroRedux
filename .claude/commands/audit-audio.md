---
description: "Deep audit of the M44 audio subsystem — kira backend, spatial sub-tracks, listener pose, reverb send, streaming music"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Audio Subsystem Audit (M44)

Audit the `byroredux-audio` crate (M44) for correctness across the kira backend integration: graceful-degradation manager, listener pose sync, spatial sub-track lifecycle, one-shot dispatch (entity + queue paths), looping emitter sustain, streaming music, global reverb send, and ECS lifecycle of audio components across cell streaming.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, methodology, deduplication, context rules, and finding format.

## Scope

**Crate**: `crates/audio/src/lib.rs` (M44 — single-file crate today; if it splits into submodules update entry points before launching).

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

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3,5`). Default: all 6.
- `--depth shallow|deep`: `shallow` = check API contracts; `deep` = trace per-frame data flow + lifecycle. Default: `deep`.

## Extra Per-Finding Fields

- **Dimension**: Manager Lifecycle | Listener Pose Sync | Spatial Sub-Track Dispatch | Looping & Streaming | Reverb Send & Routing | ECS Lifecycle & Cell Streaming

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
- Cell-load → reverb level toggle: an interior detector that runs after `cell_loader` finishes flips `set_reverb_send_db(-12.0)` for interiors, back to `f32::NEG_INFINITY` for exteriors. Verify the detector exists (or is explicitly stubbed) — without it, every cell sounds the same
- `set_reverb_send_db(f32::NEG_INFINITY)` should disable routing for new tracks (kira's send semantic: -∞ dB = silent). Audit whether kira clamps `-inf` to a finite floor (some send-bus implementations do); if so, the silent default is leaky
- Reverb-send field-drop ordering: drops between `music` and `listener` (per Dim 1). Verify
**Output**: `/tmp/audit/audio/dim_5.md`

### Dimension 6: ECS Lifecycle & Cell Streaming (M40 Interaction)
**Entry points**: `crates/audio/src/lib.rs` (+ `crates/audio/src/tests.rs` post-Session-34 split) — `audio_system`, `prune_stopped_sounds`, `OneShotSound`, `AudioEmitter`; cross-cut: `byroredux/src/streaming.rs` (cell unload), `byroredux/src/cell_loader/{load,unload}.rs` (cell load/unload — was monolithic cell_loader.rs pre-Session-34)
**Checklist**:
- `audio_system` is documented to run at `Stage::Late` (after transform propagation produces final world poses). Verify the Schedule registration matches; running before transform propagation reads stale `GlobalTransform` for the listener and emitters
- `OneShotSound` marker lifecycle: removed by `dispatch_new_oneshots` after dispatch (single-frame). Audit any path that retains the marker across frames (would re-trigger the sound every frame)
- `AudioEmitter` lifecycle: stays on the entity for the duration of playback (Phase 3 contract — callers can query "is this entity still playing?"). When the playback handle reports `Stopped`, the prune pass removes `AudioEmitter`. Verify a downstream cleanup system can despawn the now-empty entity without coupling to audio state
- Cell unload (M40 streaming): when a cell unloads, the entities carrying `AudioEmitter` are despawned. The prune pass MUST observe loops still attached to despawned entities and issue tweened stops, NOT leave orphan kira handles ticking. The `entity: Option<EntityId>` field on `ActiveSound` is the link — verify the prune pass checks entity-still-alive AND component-still-present (despawn drops both, but a partial-despawn — emitter removed but entity alive — is the corner case)
- Cross-cell-load music continuity: music does NOT despawn on cell transition (single-slot, lives in `AudioWorld` resource). Audit whether `play_music` on cell-load is gated on "is the new cell's music different" — re-loading the same `StreamingSoundHandle` causes a re-decode + re-stream
- Listener entity despawn: if the entity carrying `AudioListener` is despawned (debug fly-cam destroy, world reset), the next `sync_listener_pose` early-returns. Verify the listener handle is cleaned up — it should drop with `AudioWorld` at engine shutdown, but a despawn-then-respawn cycle should not leak listener handles
- `SoundCache` (Resource): process-lifetime path-keyed cache of decoded `Arc<StaticSoundData>`. Audit growth bound — should NOT grow per cell load (interning by lowercased path is the de-dupe mechanism, mirroring `AnimationClipRegistry` #790). If a cache `clear()` ever lands, verify it does NOT invalidate `Arc`s currently held by `ActiveSound` entries
- Send-track lifetime: outlives every spatial sub-track that opted into it. Verify `reverb_send` is dropped strictly after `active_sounds` (Dim 1 invariant)
- Future phase guard: when REGN ambient soundscapes (Phase 4 of the future-phases list) lands, expect a per-region `AudioEmitter` spawn-on-cell-enter / despawn-on-cell-leave pattern — this dimension's lifecycle invariants are the contract that will be tested then
**Output**: `/tmp/audit/audio/dim_6.md`

## Phase 3: Merge

1. Read all `/tmp/audit/audio/dim_*.md` files
2. Combine into `docs/audits/AUDIT_AUDIO_<TODAY>.md` with structure:
   - **Executive Summary** — Phases shipped (1–6) vs phases stubbed (3.5b, REGN, MUSC). Findings count by severity. Headless-mode boot status (must be PASS).
   - **Lifecycle Invariant Matrix** — Field-drop order × verified / drifted, per-handle owner × renderer/audio crate.
   - **Findings** — Grouped by severity (CRITICAL first), deduplicated.
   - **Future-Phase Readiness** — What invariants this audit pinned that the next phase (FOOT / REGN / MUSC / occlusion) will rely on.
3. Remove cross-dimension duplicates (Dim 1's field-drop order shows up in Dims 4 + 5 + 6 — collapse into Dim 1's row)

## Phase 4: Cleanup

1. `rm -rf /tmp/audit/audio`
2. Inform user the report is ready
3. Suggest: `/audit-publish docs/audits/AUDIT_AUDIO_<TODAY>.md`
