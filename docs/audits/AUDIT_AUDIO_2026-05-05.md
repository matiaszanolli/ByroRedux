# Audio Subsystem Audit (M44) — 2026-05-05

Static, source-only audit of the M44 `byroredux-audio` crate
(`crates/audio/src/lib.rs`, ~1829 lines, single file). Cross-references
the engine binary (`byroredux/src/main.rs` schedule registration,
`byroredux/src/scene.rs` listener wiring, `byroredux/src/systems.rs`
footstep dispatch) and the kira 0.10.8 source under
`~/.cargo/registry/src/.../kira-0.10.8/`.

Audit scope: all 6 dimensions, depth=deep. No `cargo test` was run —
this is a source-correctness pass.

## Executive Summary

**Phases shipped (per crate docstring, lines 9–113)**:
- Phase 1: AudioWorld resource, AudioListener / AudioEmitter / OneShotSound
  components, audio_system skeleton. ✓
- Phase 2: `load_sound_from_bytes` (symphonia decode) + SoundCache. ✓
- Phase 3: Spatial sub-track playback + `spawn_oneshot_at`. ✓
- Phase 3.5: `AudioWorld::play_oneshot` queue API. ✓
- Phase 4: Looping emitters via kira `loop_region(..)`; tweened stop on
  emitter-component removal. ✓
- Phase 5: `load_streaming_sound_from_bytes` /
  `load_streaming_sound_from_file`, `play_music` / `stop_music`
  (single-slot, non-spatial main-track dispatch). ✓
- Phase 6: One global send track with `ReverbBuilder`, per-new-track
  `with_send` opt-in at construction, default `f32::NEG_INFINITY`
  silent-of-reverb. ✓

**Phases stubbed (out of scope, do NOT flag as missing)**:
- Phase 3.5b FOOT records → per-material footstep sound dispatch.
- Phase 4 future: REGN ambient soundscapes (region-based ambient layers).
- Phase 5 future: MUSC + hardcoded music routing (single-slot crossfade
  is shipped; MUSC-record-driven selection is not).
- Phase 6 future: cell-load-keyed reverb zones, raycast occlusion attenuation.

**Headless-mode boot status**: **PASS**. `AudioWorld::new()` falls back
to `manager: None` on `AudioManager::new` failure (lib.rs:230–245), the
`reverb_send` builder also falls back cleanly (lib.rs:251–269), and
every public method that touches the kira manager begins with
`let Some(mgr) = ... else { return; };` or an `is_active()` short-circuit
in `audio_system` (lib.rs:542–544). Test
`audio_world_constructs_without_panic_on_any_environment` (lib.rs:1051)
plus `audio_system_no_op_when_audio_world_inactive` (lib.rs:1232) pin
the no-device contract. No `unwrap()` on the inner `Option<AudioManager>`
appears anywhere in the crate.

**Findings count by severity**:
- CRITICAL: 0
- HIGH: 1
- MEDIUM: 4
- LOW: 6
- Total: 11

The single HIGH is a kira-default-capacity ceiling that becomes
reachable on populated cell loads. No CRITICAL items — the lifecycle
shape is sound, headless boot is clean, and the Phase 6 default of
`f32::NEG_INFINITY` keeps reverb silent until a future cell-load
detector flips it for interiors.

## Lifecycle Invariant Matrix

Field-drop order is determined by Rust struct-field declaration order
(top-to-bottom). The current declaration in `pub struct AudioWorld`
(lib.rs:185–217):

| Order | Field | Type | Drop side effect | Status |
| ----- | ----- | ---- | ---------------- | ------ |
| 1 | `active_sounds` | `Vec<ActiveSound>` (each owns `StaticSoundHandle` + `SpatialTrackHandle`) | drops handles → kira marks resources for removal | ✓ verified |
| 2 | `pending_oneshots` | `Vec<PendingOneShot>` (data only, no kira handles) | none | ✓ verified |
| 3 | `music` | `Option<StreamingSoundHandle<FromFileError>>` | none (kira `StreamingSoundHandle` has no `Drop`; `Arc<Shared>` mark-for-removal is internal) | ✓ verified |
| 4 | `reverb_send` | `Option<SendTrackHandle>` | drops send-track | ✓ verified |
| 5 | `reverb_send_db` | `f32` | none | ✓ (no-op) |
| 6 | `listener` | `Option<ListenerHandle>` | `mark_for_removal` (kira listener/handle.rs:53–55) | ✓ verified |
| 7 | `manager` | `Option<AudioManager<DefaultBackend>>` | tears down audio device | ✓ verified |

Owner audit:
- `SpatialTrackHandle`: owned by `ActiveSound._track` (underscore-prefix
  pin at lib.rs:156) — held for `Drop` side effect. Never moved out.
- `StaticSoundHandle`: owned by `ActiveSound.handle` — used for `state()`
  polling and `stop(Tween)` (Phase 4 cell-unload path).
- `ListenerHandle`: owned by `AudioWorld.listener`. Per-frame pose
  update via `set_position` / `set_orientation` (lib.rs:592–595).
- `SendTrackHandle`: owned by `AudioWorld.reverb_send`. Read-only after
  init; only `id()` is queried at per-track `with_send` time
  (lib.rs:633, 745).
- `StreamingSoundHandle`: owned by `AudioWorld.music`. Replaced
  wholesale by `play_music`; old handle is `.stop(fade)`-ed before
  reassignment (lib.rs:372–374).

The audit's lifecycle invariant — "track handles drop before listener
drops before manager drops" — is preserved. The `reverb_send_db: f32`
field sitting between `reverb_send` and `listener` is a visual-only
disruption (no Drop side effect on f32), but the invariant per-handle
ordering is intact.

## Findings

### AUD-D1-NEW-01: kira default `sub_track_capacity = 128` will be reached on populated Bethesda cells; per-track failure surfaces as a `warn!` and silent-skip of the sound

- **Severity**: HIGH
- **Dimension**: Manager Lifecycle & Spatial Sub-Track Dispatch
- **Location**: `crates/audio/src/lib.rs:230–245` (manager init);
  `crates/audio/src/lib.rs:640–661` and `:752–797` (per-dispatch failure paths)
- **Status**: NEW
- **Description**: `AudioManager::new(AudioManagerSettings::default())`
  inherits `Capacities::default()` from kira, which sets
  `sub_track_capacity = 128` (kira-0.10.8 manager/settings.rs:25). Each
  active spatial sound (entity-path one-shot, queue-path one-shot, OR
  looping emitter) holds one spatial sub-track for the duration of
  playback. A populated FO3 / FNV interior (Megaton has 929 REFRs;
  ~30–60 NPCs in a populated bunker; ambient looping per cell) plus a
  layer of footstep one-shots can reach 128 simultaneous sub-tracks
  during cell-load bursts. When kira returns `ResourceLimitReached`,
  the dispatch path logs `warn!` and `continue`s — the sound is
  silently dropped from that frame's playback.
- **Evidence**:
  ```
  # kira-0.10.8/src/manager/settings.rs:22–32
  impl Default for Capacities {
      fn default() -> Self {
          Self {
              sub_track_capacity: 128,
              send_track_capacity: 16,
              ...
          }
      }
  }
  # kira-0.10.8/src/manager.rs:131–148 (add_spatial_sub_track)
  ... .insert(track)?  // returns Err(ResourceLimitReached) at cap

  # crates/audio/src/lib.rs:752–760 (dispatch_new_oneshots)
  let mut track = match mgr.add_spatial_sub_track(...) {
      Ok(t) => t,
      Err(e) => {
          log::warn!("M44 Phase 3: add_spatial_sub_track failed for entity {:?}: {e}", p.entity);
          continue;
      }
  };
  ```
- **Impact**: Sounds drop silently during cell-load bursts. The user
  hears a partial soundscape with no obvious error in normal logs
  (only WARN). Hard to diagnose because the symptom is "some sounds
  missing," not a crash. Will be triggered by every populated interior
  cell once Phase 3.5b FOOT records + REGN ambient soundscapes land.
- **Related**: M44 Phase 3.5b (FOOT records), Phase 4 future (REGN).
- **Suggested Fix**: Override capacities at manager init —
  `AudioManagerSettings { capacities: Capacities { sub_track_capacity: 512, send_track_capacity: 32, ..Default::default() }, ..Default::default() }`. 512
  is a comfortable headroom for the worst Bethesda interior cell (FO4
  Diamond City Market sits around 400 active emitters in vanilla; 512
  also leaves room for the Phase 4 REGN ambient layer). Pin via a
  module-level `const SUB_TRACK_CAPACITY: usize = 512;` so the cap is
  one-line-greppable. Add a smoke test that asserts the configured cap
  exceeds 128 (regression gate against a "simplify back to default"
  refactor).

---

### AUD-D2-NEW-02: Multi-`AudioListener` entities silently pick the first iteration result with no warning

- **Severity**: MEDIUM
- **Dimension**: Listener Pose Sync
- **Location**: `crates/audio/src/lib.rs:556–564`
- **Status**: NEW
- **Description**: `sync_listener_pose` does
  `q.iter().next()` against the `AudioListener` query. When more than
  one entity carries the marker (mod scenario, debug fly-cam swap,
  third-person camera transition leaving the old camera marker in
  place), iteration order determines which entity drives the listener
  pose. Per CLAUDE.md "fly-cam swap" workflow this is silent and
  happens during gameplay, not just at startup.
- **Evidence**:
  ```rust
  // crates/audio/src/lib.rs:556–564
  let listener_entity = {
      let Some(q) = world.query::<AudioListener>() else { return; };
      let Some((entity, _)) = q.iter().next() else { return; };
      entity
  };
  ```
  No counter, no warning when iteration would produce more than one
  candidate. The crate docstring at lib.rs:445–446 acknowledges
  "At most one entity should carry this. If multiple do, the audio
  system uses whichever one comes first" — the policy is documented
  but not warned-on.
- **Impact**: After a fly-cam destroy-then-spawn cycle, the audio
  listener may stay attached to the OLD entity (despawned) — actually,
  the listener handle is reused (see AUD-D6-NEW-08), so this is fine.
  But during the brief window where two `AudioListener` entities
  coexist (third-person cutscene transition), spatial attenuation will
  use whichever wins iteration. No `warn!` to diagnose.
- **Related**: AUD-D6-NEW-08 (listener-handle reuse on entity churn).
- **Suggested Fix**: Add a one-shot warning at the start of
  `sync_listener_pose`: count the iterator, log
  `warn!("multiple AudioListener entities found ({n}); using first")`
  the first time the count exceeds 1, then debounce so it doesn't spam
  per-frame. A `static AtomicBool` or a flag on `AudioWorld` works.

---

### AUD-D3-NEW-03: `prune_stopped_sounds` re-issues `.stop()` every tick for despawned looping emitters until kira reports Stopped

- **Severity**: LOW
- **Dimension**: Spatial Sub-Track Dispatch / Looping & Streaming
- **Location**: `crates/audio/src/lib.rs:817–846`
- **Status**: NEW
- **Description**: When a looping emitter's source entity has lost its
  `AudioEmitter` component (cell unload, explicit removal),
  `prune_stopped_sounds` issues a tweened `stop()` on the kira handle
  each tick until the handle reports `Stopped`. The tween default is
  10 ms (kira tween.rs:104–112), so by the time the next prune tick
  runs (~16 ms later at 60 FPS), the state has flipped and the entry
  drops. But if the audio system tick-rate is faster than the kira
  tween rate, the sweep will call `stop()` multiple times, each
  resetting the tween. kira treats subsequent stops as new fade
  commands.
- **Evidence**:
  ```rust
  // crates/audio/src/lib.rs:824–846
  let mut to_stop_indices: Vec<usize> = Vec::new();
  for (idx, s) in audio_world.active_sounds.iter().enumerate() {
      if !s.looping { continue; }
      let Some(entity) = s.entity else { continue; };
      let still_has_emitter = emitter_q...
      if !still_has_emitter {
          to_stop_indices.push(idx);  // marked every tick until Stopped reports
      }
  }
  ...
  for idx in &to_stop_indices {
      audio_world.active_sounds[*idx].handle.stop(Tween::default());
  }
  ```
  No "stop already issued" flag on `ActiveSound`.
- **Impact**: Redundant kira commands during the ~10 ms fade window.
  Wasted CPU on re-walking the active list and re-marking; minor
  ringbuf traffic. Not a correctness issue (kira's repeated-stop is
  idempotent in effect). Becomes more visible if a future fade duration
  is longer (e.g., a 1-second graceful cell-unload fade).
- **Related**: AUD-D4-NEW-04 (fade duration is hard-coded to
  `Tween::default()` 10 ms).
- **Suggested Fix**: Add a `stop_issued: bool` field to `ActiveSound`
  and skip the re-stop when set. Drop the entry on the next tick that
  observes `Stopped`. Trivial change, prevents future-phase regressions
  if the fade duration is tuned up.

---

### AUD-D4-NEW-04: Looping emitter stop uses hard-coded `Tween::default()` (10 ms) — risk of audible click on long sustained ambients

- **Severity**: LOW
- **Dimension**: Looping & Streaming
- **Location**: `crates/audio/src/lib.rs:842–845`
- **Status**: NEW
- **Description**: When a looping emitter's source entity is despawned,
  `prune_stopped_sounds` issues `.stop(Tween::default())` — kira's
  default tween is 10 ms linear (kira tween.rs:104–112). For short
  sustained ambients (campfire crackle, generator hum) 10 ms is
  inaudible. For long-tailed ambients (cathedral choir, distant
  thunder loop) the abrupt fade can produce a faint click on cell exit.
- **Evidence**:
  ```rust
  // crates/audio/src/lib.rs:842–846
  for idx in &to_stop_indices {
      audio_world.active_sounds[*idx]
          .handle
          .stop(Tween::default());  // 10 ms linear, hard-coded
  }
  ```
  No way for callers (cell-unload path, scripted cutscene) to specify
  a longer fade.
- **Impact**: Possible audible click on long-tail ambient cell-unload.
  No way to tune per-emitter or globally. Doesn't break correctness
  but lowers production polish.
- **Related**: AUD-D3-NEW-03.
- **Suggested Fix**: Add an `unload_fade_ms: f32` field to
  `AudioEmitter` (default 10 ms) and read it during the prune sweep.
  Or: make the prune sweep accept a global "cell-unload fade"
  parameter from `AudioWorld` (configurable via a method).

---

### AUD-D5-NEW-05: Cell-load reverb-level detector is not wired — interiors and exteriors sound identically dry

- **Severity**: MEDIUM
- **Dimension**: Reverb Send & Routing
- **Location**: cross-cut: `byroredux/src/cell_loader.rs`,
  `byroredux/src/streaming.rs`, `crates/audio/src/lib.rs:427–429` (the
  setter exists but no caller toggles it)
- **Status**: NEW
- **Description**: The crate docstring (lib.rs:99–105) and Phase 6
  promise an "interior detector that runs after `cell_loader` finishes"
  to flip `set_reverb_send_db(-12.0)` for interiors and back to
  `f32::NEG_INFINITY` for exteriors. `set_reverb_send_db` exists, but
  no call site invokes it: `grep set_reverb_send_db
  /mnt/data/src/gamebyro-redux/{byroredux,crates}/**/*.rs` returns only
  the definition + tests.
- **Evidence**:
  ```
  $ grep -rn "set_reverb_send_db" /mnt/data/src/gamebyro-redux/byroredux/ /mnt/data/src/gamebyro-redux/crates/audio/
  crates/audio/src/lib.rs:427:    pub fn set_reverb_send_db(&mut self, db: f32) {
  crates/audio/src/lib.rs:1305:        world.set_reverb_send_db(-12.0);
  crates/audio/src/lib.rs:1307:        world.set_reverb_send_db(f32::NEG_INFINITY);
  # only the setter definition + 2 unit-test call sites
  ```
- **Impact**: Every cell sounds dry (no audible reverb) regardless of
  interior/exterior. The Phase 6 "Better-than-Bethesda axis" claim
  about reverb zones is unrealised in M44. Functional but lacks the
  promised interior bloom.
- **Related**: Phase 6 future (cell-acoustic-driven reverb zones).
  AUD-D5-NEW-06 (the bigger gap: per-cell acoustic data).
- **Suggested Fix**: Add a system in `Stage::Late` (or an exclusive
  cell-load callback in `byroredux/src/streaming.rs`) that observes
  the active cell's interior/exterior bit and calls
  `audio_world.set_reverb_send_db(-12.0)` or `NEG_INFINITY`. The
  CELL record's flag bit 0 distinguishes interior vs exterior; that's
  already plumbed through `cell_loader.rs`. Hook there.

---

### AUD-D5-NEW-06: `set_reverb_send_db` is documented as "applies to new sounds" but the docstring buries this; long-running ambients won't pick up reverb-level changes mid-playback

- **Severity**: LOW
- **Dimension**: Reverb Send & Routing
- **Location**: `crates/audio/src/lib.rs:420–429`
- **Status**: NEW
- **Description**: Per kira's API design, `with_send` is
  build-time-only on `SpatialTrackBuilder` (kira spatial_builder.rs:128–134)
  — there is no per-track `set_send_volume` after construction.
  Already-playing looping ambients (a cathedral chant, a generator hum)
  spawned BEFORE a `set_reverb_send_db(-12.0)` call will continue to
  play with their old (likely silent) send level. The docstring at
  lib.rs:421–426 captures the spirit ("Already-playing sounds keep
  their construction-time send level") but doesn't surface the
  consequence: a reverb-level change toward a populated cell will not
  retro-apply to the cell's existing ambient layer until those
  ambients are restarted.
- **Evidence**:
  ```
  # crates/audio/src/lib.rs:420–429
  /// **Phase 6**: set the per-new-spatial-track reverb send level
  /// in decibels. Already-playing sounds keep their construction-
  /// time send level; the change applies to *new* sounds dispatched
  /// after the call. ...

  # kira-0.10.8/src/track/sub/spatial_builder.rs:128–134
  /// Routes this track to the given send track with the given volume.
  pub fn with_send(mut self, track: ..., volume: ...) -> Self {
      self.sends.insert(track.into(), volume.into());  // build-time only
      ...
  }
  ```
  No `SpatialTrackHandle::set_send_volume(...)` exists in the kira
  0.10 API.
- **Impact**: The crate docstring claim "for short SFX (footsteps,
  gunshots) the level naturally refreshes as new sounds replace old
  ones" only works when sounds are short. Long ambients that span the
  cell-load → interior reverb-flip transition won't bloom. User-
  perceptible only after AUD-D5-NEW-05 lands and reverb starts firing.
- **Related**: AUD-D5-NEW-05.
- **Suggested Fix**: Once the cell-load detector lands (AUD-D5-NEW-05),
  on a reverb-level-flip event, restart all currently-active looping
  emitters (stop with a fade, then re-dispatch via the same
  `dispatch_new_oneshots` path so the new send level takes effect).
  Or: defer Phase 6 reverb-level dynamics until kira surfaces a
  per-track `set_send_volume` API. Document the limitation in the
  next-phase contract — see Future-Phase Readiness.

---

### AUD-D6-NEW-07: `audio_system` runs in `Stage::Late` (correct), but `footstep_system` runs in `Stage::Update` and writes `play_oneshot` queue entries from a stale `GlobalTransform`

- **Severity**: MEDIUM
- **Dimension**: ECS Lifecycle
- **Location**: `byroredux/src/main.rs:315`, `byroredux/src/main.rs:340`,
  `byroredux/src/systems.rs:773–843`
- **Status**: NEW
- **Description**: `footstep_system` is registered in `Stage::Update`
  (main.rs:315), running BEFORE `transform_propagation_system` in
  `Stage::PostUpdate` (main.rs:321). The footstep system reads
  `GlobalTransform.translation` to generate the world-space dispatch
  position (systems.rs:799–803). The acknowledgement comment
  (main.rs:305–314) admits the GlobalTransform is "one frame stale
  relative to the camera's Transform" but waves it off as
  "~3 cm of motion." For a fly-cam at 200+ engine units/s (sprint
  + boost), that's 200/60 = ~3.3 units/frame — not 3 cm but 3 game
  units. At Bethesda interior scales (cells are ~50–200 units across)
  that's noticeable spatial-pan offset. The audit dimension explicitly
  flags this: "running before transform propagation reads stale
  GlobalTransform for the listener and emitters."
- **Evidence**:
  ```
  # byroredux/src/main.rs:305–315
  // M44 Phase 3.5: footstep dispatch. Runs in Stage::Update so
  // it sees the post-fly-camera Transform (which the camera
  // system writes in Stage::Early) but BEFORE
  // transform_propagation in Stage::PostUpdate — so the
  // GlobalTransform we read is one frame stale relative to the
  // camera's Transform. That's acceptable for footstep position
  // accuracy at human movement speeds (~1 frame at 60 FPS = 17
  // ms = ~3 cm of motion).
  scheduler.add_to(Stage::Update, footstep_system);
  ...
  scheduler.add_to(Stage::PostUpdate, make_transform_propagation_system());
  ```
  `audio_system` itself runs at `Stage::Late` (main.rs:340) — that part
  is correct (verified against the audit checklist at audit-audio.md:112).
  The issue is the upstream queue producer's stage.
- **Impact**: Footsteps at fast-travel speeds (and during teleports,
  warp-debug commands) audibly trail the listener. For human-walk
  speeds (~5 units/s), the comment's 3-cm claim is roughly right; for
  sprint (~30+ units/s) it's 0.5 units; for fly-cam boost it's 3+
  units. Bethesda's "feels like a game" axis cares about footstep-
  to-position correlation.
- **Related**: M44 Phase 3.5b (FOOT records).
- **Suggested Fix**: Move `footstep_system` to `Stage::PostUpdate`
  AFTER `transform_propagation_system`, OR have it read `Transform`
  directly when the entity has no parent (then the local transform IS
  the world transform). The comment's reasoning ("3 cm of motion")
  underestimates the worst case by ~100×. Pin a regression test that
  spawns a fast-moving entity, ticks one frame, and asserts the
  emitted footstep position matches the post-propagation
  GlobalTransform.

---

### AUD-D6-NEW-08: Listener handle is never reset on AudioListener entity despawn — verified safe (handle is reused via lazy create + per-frame pose update), document the contract

- **Severity**: LOW
- **Dimension**: ECS Lifecycle
- **Location**: `crates/audio/src/lib.rs:555–595`
- **Status**: NEW
- **Description**: When the entity carrying `AudioListener` is
  despawned, `sync_listener_pose` early-returns at line 557–562 — the
  `audio_world.listener` handle is NOT reset to `None`. On the next
  frame, if a NEW entity gets `AudioListener`, line 574
  (`audio_world.listener.is_none()`) is FALSE, so we fall through to
  the `else if` branch (line 592) and update the EXISTING handle's
  pose. **This is correct (handle reuse, no kira leak), but the
  contract is non-obvious from reading the code.**
- **Evidence**:
  ```rust
  // crates/audio/src/lib.rs:555–595
  fn sync_listener_pose(world: &World, audio_world: &mut AudioWorld) {
      let listener_entity = { ... };  // early-return on no AudioListener
      let pose = { ... };              // early-return on no GlobalTransform
      if audio_world.listener.is_none() { ... add_listener ... }
      else if let Some(handle) = audio_world.listener.as_mut() {
          handle.set_position(pose.0, Tween::default());
          handle.set_orientation(pose.1, Tween::default());
      }
  }
  ```
  kira's `listener_capacity = 8` (kira-0.10.8/src/manager/settings.rs:29)
  — even if despawn-respawn DID create a new handle each cycle, the
  cap would catch a runaway after 8 cycles. But the actual code reuses
  the existing handle, so the cap is irrelevant.
- **Impact**: None today. Future-phase risk: if someone refactors
  `sync_listener_pose` to "clear `audio_world.listener` when no entity
  carries the marker," the next respawn would call `add_listener`
  again — fine the first time, but a bursty churn (debug fly-cam
  destroy-create loop) could exhaust kira's `listener_capacity = 8`.
- **Related**: AUD-D2-NEW-02 (multi-listener entity case).
- **Suggested Fix**: Add a doc comment at lib.rs:553 stating "Listener
  handle is created lazily on first observation and REUSED across
  AudioListener entity churn — never cleared. This is intentional:
  prevents listener_capacity exhaustion on rapid entity churn." A
  one-line guard against future "simplify by clearing on missing
  entity" refactors.

---

### AUD-D6-NEW-09: `SoundCache` has no eviction; long sessions with mod-loaded SFX can grow unboundedly

- **Severity**: LOW
- **Dimension**: ECS Lifecycle (Cell Streaming)
- **Location**: `crates/audio/src/lib.rs:951–1041`
- **Status**: NEW
- **Description**: `SoundCache.map: HashMap<String, Arc<StaticSoundData>>`
  has no eviction policy. The crate docstring at lib.rs:962–966
  acknowledges this ("Eviction strategy: **none today**") and argues
  the vanilla SFX set is small enough to fit. But mod-loaded SFX
  (LARGE mods like Project Nevada, Tale of Two Wastelands) each add
  hundreds of unique sounds, and the cache key is the full BSA path
  (no path-aliasing collapse for "same sound, different filename").
  A 24-hour session with frequent mod swaps could grow the cache
  unboundedly.
- **Evidence**:
  ```rust
  // crates/audio/src/lib.rs:951–1041
  pub struct SoundCache {
      map: HashMap<String, Arc<StaticSoundData>>,
  }
  // No clear(), no LRU, no max_entries.
  ```
- **Impact**: Memory growth on long sessions with heavy mod use. The
  vanilla case is bounded (~6,000 unique SFX × ~30 KB decoded
  average = ~180 MB; well within budget). Mod-stack with FCO + TTW +
  Mojave Express + Project Nevada can push past 1 GB.
- **Related**: AnimationClipRegistry (#790, similar process-lifetime
  cache pattern).
- **Suggested Fix**: Either (a) document the upper bound and accept
  it, (b) add an LRU eviction with a `max_entries` cap once a future
  scenario surfaces, or (c) add a `clear()` method that the cell-
  unload path can call when a region exits scope. The crate docstring
  already acknowledges this — no urgent action, but pin a
  `cache_bytes_estimate()` helper that telemetry can poll so a future
  regression surfaces in `stats` output.

---

### AUD-D3-NEW-10: `drain_pending_oneshots` `mem::take` happens BEFORE the manager-active check, dropping queued entries unreachably

- **Severity**: LOW
- **Dimension**: Spatial Sub-Track Dispatch
- **Location**: `crates/audio/src/lib.rs:603–624`
- **Status**: NEW
- **Description**: `drain_pending_oneshots` first checks listener_id
  (line 604), then takes the pending vec (line 610), then checks the
  manager (line 619). If the manager is `None` at line 619, the
  pending entries are dropped (already-taken into a local) without
  dispatch. The comment at line 620–623 says "Inactive — queue
  cleared" — but in practice this is unreachable: `audio_system`
  early-returns at line 542 (`if !audio_world.is_active() { return; }`)
  when the manager is `None`, so the manager check at line 619 can
  never trigger inside `drain_pending_oneshots`. Once `audio_system`
  starts, the manager state cannot change.
- **Evidence**:
  ```rust
  // crates/audio/src/lib.rs:538–550 (audio_system)
  if !audio_world.is_active() { return; }   // gates everything
  ...
  drain_pending_oneshots(&mut audio_world);

  // crates/audio/src/lib.rs:603–624
  fn drain_pending_oneshots(audio_world: &mut AudioWorld) {
      let Some(listener_id) = ... else { return; };
      if audio_world.pending_oneshots.is_empty() { return; }
      let pending = std::mem::take(&mut audio_world.pending_oneshots);  // takes
      ...
      let Some(mgr) = audio_world.manager.as_mut() else {
          return;  // unreachable in practice — pending is dropped here
      };
  }
  ```
- **Impact**: Dead defensive branch. Wastes a cycle of `mem::take`
  before realising the take was wasted. Trivial perf cost. Slight
  mental-model mismatch reading the code: "is this protected for
  re-init?" — answer is "no, the check above already guarantees
  manager is Some."
- **Related**: None.
- **Suggested Fix**: Move the manager check UP, before `mem::take`,
  OR remove the redundant manager check entirely (it's already
  guaranteed Some by `audio_system`'s early-return). Cleanup pass; not
  urgent.

---

### AUD-D3-NEW-11: `play_oneshot` queue cap at 256 uses `Vec::remove(0)` — O(n) shift on every push when full

- **Severity**: LOW
- **Dimension**: Spatial Sub-Track Dispatch
- **Location**: `crates/audio/src/lib.rs:320–342`
- **Status**: NEW
- **Description**: When the pending queue hits `MAX_PENDING = 256`,
  `play_oneshot` calls `self.pending_oneshots.remove(0)` — O(n)
  shift of 256 elements per push. On a no-device-host (the only
  scenario where the queue can saturate, since `audio_system` drains
  every frame on an active host), 1000+ enqueues per second is
  unrealistic, but each saturated push is O(256). Could be
  O(1) with a `VecDeque` + ring-buffer.
- **Evidence**:
  ```rust
  // crates/audio/src/lib.rs:328–335
  if self.pending_oneshots.len() >= MAX_PENDING {
      log::warn!(...);
      self.pending_oneshots.remove(0);  // O(n) shift
  }
  self.pending_oneshots.push(...);
  ```
- **Impact**: Negligible. Saturation only happens on no-device-host
  with a runaway upstream producer. The warn-log itself is more
  expensive than the shift. Pure code-quality finding.
- **Related**: None.
- **Suggested Fix**: Switch to `VecDeque<PendingOneShot>` with
  `pop_front` on saturation. Drain becomes
  `pending_oneshots.drain(..)`. One-line type change.

## Future-Phase Readiness

This audit pinned the following invariants that the next M44 phases
will rely on:

### Phase 3.5b: FOOT records → per-material footstep sound

- **Lifecycle contract**: `FOOT` records map material types to sound
  asset paths. The footstep dispatch already routes through
  `AudioWorld::play_oneshot` (queue API) — that path is stable. The
  per-material lookup just needs to choose the right `Arc<StaticSoundData>`
  before enqueuing. No structural change to `AudioWorld` required.
- **Capacity contract**: AUD-D1-NEW-01 (`sub_track_capacity = 128`)
  becomes critical here. Each footstep is one spatial sub-track; in
  combat (multiple NPCs running) you can hit 30+ overlapping footstep
  sounds. Fix the cap BEFORE landing FOOT records.
- **Stale-pose risk**: AUD-D6-NEW-07 (`footstep_system` in
  `Stage::Update`) becomes more visible with FOOT records, since
  per-material footstep dispatch will fire MORE one-shots per frame.
  Fix the stage placement BEFORE landing FOOT records.

### Phase 4 future: REGN ambient soundscapes

- **Lifecycle contract**: REGN spawns a per-region `AudioEmitter` with
  `looping = true` on cell-enter, despawns on cell-leave. The Phase 4
  prune sweep handles "AudioEmitter component removed → tweened stop"
  correctly (verified at lib.rs:817–846). The cell-unload path must
  remove the `AudioEmitter` component (or despawn the entity); it must
  NOT silently drop kira handles, or playback will continue audibly
  until kira's resource limit purges it.
- **Click-on-unload risk**: AUD-D4-NEW-04 (10 ms hard-coded fade) is
  more audible on long-tail ambients (a thunder roll, a chant). Tune
  the fade duration before landing REGN.
- **Capacity contract**: REGN can produce 4–8 ambient layers per
  region (FO3 wasteland regions stack 5–7 typical). Plus footsteps,
  plus per-NPC voice — capacity ceiling is reachable. Fix
  AUD-D1-NEW-01 first.

### Phase 5 future: MUSC + hardcoded music routing

- **Single-slot contract**: `AudioWorld.music: Option<StreamingSoundHandle>`
  is fixed at one slot. MUSC-driven crossfade must call `play_music`
  (which handles the fade-out-fade-in via `existing.stop(fade)` + new
  handle assignment). Multi-slot music is OUT OF SCOPE — adding it
  would break the "music is never spatial" invariant pinned at lib.rs:344–390.
- **Continuity contract**: Music does NOT despawn on cell transition
  (verified — `music` is a `AudioWorld` field, lives across cell
  loads). MUSC routing should gate `play_music` on
  "is the new cell's music different from the current track" to avoid
  re-decoding the same StreamingSoundData. There's no test pinning
  this today.

### Phase 6 future: cell-acoustic-driven reverb zones

- **Wired-but-unused contract**: AUD-D5-NEW-05 (cell-load detector
  not wired) is the immediate gap. Once landed:
- **Build-time-only `with_send` constraint**: AUD-D5-NEW-06 — kira 0.10
  has no `SpatialTrackHandle::set_send_volume`. Reverb-level changes
  apply to NEW sounds. Long-running ambients spawned before a flip
  will not bloom. Either restart all looping emitters on flip, or
  defer Phase 6 dynamics until kira surfaces a runtime API. Pin this
  contract in the cell-acoustic-design doc before landing.

### Raycast occlusion attenuation (no phase number assigned yet)

- The current `Attenuation` struct (lib.rs:459–476) is min/max
  distance with linear falloff. Raycast occlusion would need an
  additional per-emitter "blocked by geometry" multiplier. Per-track
  volume CAN be set after construction (kira's
  `StaticSoundHandle::set_volume`), so the architectural shape
  supports adding it. The audit found no blockers.

## Methodology Note

Per `feedback_audit_findings.md` ("16% of audit findings are stale-
premise"), every finding above was checked against current code:

- AUD-D1-NEW-01: re-verified `Capacities::default()` in kira-0.10.8
  source.
- AUD-D5-NEW-05: re-verified `set_reverb_send_db` has no callers via
  full-tree grep.
- AUD-D6-NEW-07: re-verified scheduler stage ordering in
  `byroredux/src/main.rs:315, 321, 340`.
- AUD-D6-NEW-08: traced through `sync_listener_pose` to confirm the
  handle-reuse path (despawn-respawn does NOT leak listener handles).

Two candidate findings WERE disproved during the audit and are
explicitly NOT included:

- **(disproved) "kira left/right channel inversion"**: kira docs at
  `listener/handle.rs:35–36` confirm "unrotated listener faces -Z,
  +X right, +Y up" — matches ByroRedux camera convention exactly.
  Cross-verified `listener_ear_positions` math at
  `kira-0.10.8/src/track/sub.rs:348–355`. Confirmed clean.
- **(disproved) "field-drop order has reverb_send_db disrupting
  invariant"**: `f32` has no `Drop`, so the field's position in the
  declaration order is invisible to drop sequencing. Per-handle
  ordering is preserved.

## Dedup Status

`/tmp/audit/issues.json` (200 most recent issues) was scanned for
audio-related keywords (`audio`, `kira`, `sound`, `reverb`, `listener`,
`music`, `oneshot`). Two matches found, both unrelated:

- #810 [CLOSED] "FNV-D2-NEW-03: 31-record long tail dispatch coverage
  gap (audio / hardcore / Caravan / load screens / supporting metadata)" —
  ESM record dispatch, not the audio crate.
- #693 [CLOSED] "O3-N-05: CELL parser drops XCMT (pre-Skyrim music)
  and XCCM (Skyrim climate override per cell)" — CELL parser, not
  the audio crate.

No prior audio-subsystem audit exists in `docs/audits/`. All 11
findings are NEW.

## Suggested Next Action

```
/audit-publish docs/audits/AUDIT_AUDIO_2026-05-05.md
```

Open issues for the HIGH (AUD-D1-NEW-01) and the four MEDIUM findings
first; the six LOWs can batch later if there's no immediate fixer
bandwidth.
