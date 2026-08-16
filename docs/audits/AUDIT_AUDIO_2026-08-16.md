# Audio Subsystem Audit (M44) — 2026-08-16

- **Command**: `/audit-audio` → all 7 dimensions, `--depth deep` (one leg of a
  `comprehensive` audit-suite sweep)
- **Branch**: main · **HEAD**: `85b77371`
- **kira**: pinned `0.10` (workspace `Cargo.toml`, unchanged) · resolved
  `kira-0.10.8`
- **Method**: single-agent, no sub-agents. Every dimension re-derived from live
  source: `crates/audio/src/lib.rs` (1326 lines, read in full),
  `crates/audio/src/tests.rs`, `byroredux/src/systems/audio.rs` (562 lines, read
  in full), plus `byroredux/src/boot.rs` (scheduler + resource wiring),
  `byroredux/src/scene.rs` (listener/footstep opt-in),
  `byroredux/src/asset_provider/texture.rs`, `crates/core/src/ecs/scheduler.rs`
  (stage-ordering guarantee), `crates/core/src/ecs/world.rs` (despawn →
  component erasure), and the vendored `kira-0.10.8` sources for the
  `SpatialTrackHandle` / `StreamingSoundHandle` contracts. Dedup baseline:
  `/tmp/audit/issues.json` (269 OPEN issues) + the full prior
  `docs/audits/AUDIT_AUDIO_*.md` chain back to `_05-05`. Per-dimension scratch
  notes at `/tmp/audit/audio/dim_1..7.md`.

---

## Delta Analysis (since `AUDIT_AUDIO_2026-08-07.md` HEAD `79bfc76e`)

| File | Change | Audio-relevant? |
|---|---|---|
| `crates/audio/src/lib.rs` | **two fixes landed** — `c0f3cda3` (Fix #2405: extract shared reverb-send gate helper) and `8a404914` (consume spent one-shot markers, #2394) | **Yes** — both close the prior cycle's findings |
| `crates/audio/src/tests.rs` | two new guards added by the same commits | **Yes** |
| `byroredux/src/systems/audio.rs` | none | — |
| `byroredux/src/components.rs` | none in the `Footstep*` structs | — |
| `byroredux/src/boot.rs` | `8a404914` touched Late-stage access declarations; the audio block's registration and stages are unchanged | Verified, no behavioural drift |
| `byroredux/src/scene.rs` | none touching the `AudioListener` / `FootstepEmitter` opt-in | — |
| `byroredux/src/asset_provider/texture.rs` | none | — |

**Net: the interval closed both of the prior cycle's open items.** After ten
consecutive audit cycles this is the first one where the audio crate's own
findings backlog reached zero on entry.

---

## Executive Summary

**7 dimensions run, 3 NEW findings (0 CRITICAL / 0 HIGH / 1 MEDIUM / 2 LOW).**
Per-dimension counts, including clean ones:

| # | Dimension | Findings |
|---|---|---|
| 1 | Spatial Sub-Track Lifecycle & Leaks | **1** (MEDIUM) |
| 2 | Listener Pose & Attenuation | 0 |
| 3 | SoundCache Growth & Eviction | 0 |
| 4 | Streaming Music Lifecycle | 0 |
| 5 | Reverb Send & Routing | 0 |
| 6 | Manager Lifecycle, ECS & Cell Streaming | **1** (LOW, doc rot) |
| 7 | Gameplay Audio Wiring | **1** (LOW, doc rot) |

- **Headless-mode boot**: **PASS**. `AudioManager::new` failure leaves
  `manager = None` (`lib.rs:332-339`); zero `.unwrap()` on the manager `Option`
  anywhere in the crate. Guard
  `audio_world_constructs_without_panic_on_any_environment` green.
- **Guards re-run live on HEAD `85b77371`**:

| Suite | Result |
|---|---|
| `cargo test -p byroredux-audio` | **21 passed, 0 failed, 6 ignored** (ignored = real-audio-device + vanilla-FNV-data gated, not broken) |

- **Prior-cycle findings — both CLOSED and now regression-guarded**:
  - `AUD-2026-08-07-D5-01` / **#2405** (reverb-send gate duplicated `-60.0` as a
    literal across two dispatch sites) → fixed by `c0f3cda3`. The copy-pasted
    block is gone; `apply_reverb_send` (`lib.rs:158-169`) + `reverb_send_gate_open`
    (`lib.rs:175-177`) are shared by both sites and key off `SILENCE_DB`. New
    guard `reverb_send_gate_matches_silence_db_boundary` (`tests.rs:1186`) pins
    the boundary, the `NEG_INFINITY` sentinel and the NaN case.
  - **#2394** / `ECS-D7-2026-08-07-01` (`OneShotSound` marker leaked on both
    `dispatch_new_oneshots` failure arms) → fixed by `8a404914`. A `consumed`
    vec (`lib.rs:941`) is pushed on both `Err` arms (`964`, `989`) and on
    success (`1000`). New guard
    `oneshot_marker_is_consumed_on_both_dispatch_failure_arms` (`tests.rs:1220`)
    is a source-level pin (both arms are kira failures no headless test can
    force) and additionally asserts the push-site count is exactly 3, so a new
    loop exit that skips the marker fails the build.
- **Shipped surface (Phases 1–6), re-confirmed line-by-line**: `AudioWorld`
  graceful degradation (`SUB_TRACK_CAPACITY = 512` / `SEND_TRACK_CAPACITY = 32`,
  both above kira defaults); `AudioListener` / `AudioEmitter` / `OneShotSound`
  (all `SparseSetStorage`); `audio_system` (`sync_listener_pose` →
  `drain_pending_oneshots` → `dispatch_new_oneshots` → `prune_stopped_sounds`);
  `load_sound_from_bytes` + `SoundCache` (case-insensitive keys, manual
  `clear()`-only eviction); both dispatch paths (queue `VecDeque` cap 256 with
  `pop_front` drop-oldest; entity path with `loop_region(..)`); tweened-`stop()`
  despawn truncation for looping AND non-looping with the `stop_issued`
  debounce; single-slot streaming music on the main (non-spatial) track; global
  reverb send (`feedback 0.85` / `damping 0.6` / `stereo_width 1.0` / `Mix::WET`,
  `f32::NEG_INFINITY` dry default). Engine consumers: `footstep_system` (still
  the only `play_oneshot` caller) and `reverb_zone_system` (still the only
  `set_reverb_send_db` caller).
- **Pending (future-phase, not flagged as missing)**: Phase 3.5b FOOT →
  per-material sound, REGN ambient soundscapes, MUSC routing, per-cell-acoustics
  reverb (detector is binary interior/exterior only), raycast occlusion
  attenuation.
- **MUSC parse→play gap (confirmed still absent, by design)**: cell-music
  FormIDs are parsed (`default_music`/ZNAM, `music_type_form`/XCMO in
  `crates/plugin/src/esm/cell/`) but `grep play_music` across `byroredux/` and
  every non-audio crate returns zero hits. Single-slot / main-track / streaming-
  type invariants stay pinned for the eventual caller.

---

## Lifecycle Invariant Matrix

Owned by Dimension 6, with the Dim 1/4/5 pointers collapsed here per the skill's
dedup instruction. All re-derived independently this cycle against live line
numbers.

| Invariant | State | Anchor |
|---|---|---|
| `AudioWorld` field-drop order (`active_sounds` → `pending_oneshots` → `music` → `reverb_send` → `reverb_send_db` → `listener` → `manager` → `multi_listener_warned`) | HOLDS | `lib.rs:261-301` |
| Manager capacities exceed kira defaults (512 / 32) | HOLDS | `lib.rs:197-198`, applied `316-320` |
| Lazy listener creation, no frame-1 cold-start panic (4 sequential early-return gates) | HOLDS | `lib.rs:728`, `738`, `758`, `761` |
| Sticky listener, never cleared on entity churn (#849) | HOLDS | written only at `lib.rs:778`; no clear site exists |
| Multi-listener diagnostic debounced (#843) | HOLDS | `lib.rs:741-754`, `multi_listener_warned` never reset |
| Orientation contract — quat handed to kira is already renderer-space | HOLDS | `lib.rs:764`; Z-up→Y-up resolved at NIF import (`crates/nif/src/import/coord.rs`) before any `Transform` exists |
| Attenuation `RangeInclusive`, `min<=max` normalized (#1612) | HOLDS | `Attenuation::distance_range()` `lib.rs:589-593`; guard `reversed_attenuation_normalizes_instead_of_panicking` |
| `add_listener` failure is transient-retry, not permanent lockout | HOLDS | `lib.rs:780-783` |
| `ActiveSound._track` underscore name intact, held for Drop side effect | HOLDS | `lib.rs:217`, `852`, `996` |
| **Entity-path spatial sub-track position updated per frame** | **DRIFTED from its own docstring** — set once at dispatch, never repositioned | `lib.rs:609-612` (claim) vs `956` (only write) — **AUD-2026-08-16-D1-01** |
| Both dispatch paths gate on `listener_id` before any work | HOLDS | `lib.rs:796` / `875` |
| Both dispatch paths apply the reverb gate through one shared helper | HOLDS (was the prior cycle's LOW, now structural) | `lib.rs:828-832` / `951-955` → `apply_reverb_send` `158-169` |
| `looping` / `loop_region(..)` applied ONLY in the entity path | HOLDS | `lib.rs:974-980`; `PendingOneShot` (`240-245`) has no `looping` field — structurally incapable |
| Volume→dB conversion centralized (`linear_volume_to_db`) | HOLDS | `lib.rs:144-150`, called from `481`, `840`, `972` |
| `Arc<StaticSoundData>` clone is Arc-share, not PCM deep-copy | HOLDS | `lib.rs:915`, `841`/`973`; kira `frames: Arc<[Frame]>` |
| Drain cap (>32/tick warns) · producer cap (256, `pop_front`) · drain-gate-before-`mem::take` (#851/#852/#853) | HOLDS | `lib.rs:816-823`, `431-441`, `796`→`812`→`815` |
| `OneShotSound` marker consumed on success **and** both failure arms (#2394) | **HOLDS — fixed this interval** | `lib.rs:941`, `964`, `989`, `1000`, removal `1007-1013` |
| Despawn truncation — tweened `stop()` on emitter removal, looping + non-looping (#845/#858/SAFE-23), `stop_issued` debounce (#844) | HOLDS | `prune_stopped_sounds` `lib.rs:1020-1097`; `World::despawn`/`despawn_batch` (`crates/core/src/ecs/world.rs:121`/`145`) erase every storage row, so cell-unload does surface as "lost its `AudioEmitter`" |
| Queue-driven sounds (`entity == None`) exempt from despawn truncation | HOLDS | `lib.rs:1044` |
| `SoundCache` lowercase-once, `clear()` doesn't invalidate live `Arc`s, dormant in engine (#859/#850) | HOLDS | `lib.rs:1241`/`1249`/`1265`, `1300-1302`; only `byroredux/` reference is the diagnostic read at `byroredux/src/ownership_sample.rs:63`, and the resource is never inserted |
| `get_or_load` invokes loader only on a genuine miss | HOLDS | `lib.rs:1265-1268`; guard `sound_cache_get_or_load_invokes_loader_only_on_miss` |
| Single-slot music · main track (not spatial) · streaming (not buffered) types · fade-then-drop `stop_music` | HOLDS | `lib.rs:276`, `483`, `1171`/`1181`, `496-510` (`fade_out_secs.max(0.0)`; no `impl Drop` on `StreamingSoundHandle`, so the fade completes) |
| `is_music_active` reports inactive right after `stop_music` (unblocks a legit re-`play_music`) | HOLDS | `lib.rs:515-520` |
| Reverb send-track creation `None`-safe · default `NEG_INFINITY` · `> SILENCE_DB` gate · construction-time-only (#847, documented limitation) | HOLDS | `lib.rs:346-364`, `370`, `175-177`, `530-543` |
| Scheduler stages (`PostUpdate` footstep → `Late` reverb-then-audio) | HOLDS, structurally guaranteed | `boot.rs:1017` (footstep, `PostUpdate` exclusive), `boot.rs:1279-1285` (reverb, Late parallel batch), `boot.rs:1303` (audio, Late exclusive); `Scheduler::run` (`crates/core/src/ecs/scheduler.rs:474-511`) completes the whole parallel batch of a stage before any exclusive in it |
| `AudioWorld::new()` called exactly once, at boot; never on cell transition | HOLDS | single call site `boot.rs:482`; zero `AudioWorld` / `audio_system` / `SoundCache` refs in `streaming.rs`, `cell_loader/load.rs`, `cell_loader/unload.rs` |
| Footstep stride (XZ-only, reset-not-remainder), first-tick seed (#848), `FootstepScratch` reuse on both paths (#932), lock-drop ordering, `{0.5,12.0}` attenuation | HOLDS | `byroredux/src/systems/audio.rs:146-154`, `140-144`, `127`/`166-167`/`174-176`/`197-199`, `167`→`169`, `187-188` |
| Camera opt-in component-driven (no hardcoded entity) | HOLDS | `byroredux/src/scene.rs:840`/`844`; `footstep_system` walks `query_mut::<FootstepEmitter>()` generically; storage pre-registered `boot.rs:545` |
| `reverb_zone_system` constants, bit-equality gate, dual no-op safety, runs before `audio_system` | HOLDS | `byroredux/src/systems/audio.rs:43`/`46`/`67`, `49`, `60` |
| **Audio scheduler-wiring comments cite the correct files / current behaviour** | **DRIFTED** | `boot.rs:1286-1290`, `byroredux/src/systems/audio.rs:37-39` — **AUD-2026-08-16-D6-01** |
| **`ROADMAP.md` M44 row test counts match the live suite** | **DRIFTED** | `ROADMAP.md:672` vs live 21/6 — **AUD-2026-08-16-D7-01** |

---

## Findings

### AUD-2026-08-16-D1-01: Entity-path spatial sub-track position is frozen at dispatch, while `AudioEmitter`'s docstring promises a per-frame update

- **Severity**: MEDIUM
- **Dimension**: Spatial Sub-Track Lifecycle & Leaks
- **Location**: `crates/audio/src/lib.rs:609-612` (the false claim),
  `crates/audio/src/lib.rs:956` (the only position write),
  `crates/audio/src/lib.rs:217` (`ActiveSound._track`)
- **Status**: NEW
- **Description**: `AudioEmitter`'s struct docstring states, verbatim:

  ```rust
  /// Static-payload audio emitter. Holds the decoded sound data and
  /// attenuation. The audio system reads the entity's `GlobalTransform`
  /// every frame to update the spatial position.
  ```

  The audio system does no such thing. `dispatch_new_oneshots` reads
  `GlobalTransform.translation` **once**, at dispatch (`lib.rs:918`), passes it
  to `mgr.add_spatial_sub_track(listener_id, p.position, track_builder)`
  (`lib.rs:956`), and stores the resulting handle in
  `ActiveSound._track`. That field is deliberately underscore-prefixed as a
  Drop-only holder and is never read or mutated again — `prune_stopped_sounds`
  touches `handle`, `entity`, `unload_fade_ms` and `stop_issued`, never
  `_track`. A workspace-wide grep for `set_position` finds exactly one audio
  call site, and it is the **listener** (`lib.rs:785`), not any emitter track.

  This is not a kira limitation: `SpatialTrackHandle::set_position` exists in
  the pinned version
  (`~/.cargo/registry/src/*/kira-0.10.8/src/track/sub/spatial_handle.rs:96`),
  taking the same `Tween` the listener path already uses. The per-frame update
  is simply unimplemented.

  The gap is load-bearing for the **Phase 4 looping-emitter feature that is
  already marked shipped** (`docs/feature-matrix.md:138`, "Looping ambient
  (tweened stop on despawn) ✓"). A looping `AudioEmitter` on a moving entity —
  an NPC-carried torch, a creature ambient, moving machinery, a scripted
  moving-sound prop — plays anchored at its spawn point for its entire life,
  drifting further from the entity every frame with no upper bound and no
  diagnostic.
- **Evidence**:
  ```rust
  // lib.rs:956 — the sole position input, dispatch-time only
  let mut track = match mgr.add_spatial_sub_track(listener_id, p.position, track_builder) {
  // lib.rs:996 — stored as a Drop-only holder, never touched again
      _track: track,
  ```
  `grep -n "set_position" crates/audio/src/lib.rs` → `680` (a doc line), `785`
  (`ListenerHandle`). No emitter reposition anywhere.
- **Impact**: **Latent today, wrong by construction tomorrow.** There are
  currently zero `AudioEmitter` insert sites in `byroredux/` — `scene.rs:840`
  inserts only `AudioListener`, and `spawn_oneshot_at` has no engine callers —
  so nothing exercises the entity path in a shipping build. The live audio path
  is the queue (`play_oneshot`), which takes an explicit `position: Vec3` per
  call and is therefore correct-by-contract for footsteps. The blast radius is
  the next producer: FOOT/3.5b, REGN ambient layers, and any scripted emitter
  will read this docstring, attach an emitter to a moving entity, and get a
  sound pinned in space. The docstring is the specific hazard — it converts a
  known gap into a false guarantee, which is exactly the failure mode the
  project's "verify the premise" hygiene rule exists to prevent.
- **Related**: `docs/feature-matrix.md:138` (Phase 4 marked ✓);
  `ROADMAP.md:672` M44 Phase 4 text (describes `loop_region` + tweened stop, and
  correctly does *not* claim position tracking — the crate docstring is the only
  place the false claim lives). Adjacent to #847's documented "construction-time
  only" reverb-send limitation, which is the same shape of kira build-time-vs-
  live-handle distinction but *is* correctly documented as a limitation.
- **Suggested Fix**: Either (a) rename `_track` → `track` and add a
  reposition pass to `audio_system` — for each `ActiveSound` with
  `entity: Some(e)`, look up the current `GlobalTransform` and call
  `track.set_position(pos, Tween::default())`, mirroring the listener path
  exactly; or (b) if per-frame repositioning is deliberately deferred, correct
  the `AudioEmitter` docstring to say the position is sampled **once at
  dispatch** and record the moving-emitter limitation next to #847's, so the
  FOOT/REGN producer plans around it. Option (a) is a ~10-line addition and
  removes the trap; either way the docstring must stop asserting behaviour the
  code doesn't have.

### AUD-2026-08-16-D6-01: Stale audio scheduler-wiring comments — `audio_system` described as a "Phase 1 stub", `reverb_zone_system` registration attributed to `main.rs`

- **Severity**: LOW
- **Dimension**: Manager Lifecycle & ECS/Cell Streaming (+ Gameplay Audio Wiring)
- **Location**: `byroredux/src/boot.rs:1286-1290`;
  `byroredux/src/systems/audio.rs:37-39`
- **Status**: NEW
- **Description**: Two separate comments in the audio scheduler wiring describe
  an engine that no longer exists.

  1. `boot.rs:1286-1290`, immediately above
     `scheduler.add_exclusive(Stage::Late, byroredux_audio::audio_system)`:
     *"The Phase 1 body is a stub (see `byroredux_audio::audio_system`); future
     phases (one-shot dispatch, listener pose sync, looping emitter lifecycle)
     flesh it out without touching the schedule wiring."* All three named phases
     shipped long ago — `audio_system` (`lib.rs:694-706`) dispatches four
     helpers, and the crate docstring itself marks Phases 1–6 complete. A reader
     auditing stage assignment is told the system does nothing.
  2. `byroredux/src/systems/audio.rs:37-39`, in `reverb_zone_system`'s docstring:
     *"Runs in `Stage::Late` alongside `audio_system` (registered first in
     main.rs so the level is in place before any new spatial track gets
     constructed this frame)."* The registration is at `boot.rs:1279-1285`;
     `byroredux/src/main.rs` has zero scheduler `add_*` calls for audio. The
     ordering claim itself is correct — `reverb_zone_system` is in the Late
     parallel batch and `audio_system` is a Late exclusive, and
     `Scheduler::run` finishes the batch before the exclusives — only the file
     attribution is wrong.
- **Evidence**: `grep -n "reverb_zone\|audio_system\|add_exclusive\|add_to_with_access" byroredux/src/main.rs`
  returns one unrelated line (`:352`, a comment about `DebugDrainSystem`). The
  live registrations are `boot.rs:1017` (footstep), `boot.rs:1281` (reverb),
  `boot.rs:1303` (audio).
- **Impact**: Documentation only — no runtime behaviour. The cost is audit and
  maintenance friction: item 2 sends a reader to the wrong file when verifying
  the reverb-before-audio ordering, and item 1 actively understates what runs in
  `Stage::Late` for anyone reasoning about that stage's cost or access
  declarations.
- **Related**: Direct sibling of `AUD-2026-07-25-01` — the *same class* of
  main.rs→boot.rs attribution rot, in the *same file*, fixed 2026-08-03 for
  `footstep_system`'s docstring two functions further down
  (`byroredux/src/systems/audio.rs:97-98`, now correctly citing
  `scene.rs::setup_scene`). The `reverb_zone_system` docstring immediately above
  it was missed by that pass. Also the same class as #1859 / `AUD-2026-07-02-01`
  (`SoundCache` docstring citing the pre-Session-34 `asset_provider.rs` path).
- **Suggested Fix**: Two one-line edits — replace "main.rs" with
  "`byroredux/src/boot.rs`" in the `reverb_zone_system` docstring, and replace
  the boot.rs "Phase 1 body is a stub" sentence with a statement of what
  `audio_system` actually does today (listener sync → queue drain → entity
  dispatch → prune).

### AUD-2026-08-16-D7-01: `ROADMAP.md` M44 row reports stale test counts and contradicts itself on the reverb-toggle wiring

- **Severity**: LOW
- **Dimension**: Gameplay Audio Wiring
- **Location**: `ROADMAP.md:672` (M44 active-milestone row), `ROADMAP.md:1062`
  (closed known-issue entry)
- **Status**: NEW
- **Description**: `ROADMAP.md` is the project's authoritative source for M44
  status (per `.claude/commands/_audit-common.md`), and the M44 row carries two
  stale claims:

  1. *"**Tests**: 12 default + 5 `#[ignore]`d real-data integrations on cpal"*.
     Live: `cargo test -p byroredux-audio` reports **21 passed, 0 failed, 6
     ignored**. The drift accumulated across the #1612 attenuation guard, the
     #932/#844/#858 despawn guards, and this interval's two new #2394/#2405
     guards.
  2. `ROADMAP.md:1062` still lists *"per-cell-load reverb-toggle wiring (API
     ships, cell loader doesn't call yet)"* among M44's pending items — directly
     contradicted by `ROADMAP.md:672`'s own *"Cell-load reverb-toggle wiring
     closed 2026-05-08 (#846)"*, and by the live `reverb_zone_system`
     (`byroredux/src/systems/audio.rs:40-76`) with its five regression tests.
- **Evidence**: `cargo test -p byroredux-audio` →
  `test result: ok. 21 passed; 0 failed; 6 ignored`. `ROADMAP.md:672` vs
  `ROADMAP.md:1062`, same file, opposite claims about #846.
- **Impact**: Documentation only. The self-contradiction is the sharper half —
  a reader landing on the closed-issues list is told a shipped, tested feature
  is unbuilt, which is precisely the stale-premise trap that has produced ~5 of
  30 bad findings in past audit sweeps.
- **Related**: The audit-hygiene rule "verify the audit premise against current
  code before proposing a fix" exists because of exactly this kind of doc
  contradiction. `docs/feature-matrix.md:131-142` is currently accurate and can
  serve as the reconciliation target.
- **Suggested Fix**: At the next `/session-close`, refresh the M44 row's test
  counts to 21/6 and delete the reverb-toggle bullet from the `ROADMAP.md:1062`
  pending list (or mark it closed inline, pointing at #846), leaving FOOT / REGN
  / occlusion as the genuine remainder.

---

## Disproved candidates (investigated, not reported)

Recorded so the next cycle doesn't re-derive them.

- **Queue accumulation before the listener exists.**
  `drain_pending_oneshots` early-returns at `lib.rs:796` when
  `audio_world.listener` is `None`, leaving items in `pending_oneshots`;
  `play_oneshot` only gates on `manager.is_none()`. In principle a burst of up
  to 256 stale one-shots could dispatch at their stale positions once a listener
  finally appears. **Disproved as a live path**: the same entity gets both
  markers in one place (`scene.rs:840` `AudioListener`, `scene.rs:844`
  `FootstepEmitter`), and `footstep_system`'s first-tick seed (`#848`,
  `systems/audio.rs:140-144`) guarantees frame 1 enqueues nothing — so the
  listener is created on frame 1's `audio_system` tick, before the first
  possible enqueue on frame 2. Bounded, logged, and unreachable in practice.
- **`prune_stopped_sounds` mass-stopping every entity sound when the
  `AudioEmitter` storage is unregistered.** `emitter_q` is
  `world.query::<AudioEmitter>()` and the presence test falls back to `false`
  (`lib.rs:1047-1050`), which would mark every entity-driven sound for stop.
  **Disproved**: an `ActiveSound` with `entity: Some(..)` can only exist if
  `dispatch_new_oneshots` already succeeded at
  `world.query::<AudioEmitter>()` (`lib.rs:900`), so the storage is registered
  by construction whenever the fallback could matter.
- **`spawn_oneshot_at` leaking entities.** The helper spawns an entity that
  `prune_stopped_sounds` strips the `AudioEmitter` from but never despawns, and
  the "downstream cleanup system" its docstring defers to does not exist.
  **Disproved as a present defect**: `spawn_oneshot_at` has zero callers outside
  `crates/audio/src/tests.rs`, so no entity is ever created by it at runtime.
  Worth re-checking the moment a producer lands — noted in Future-Phase
  Readiness rather than filed.
- **`FootstepScratch` resource-mut lock held across two component-storage
  locks.** `footstep_system` acquires `FootstepScratch` at
  `systems/audio.rs:124` and holds it while acquiring `GlobalTransform` and
  `FootstepEmitter` (`129`/`132`). **Disproved as a deadlock risk**: resource
  and component locks are separate maps, no system acquires them in the reverse
  order, and `footstep_system` is registered `add_exclusive` so nothing runs
  concurrently with it. The early-returns inside that block drop the guard
  normally and `clear()` preserves the Vec capacity, so the #932 property is
  intact on those paths too.
- **`std::collections::HashMap` in `SoundCache` (`lib.rs:1221`) as a #2923
  violation.** **Disproved**: the #2923 `FxHashMap` rule is explicitly scoped to
  the per-frame render/skinning path. `SoundCache` is load-time, path-keyed, and
  currently has no producer at all.
- **`# Phase N (this commit)` headings for six different commits in the module
  docstring.** Real but previously adjudicated: `AUD-2026-06-14` flagged the
  phase-number collision, which was fixed by rewriting the "Future work" block
  to list items by name. The residual heading wording has been accepted across
  four subsequent cycles; not re-litigated.

---

## Future-Phase Readiness (invariants pinned for the next phase)

- **FOOT / 3.5b (per-material footstep sound)**: `FootstepConfig.default_sound`
  decoupling and `FootstepScratch` Vec reuse survive unchanged; a producer can
  wire per-material lookups without touching the stride / seed / `{0.5, 12.0}`
  attenuation logic. The queue path it uses is position-correct by contract, so
  AUD-2026-08-16-D1-01 does **not** block it.
- **REGN (ambient soundscapes)**: this is the phase AUD-2026-08-16-D1-01 blocks
  — region ambient layers are the archetypal long-lived looping emitter, and any
  that ride a moving entity will be position-frozen. Resolve the emitter
  reposition question *before* the REGN producer lands. Sub-track capacity (512)
  still exceeds the ~400-emitter populated-interior projection; sticky-listener
  and despawn-truncation guards cover mass emitter churn on cell streaming.
- **MUSC routing**: single-slot / main-track / streaming-type invariants pinned;
  parse→play wiring re-confirmed absent. The eventual caller must gate on FormID
  equality — re-playing the same handle re-decodes and re-streams.
- **`SoundCache` producer**: the decoupled API plus its three guards survive, so
  a first consumer can land without a structural rewrite — but it MUST wire
  eviction at the same time (no automatic LRU; `bytes_estimate` exists for the
  telemetry signal but still has no `stats` console consumer, per #850's
  accepted resolution).
- **Reverb per-cell acoustics**: the detector is binary interior/exterior; the
  bit-equality-gated transition in `reverb_zone_system` is the extension point,
  and `apply_reverb_send` is now the single place a per-cell send value reaches
  a track — the two knobs can no longer drift apart, which is what #2405's fix
  bought.
- **Entity-path dispatch failure handling**: #2394's fix is pinned by a
  source-level guard that also counts the `consumed.push` sites, so a newly
  added loop exit that forgets the marker fails the build rather than shipping.

---

## Delta vs prior report

This report supersedes `AUDIT_AUDIO_2026-08-07.md`. That cycle closed with one
NEW LOW (`AUD-2026-08-07-D5-01`) and one re-confirmed Existing MEDIUM (#2394).
This cycle:

- Verified **both** are now fixed and regression-guarded (`c0f3cda3`,
  `8a404914`; guards `reverb_send_gate_matches_silence_db_boundary` and
  `oneshot_marker_is_consumed_on_both_dispatch_failure_arms`), and confirmed
  neither issue appears in the 269-entry OPEN snapshot.
- Re-ran the suite live: 21 passed / 0 failed / 6 ignored, up from the prior
  cycle's 19 / 0 / 6.
- Surfaced one MEDIUM the ten-report chain had not caught: every prior cycle
  verified `ActiveSound._track`'s *name* and Drop-side-effect contract (the leak
  class) without ever asking whether the track's **position** is maintained. It
  is not, and the `AudioEmitter` docstring says it is.
- Surfaced two LOW doc-rot items: a sibling of the already-fixed
  `AUD-2026-07-25-01` attribution rot that the 2026-08-03 fix pass missed in the
  same file, and stale/self-contradictory M44 status in `ROADMAP.md`.

---

## Severity Counts

- **CRITICAL**: 0
- **HIGH**: 0
- **MEDIUM**: 1 (NEW: AUD-2026-08-16-D1-01)
- **LOW**: 2 (NEW: AUD-2026-08-16-D6-01, AUD-2026-08-16-D7-01)
