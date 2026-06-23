# Audio Subsystem Audit (M44) — 2026-06-23

Static, source-only audit of the M44 `byroredux-audio` crate
(`crates/audio/src/lib.rs`, 1292 lines + `crates/audio/src/tests.rs`,
1178 lines) plus the only live engine-side consumers
(`byroredux/src/systems/audio.rs` — `footstep_system` +
`reverb_zone_system`, `byroredux/src/asset_provider.rs` —
`try_load_default_footstep`, and the schedule wiring in
`byroredux/src/main.rs`). All 7 dimensions, depth=deep. No `cargo test`
run — source-correctness pass, verified against the live kira `0.10` API.

## Executive Summary

**Phases shipped (per crate docstring + verified against the live API)**:
Phases 1–6 are all present and the docstring no longer drifts from the
shipped surface (the "Future work" block at `lib.rs:107–116` now reads
"Phases 4–6 above have shipped" and lists pending work by name, not by a
contradictory phase number).

| Phase | Surface | Status |
|-------|---------|--------|
| 1 | `AudioWorld` resource, `AudioListener`/`AudioEmitter`/`OneShotSound`, `audio_system` | ✓ |
| 2 | `load_sound_from_bytes` (symphonia) + `SoundCache` | ✓ |
| 3 | Spatial sub-track playback + `spawn_oneshot_at` | ✓ |
| 3.5 | `play_oneshot` queue API | ✓ |
| 4 | Looping emitters + tweened stop on emitter-remove | ✓ |
| 5 | `load_streaming_sound_from_*` + `play_music`/`stop_music` (single-slot, non-spatial) | ✓ |
| 6 | One global reverb send track, per-track `with_send` opt-in, default `NEG_INFINITY` | ✓ |

**Engine-side consumers shipped**:
- Footstep gameplay loop — `footstep_system` (the only live `play_oneshot`
  caller), wired onto the fly-cam in `scene.rs:443` via `FootstepEmitter`.
- Per-cell reverb — `reverb_zone_system` (the only live
  `set_reverb_send_db` caller), `-12 dB` interior / `NEG_INFINITY` exterior.

**Pending (NOT flagged as missing — out of scope)**: Phase 3.5b FOOT
records → per-material footstep sound; REGN ambient soundscapes; MUSC +
hardcoded music routing; per-cell-acoustic reverb (current detector is
binary interior/exterior); raycast occlusion attenuation.

**MUSC parse→play gap (confirmed, expected)**: cell-music FormIDs ARE
parsed (`default_music` ZNAM, `music_type_form` XCMO — `crates/plugin/src/esm/cell/`),
but NO caller invokes `play_music` (grep across `byroredux/` returns zero
non-test hits). This is the documented future-phase boundary, not a
regression. The single-slot / main-track invariants are pinned for the
eventual caller (see Future-Phase Readiness).

**Headless-mode boot status: PASS.** `AudioWorld::new` falls back to
`manager: None` on `AudioManager::new` failure (`lib.rs:280–297`), the
reverb send-track creation also falls back cleanly (`lib.rs:303–321`), and
every public method gates on the inner `Option<AudioManager>` via
`let Some(mgr) = ... else { return; }` or `audio_system`'s `is_active()`
early-return (`lib.rs:659`). No `unwrap()` on the inner option anywhere.
`play_oneshot` now drops up-front when `manager.is_none()` (`lib.rs:385`),
so the queue cannot fill on a headless host (#853). Guards:
`audio_world_constructs_without_panic_on_any_environment`,
`audio_system_no_op_when_audio_world_inactive`,
`play_oneshot_drops_when_manager_inactive`.

**Findings count by severity**: CRITICAL 0 · HIGH 0 · MEDIUM 0 · LOW 1.
Total: 1 (a single deliberate-dup observation; no correctness bug found).

### Delta vs prior reports

This is the strongest the subsystem has audited. Every finding from BOTH
prior reports is now CLOSED and regression-guarded:

**`AUDIT_AUDIO_2026-05-05.md` (11 findings)** — all resolved:
- AUD-D1-NEW-01 (HIGH, kira default cap 128) → FIXED. `SUB_TRACK_CAPACITY
  = 512` / `SEND_TRACK_CAPACITY = 32` consts (`lib.rs:154–155`), applied
  in `new()` (`lib.rs:272–279`). Guard: `manager_capacities_exceed_kira_defaults`.
- AUD-D2-NEW-02 (multi-listener silent) → FIXED. Debounced one-shot warn
  via `multi_listener_warned` (`lib.rs:702–715`).
- AUD-D3-NEW-03 (re-stop every tick) → FIXED. `stop_issued` flag on
  `ActiveSound` (`lib.rs:190`, `1007`, `1038`).
- AUD-D4-NEW-04 (hard-coded 10 ms fade) → FIXED. Per-emitter
  `unload_fade_ms` (`lib.rs:606`), read in prune (`lib.rs:1028`).
- AUD-D5-NEW-05 (reverb detector not wired) → FIXED. `reverb_zone_system`
  (`systems/audio.rs:40`).
- AUD-D5-NEW-06 (build-time send level) → documented limitation (#847), not
  a bug; the docstring at `lib.rs:491–504` carries the contract.
- AUD-D6-NEW-07 (footstep_system stale GlobalTransform) → FIXED. Moved to
  `Stage::PostUpdate` after transform propagation (`main.rs:824`, #848).
- AUD-D6-NEW-08 (sticky listener) → documented contract (`lib.rs:673–686`).
- AUD-D6-NEW-09 (SoundCache no eviction) → documented dormant API (#859);
  `clear()` + `bytes_estimate` exist for the future consumer.
- AUD-D3-NEW-10 (mem::take before manager check) → FIXED. Gate moved before
  the take (`lib.rs:773–776`, #851).
- AUD-D3-NEW-11 (`Vec::remove(0)` O(n)) → FIXED. `pending_oneshots` is a
  `VecDeque` with `pop_front` (`lib.rs:228`, `397`, #852).

**`AUDIT_AUDIO_2026-06-14.md` (4 findings, issues #1612–#1615)** — all CLOSED:
- AUD-2026-06-14-01 (reversed `Attenuation` panics kira render thread) →
  FIXED. `Attenuation::distance_range()` clamp-normalizes `min`/`max`
  (`lib.rs:550–554`, #1612). Guard: `reversed_attenuation_normalizes_instead_of_panicking`.
- AUD-2026-06-14-02 (`bytes_estimate` present-tense telemetry doc) → FIXED.
  Docstring now reads "Not yet wired — no non-test caller exists"
  (`lib.rs:1278`) and "No `stats` consumer exists today" (`lib.rs:1169`).
- AUD-2026-06-14-03 (Future-phases doc collision) → FIXED. Block now
  reads "Phases 4–6 above have shipped" (`lib.rs:109`).
- AUD-2026-06-14-04 (stale `resolve_footstep_sound` in docstring) → FIXED.
  `lib.rs:1176` now reads `try_load_default_footstep`.

## Lifecycle Invariant Matrix

Field-drop order is declaration order in `pub struct AudioWorld`
(`lib.rs:218–258`). Rust drops top-to-bottom; handles must drop before the
manager that owns the kira device.

| Order | Field | Drop side effect | Status |
|-------|-------|------------------|--------|
| 1 | `active_sounds: Vec<ActiveSound>` (owns `SpatialTrackHandle` via `_track`) | drops handles → kira marks resources for removal | ✓ verified |
| 2 | `pending_oneshots: VecDeque<PendingOneShot>` (data only) | none | ✓ |
| 3 | `music: Option<StreamingSoundHandle>` | internal mark-for-removal | ✓ |
| 4 | `reverb_send: Option<SendTrackHandle>` | drops send track | ✓ |
| 5 | `reverb_send_db: f32` | none | ✓ (no-op) |
| 6 | `listener: Option<ListenerHandle>` | mark-for-removal | ✓ |
| 7 | `manager: Option<AudioManager>` | tears down audio device | ✓ verified |
| 8 | `multi_listener_warned: bool` | none | ✓ (no-op) |

The handle-before-manager invariant holds: `active_sounds` (track handles)
→ `music` → `reverb_send` → `listener` all drop before `manager`. The two
plain-data fields between handles (`reverb_send_db` at 5, `multi_listener_warned`
at 8) have no `Drop` and are invisible to drop sequencing.

**Per-handle owner audit** — all single-owner, no leak path:
- `SpatialTrackHandle` → `ActiveSound._track` (underscore-pin for Drop side
  effect, `lib.rs:174`); never moved out; lands in `active_sounds` before
  the dispatch helper returns (both paths: `lib.rs:818`, `960`).
- `StaticSoundHandle` → `ActiveSound.handle`; polled via `state()` + tweened
  `stop()`.
- `ListenerHandle` → `AudioWorld.listener`; created once, **never cleared**
  on entity churn (#849 sticky-listener contract, `lib.rs:673–686`).
- `SendTrackHandle` → `AudioWorld.reverb_send`; read-only after init
  (`id()` only).
- `StreamingSoundHandle` → `AudioWorld.music`; single slot, fade+replace.

**Despawn-truncation guard (#845/#858/SAFE-23)**: `prune_stopped_sounds`
stops any `entity == Some` sound whose `AudioEmitter` query misses, using
the captured `unload_fade_ms`, debounced by `stop_issued`, dropping the
entry only on `PlaybackState::Stopped`, and removing the `AudioEmitter` on
completion. Applies to looping AND non-looping (`lib.rs:986–1063`).
Queue-driven (`entity == None`) sounds are exempt and run to natural
termination. Guards: `looping_emitter_survives_natural_duration_and_stops_on_emitter_remove`,
`non_looping_emitter_stops_on_emitter_remove_regression_858`.

## Findings

### AUD-2026-06-23-01: vol→dB conversion duplicated verbatim across three dispatch sites (no drift; helper-extraction opportunity)

- **Severity**: LOW
- **Dimension**: Spatial Sub-Track Lifecycle
- **Location**: `crates/audio/src/lib.rs:438–442` (`play_music`),
  `:805–809` (`drain_pending_oneshots`), `:937–941` (`dispatch_new_oneshots`)
- **Status**: NEW
- **Description**: The linear-amplitude → decibels conversion
  (`if vol > 0.0001 { 20.0 * vol.log10() } else { -60.0 }`) is copy-pasted
  identically into all three sound-dispatch paths. The skill's Dimension 1
  checklist asks to "flag drift between the three copies (a divergent clamp
  is a real bug)" — **there is no drift**: all three copies are byte-for-byte
  identical (`> 0.0001` epsilon, `20.0 * log10`, `-60.0` silence clamp), so
  this is NOT a correctness bug. It is a maintainability hazard only: a
  future tweak to one site (e.g. changing the silence floor to match the
  reverb gate's `-60.0` cutoff, or the epsilon) could silently diverge the
  others.
- **Evidence**:
  ```rust
  // identical at lib.rs:438, 805, 937
  let db = if /* vol */ > 0.0001 {
      20.0 * /* vol */.log10()
  } else {
      -60.0
  };
  ```
- **Impact**: None today (all copies agree). Future risk of inconsistent
  per-path loudness if one site is edited without the others. Blast radius
  is bounded to gain — no AS/SSBO/GPU correctness exposure.
- **Related**: Reverb-send gate (`is_finite() && > -60.0`) is also
  duplicated verbatim at `lib.rs:794` and `:916` — same no-drift, dup-only
  situation; a shared `linear_to_db(f32) -> f32` free fn plus a
  `reverb_gate` helper would collapse both.
- **Suggested Fix**: Extract a private `fn linear_volume_to_db(v: f32) -> f32`
  (and optionally a `fn reverb_send_for(&self) -> Option<(SendTrackId, f32)>`)
  and call it from the three/two sites. Low priority — the inline copies are
  small and currently consistent.

## Future-Phase Readiness

Invariants this audit pinned for the next M44 phases:

- **3.5b FOOT records → per-material sound**: dispatch already routes
  through `play_oneshot` (queue path) — stable. Per-material lookup only
  needs to choose the right `Arc<StaticSoundData>` before enqueue. The
  `SUB_TRACK_CAPACITY = 512` headroom (#842) now covers the burst of
  overlapping footsteps from multiple NPCs; the prior HIGH ceiling is gone.
- **REGN ambient soundscapes**: the prune sweep's "AudioEmitter removed →
  tweened stop" path (looping + non-looping, per-emitter `unload_fade_ms`)
  is verified. REGN's cell-unload must remove the `AudioEmitter` component
  (or despawn the entity); it must NOT silently drop kira handles.
- **MUSC routing**: single-slot `music` field + non-spatial main-track
  dispatch is pinned. The eventual caller MUST gate `play_music` on FormID
  equality (re-loading the same `StreamingSoundHandle` re-decodes +
  re-streams). `play_music` already crossfades (fade-out old + fade-in new
  over the same tween) and never blocks on `is_music_active` (it always
  fades+replaces), so re-entrancy during a fade tail is safe.
- **Per-cell acoustic reverb**: build-time-only `with_send` (kira 0.10 has
  no runtime `set_send_volume`) means a reverb-level flip applies to NEW
  tracks only (#847). The cell-load handoff is correct by design
  (`reverb_zone_system` runs before `audio_system` in `Stage::Late`, so a
  track built this frame picks up this frame's send level); long-running
  ambients across an interior↔exterior transition keep their build-time
  level until they end (documented limitation, AUD-D5-NEW-06).
- **Raycast occlusion**: `StaticSoundHandle::set_volume` IS available
  post-construction, so a per-emitter "blocked by geometry" multiplier can
  be layered on without an architectural change. No blockers found.

## Methodology Note

Per `feedback_audit_findings.md`, every prior-report finding was re-checked
against current code before being recorded as resolved (not assumed). Two
candidate new findings were investigated and **disproved**:

- **(disproved) "reverb gate drifts between the two dispatch paths"**:
  `lib.rs:794` and `:916` are character-identical
  (`reverb_send_db.is_finite() && reverb_send_db > -60.0`). No divergence.
  Recorded only as a dup note under AUD-2026-06-23-01 (Related).
- **(disproved) "footstep_system reads stale GlobalTransform"**: this was
  the prior AUD-D6-NEW-07; verified FIXED — `footstep_system` is now
  `add_exclusive(Stage::PostUpdate, ...)` at `main.rs:824`, sequencing AFTER
  `make_transform_propagation_system()` (`main.rs:800`). The
  `FootstepScratch` Vec-reuse (#932) restores the moved-out buffer on BOTH
  the success path (`systems/audio.rs:197`) and the `AudioWorld`-absent bail
  (`systems/audio.rs:174`), and the scratch mut-lock is dropped before
  `AudioWorld` is acquired (`systems/audio.rs:167–169`) — no double
  resource-mut hold.

Stage-ordering claim verified against the scheduler: parallel systems run
first, exclusive systems sequentially after (`crates/core/src/ecs/scheduler.rs:9`,
`:273–278`). `reverb_zone_system` (parallel batch) therefore runs before
`audio_system` (exclusive) within `Stage::Late` — the ordering the
`main.rs:912–924` comment asserts holds structurally.

## Dedup Status

`/tmp/audit/issues.json` (open issues) + `gh` closed-issue queries scanned
for `audio|kira|sound|reverb|listener|footstep|AUD-|attenuation|music`.

- **Open audio issues: zero.**
- Closed: #1612–#1615 (the four 06-14 findings) — all verified fixed in
  current code, no regression. #842, #844, #845, #846, #847, #848, #849,
  #850, #851, #852, #853, #858, #859, #932, #1612 referenced as the
  closed-fix provenance for the regression guards above.

AUD-2026-06-23-01 is NEW (the verbatim vol→dB / reverb-gate duplication has
not been filed; the prior reports flagged drift-as-bug, which does not
exist here — this is the dup-only residue).

## Suggested Next Action

```
/audit-publish docs/audits/AUDIT_AUDIO_2026-06-23.md
```

Only one LOW finding; it can batch with other tech-debt cleanups. The
subsystem is otherwise clean — no CRITICAL/HIGH/MEDIUM, headless boot PASS,
every prior finding closed and regression-guarded.
