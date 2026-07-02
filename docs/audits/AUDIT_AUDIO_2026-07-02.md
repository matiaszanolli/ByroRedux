# Audio Subsystem Audit (M44) — 2026-07-02

Static, source-only audit of the M44 `byroredux-audio` crate
(`crates/audio/src/lib.rs`, 1293 lines; tests in
`crates/audio/src/tests.rs`, 1178 lines) plus the engine-side consumers
that drive its API (`byroredux/src/systems/audio.rs`,
`byroredux/src/components.rs`, `byroredux/src/asset_provider/texture.rs`,
schedule wiring in `byroredux/src/main.rs`, listener/footstep opt-in in
`byroredux/src/scene.rs`). kira pinned at `0.10.8` (workspace
`Cargo.toml` → `Cargo.lock`).

Scope: all 7 dimensions, depth = deep. No `cargo test` run — this is a
source-correctness pass. Every finding candidate was re-checked against
current source before inclusion.

## Executive Summary

**Crate Phases 1–6 shipped, all verified against the live API:**

- Phase 1 — `AudioWorld` (graceful-degradation `Option<AudioManager>`),
  `AudioListener` / `AudioEmitter` / `OneShotSound` components,
  `audio_system`. ✓
- Phase 2 — `load_sound_from_bytes` (symphonia decode), `SoundCache`. ✓
- Phase 3 — spatial sub-track playback, `spawn_oneshot_at`. ✓
- Phase 3.5 — `AudioWorld::play_oneshot` queue API (`VecDeque`,
  cap 256, drop-oldest via `pop_front`). ✓
- Phase 4 — looping emitters (`loop_region(..)`); tweened `stop()` on
  emitter-component removal, `stop_issued` debounce, per-emitter
  `unload_fade_ms`. ✓
- Phase 5 — `load_streaming_sound_from_bytes` / `_from_file`,
  single-slot `play_music` / `stop_music` (main-track, non-spatial). ✓
- Phase 6 — one global `SendTrackBuilder` reverb send, per-new-track
  `with_send` opt-in gated on `is_finite() && > -60.0`, default
  `f32::NEG_INFINITY`. ✓

**Engine consumers shipped:**
- Footstep gameplay loop — `footstep_system` (`Stage::PostUpdate`,
  after transform propagation), XZ-plane stride accumulation, `play_oneshot`
  dispatch, `FootstepScratch` Vec reuse.
- Per-cell reverb — `reverb_zone_system` (`Stage::Late`, before
  `audio_system`), interior `-12 dB` / exterior `NEG_INFINITY`,
  bit-equality-gated transition.

**Pending (future-phase, NOT flagged as missing):** Phase 3.5b FOOT
records → per-material sound; REGN ambient soundscapes; MUSC + hardcoded
music routing; per-cell acoustic reverb (current detector is binary
interior/exterior); raycast occlusion attenuation.

**MUSC parse→play gap (confirmed, by design):** cell-music FormIDs are
parsed (`default_music` ZNAM, `music_type_form` XCMO in
`crates/plugin/src/esm/cell/`) but **no caller invokes `play_music`** —
`grep play_music byroredux/` returns zero non-test hits. This is a
future-phase gap; the single-slot / main-track invariants are pinned for
the eventual caller (Dim 4).

**Headless-mode boot status: PASS.** `AudioWorld::new()` falls back to
`manager: None` on `AudioManager::new` failure; `reverb_send` builder
falls back to `None`; every manager-touching path gates on
`is_some()` / `is_active()` with no `unwrap()` on the inner
`Option<AudioManager>`. Guard: `audio_world_constructs_without_panic_on_any_environment`.

**Findings count by severity:**
- CRITICAL: 0
- HIGH: 0
- MEDIUM: 0
- LOW: 1
- Total: 1

**Delta vs the prior report `docs/audits/AUDIT_AUDIO_2026-05-05.md`
(findings AUD-D1..D6-NEW-01..11 / issues #842–#859):** every one is now
**fixed and regression-guarded**. This audit found no regression of any
of them. Summary of the closure state verified in current source:

| Prior finding | Fix verified in current code | Guard |
|---|---|---|
| D1-01 kira default `sub_track_capacity=128` | `SUB_TRACK_CAPACITY=512` / `SEND_TRACK_CAPACITY=32` applied in `new()` | `manager_capacities_exceed_kira_defaults` |
| D2-02 multi-listener silent pick | debounced one-shot WARN via `multi_listener_warned` | (#843) |
| D3-03 re-`stop()` every tick | `stop_issued` flag on `ActiveSound`, skip-if-set | `looping_emitter_survives...` |
| D4-04 hard-coded 10 ms fade | `AudioEmitter.unload_fade_ms` + `ActiveSound.unload_fade_ms` capture; `DEFAULT_UNLOAD_FADE_MS=10.0` | `audio_emitter_authors_custom_unload_fade...` |
| D5-05 reverb detector unwired | `reverb_zone_system` wired in `main.rs` (`Stage::Late`, before `audio_system`) | `interior_cell_sets_subtle_reverb_send` |
| D5-06 build-time-only `with_send` | documented limitation (#847), not a bug | — |
| D6-07 footstep stale-pose (`Stage::Update`) | moved to `Stage::PostUpdate` after `make_transform_propagation_system()` | `single_large_jump_fires_one_footstep_only` |
| D6-08 sticky listener contract | `listener` never cleared; doc contract on `sync_listener_pose` | (#849) |
| D6-09 `SoundCache` unbounded | dormant API (`len()==0` steady state); `clear()` + `bytes_estimate()` present | `sound_cache_clear_drops_entries...` |
| D3-10 `mem::take` before manager gate | manager gate moved before `std::mem::take` | (#851) |
| D3-11 `Vec::remove(0)` O(n) | switched to `VecDeque` + `pop_front` | (#852/#853) |

Additional post-report hardening also verified in place: #858/SAFE-23
(non-looping despawn truncation), #932 (`FootstepScratch` Vec reuse),
#1612 (reversed-`Attenuation` normalize via `distance_range`), #1776
(archive "requested but zero opened" check).

## Lifecycle Invariant Matrix

Field-drop order (Rust drops struct fields in declaration order). Current
declaration in `pub struct AudioWorld` (`crates/audio/src/lib.rs:234-274`):

| Order | Field | Type | Drop side effect | Status |
|---|---|---|---|---|
| 1 | `active_sounds` | `Vec<ActiveSound>` (owns `StaticSoundHandle` + `SpatialTrackHandle`) | drops handles → kira marks resources for removal | ✓ |
| 2 | `pending_oneshots` | `VecDeque<PendingOneShot>` (data only) | none | ✓ |
| 3 | `music` | `Option<StreamingSoundHandle<FromFileError>>` | none (kira-internal mark-for-removal) | ✓ |
| 4 | `reverb_send` | `Option<SendTrackHandle>` | drops send track | ✓ |
| 5 | `reverb_send_db` | `f32` | none | ✓ (no-op) |
| 6 | `listener` | `Option<ListenerHandle>` | mark-for-removal | ✓ |
| 7 | `manager` | `Option<AudioManager<DefaultBackend>>` | tears down audio device | ✓ |
| 8 | `multi_listener_warned` | `bool` | none | ✓ (no-op) |

Invariant "track handles drop before listener drops before manager
drops" is preserved. `reverb_send_db: f32` and `multi_listener_warned:
bool` sit among the handles but have no `Drop`, so their positions are
invisible to drop sequencing.

Per-handle owners:
- `SpatialTrackHandle` → `ActiveSound._track` (underscore-pin at
  `lib.rs:190`); held for Drop side effect only, never moved out.
- `StaticSoundHandle` → `ActiveSound.handle`; `state()` polling +
  `stop(Tween)` on despawn truncation.
- `ListenerHandle` → `AudioWorld.listener`; sticky across entity churn
  (never cleared), per-frame `set_position`/`set_orientation`.
- `SendTrackHandle` → `AudioWorld.reverb_send`; read-only after init
  (`id()` at `with_send` time).
- `StreamingSoundHandle` → `AudioWorld.music`; `stop(fade)`-ed before
  reassignment in `play_music`.

Dispatch-path equivalence (Dim 1): entity path (`dispatch_new_oneshots`,
`entity = Some`) and queue path (`drain_pending_oneshots`, `entity =
None`) both gate on `listener_id`, both apply the same reverb-send gate,
both push into `active_sounds` before returning, and both reuse the
`Arc<[Frame]>` PCM via `(*sound).clone().volume(db)` (no deep clone).
The `20·log10` volume→dB conversion is now factored into one
`linear_volume_to_db` helper (AUD-2026-06-23-01) consumed by all three
play sites — no drift possible.

## Findings

### AUD-2026-07-02-01: `SoundCache` docstring points to `asset_provider.rs` (now a directory) instead of `asset_provider/texture.rs`

- **Severity**: LOW
- **Dimension**: SoundCache Growth
- **Location**: `crates/audio/src/lib.rs:1176-1178`
- **Status**: NEW
- **Description**: The `SoundCache` struct docstring (the "Dormant API
  (#859)" block) states the footstep path lives at
  "`byroredux/src/asset_provider.rs::try_load_default_footstep`". Post
  the Session-34 module split, `asset_provider.rs` no longer exists as a
  single file — it is a directory (`byroredux/src/asset_provider/`), and
  `try_load_default_footstep` lives in `asset_provider/texture.rs`. The
  path in the docstring resolves to a file that is not on disk.
- **Evidence**:
  ```
  # crates/audio/src/lib.rs:1176-1178 (current)
  /// call sites for `SoundCache`. The footstep dispatch path at
  /// `byroredux/src/asset_provider.rs::try_load_default_footstep` writes
  /// directly into `FootstepConfig.default_sound: Option<Arc<Sound>>`,

  $ ls byroredux/src/asset_provider*
  byroredux/src/asset_provider/   (directory)
  $ grep -rln 'fn try_load_default_footstep' byroredux/src/
  byroredux/src/asset_provider/texture.rs
  ```
- **Impact**: Documentation only; no runtime effect. A maintainer
  following the reference lands on a missing file. This is exactly the
  stale-path class the `_audit-common.md` path-reference convention
  targets. Same family as the already-closed #1615 doc-rot fix (which
  corrected the *function name* in this same block but left the *file
  path* stale).
- **Related**: #1615 (AUD-2026-06-14-04, closed — corrected the stale
  `resolve_footstep_sound` fn name in the adjacent line); Session-34
  module-split layout note.
- **Suggested Fix**: Change `asset_provider.rs::try_load_default_footstep`
  to `asset_provider/texture.rs::try_load_default_footstep` in the
  docstring.

## Dimension-by-Dimension Verification (clean)

The following were audited deep and found correct — no finding:

- **Dim 1 (Spatial Sub-Track Lifecycle & Leaks)** — `_track` name pin
  intact; both dispatch paths gate on `listener_id` and short-circuit
  frame-1; `active_sounds.push` before helper return; `looping` applied
  in entity path only; drain-cap WARN at >32; producer cap at 256 via
  `VecDeque::pop_front` with up-front `manager.is_none()` drop; manager
  gate precedes `std::mem::take`; entity path reads post-propagation
  `GlobalTransform.translation`.
- **Dim 2 (Listener Pose & Attenuation)** — lazy `add_listener` on first
  resolved `GlobalTransform`; sticky handle (never cleared on entity
  despawn); debounced multi-listener WARN; `Tween::default()` per-frame
  pose smoothing; `Attenuation::distance_range()` returns a normalized
  `RangeInclusive` (`min<=max` enforced, #1612); `add_listener` failure
  leaves `listener=None` and retries next frame. Coordinate frame:
  `GlobalTransform.rotation` is renderer-space (Y-up conversion done
  upstream in NIFAL `coord.rs`), so the `Quat` handed to kira is correct
  — no channel-inversion.
- **Dim 3 (SoundCache Growth)** — keys lowercased exactly once at
  `get`/`insert`/`get_or_load`; manual `clear()` only, no auto-LRU
  (documented, acceptable); `clear()` doesn't invalidate live
  `ActiveSound` Arcs (kira holds its own clone); zero engine call sites
  (`try_load_default_footstep` bypasses the cache → `len()==0` steady
  state); `bytes_estimate` = `frames.len() * size_of::<Frame>()`;
  `get_or_load` invokes loader only on miss. (One doc-rot finding above.)
- **Dim 4 (Streaming Music Lifecycle)** — exactly one `music` slot;
  routes through `mgr.play(...)` main track (not spatial); streaming
  types (`StreamingSoundData::from_cursor`/`from_file`), not buffered;
  `stop_music` fade = `fade_out_secs.max(0.0)`; `is_music_active` false
  on `Stopped`; MUSC parse→play gap confirmed absent (future-phase).
- **Dim 5 (Reverb Send & Routing)** — one global send in `new()`
  (`feedback(0.85) damping(0.6) stereo_width(1.0) mix(WET)`); `None`
  path never cascades to `unwrap()`; both dispatch paths apply the same
  `is_finite() && > -60.0` gate; default `reverb_send_db =
  f32::NEG_INFINITY`; construction-time-only send is documented (#847).
- **Dim 6 (Manager & ECS/Cell Streaming)** — graceful degradation;
  capacities applied; field-drop order correct (matrix above);
  `AudioWorld`/`SoundCache` are `Resource` (`&self` access); `new()` is
  boot-only; `audio_system` registered `add_exclusive(Stage::Late)` with
  body order `sync_listener_pose → drain_pending_oneshots →
  dispatch_new_oneshots → prune_stopped_sounds`; `footstep_system`
  (`Stage::PostUpdate`) precedes `audio_system` (`Stage::Late`) — same
  frame; `OneShotSound` removed after dispatch; despawn truncation via
  `AudioEmitter`-absence + `stop_issued` + `retain` on `Stopped` +
  emitter removal; queue-driven (`entity==None`) exempt from truncation.
  Scheduler semantics verified: parallel batch runs before exclusive
  systems (registration order), so `camera_follow_system` (parallel,
  writes camera GT in Late) precedes the exclusive `reverb_zone_system`
  then `audio_system` — the listener pose read is current, not stale.
- **Dim 7 (Gameplay Audio Wiring)** — stride accumulation resets to 0.0
  on fire (not subtract-remainder); first-tick seed without firing
  (#848); `FootstepScratch.triggers` clear-reused + `mem::take`-drained,
  capacity restored on BOTH success and `AudioWorld`-absent bail paths
  (#932); scratch mut-lock dropped before `AudioWorld` acquired (no
  two-lock hold, no query lock across `play_oneshot`); footstep
  attenuation `{0.5, 12.0}`; silent no-op contracts (no
  `FootstepConfig`/`default_sound`/`FootstepScratch`/`AudioWorld` →
  return, no panic/spam); component-driven camera opt-in
  (`scene.rs:443/447`); `try_load_default_footstep` no-ops cleanly on
  absent `--sounds-bsa` / unopenable BSA / missing canonical path /
  decode failure; `reverb_zone_system` `-12.0`/`NEG_INFINITY` consts,
  bit-equality gate, safe no-ops, registered before `audio_system`.

## Future-Phase Readiness

Invariants this audit re-pinned for the next phases:

- **Phase 3.5b (FOOT → per-material sound):** `play_oneshot` queue path
  is stable; per-material lookup only needs to select the right
  `Arc<StaticSoundData>` before enqueue — no `AudioWorld` structural
  change. `SUB_TRACK_CAPACITY=512` gives headroom for combat-time
  overlapping footsteps. Footstep dispatch already reads post-propagation
  pose (`Stage::PostUpdate`), so per-material fan-out inherits correct
  spatial position.
- **REGN ambient soundscapes:** cell-unload must remove the
  `AudioEmitter` component (or despawn the entity) — the prune sweep then
  issues the tweened `unload_fade_ms` stop. Long-tail ambients should
  author a longer `unload_fade_ms` (200–500 ms) to avoid the 10 ms-cutoff
  click.
- **MUSC routing:** single-slot `music` field + main-track dispatch are
  pinned. The eventual caller must gate `play_music` on FormID equality
  (re-loading the same `StreamingSoundHandle` re-decodes + re-streams).
  No test pins the equality gate today (there is no caller).
- **Per-cell acoustic reverb / occlusion:** current detector is binary
  interior/exterior; `with_send` is build-time-only (#847), so a
  reverb-flip won't retro-apply to already-playing looping ambients —
  the flip handler must re-dispatch them. `Attenuation` is min/max linear
  falloff; occlusion needs a per-emitter blocked-multiplier (kira's
  `StaticSoundHandle::set_volume` can apply post-construction, so the
  shape supports it).

## Methodology Note

Per `feedback_audit_findings.md` (~5 of 30 findings in the 2026-04 sweep
were stale-premise), every candidate was checked against current source:

- All 11 prior findings (#842–#859) re-verified as fixed + guarded
  (table above) — no regression reported.
- The one NEW finding (`asset_provider.rs` → `asset_provider/texture.rs`)
  was confirmed with `ls byroredux/src/asset_provider*` (directory, not
  file) and `grep -rln 'fn try_load_default_footstep'` (resolves only to
  `asset_provider/texture.rs`).

Candidates disproved during the audit and NOT included:
- **"listener pose stale (read before camera_follow writes GT)"** —
  disproved: `camera_follow_system` is in the Late *parallel* batch,
  which the scheduler runs *before* the Late *exclusive* systems
  (`scheduler.rs:9,109-111`); `audio_system` is exclusive, so it reads
  the freshly-written camera GT.
- **"streaming music fade cut short on handle drop in `play_music`"** —
  not reported: `play_music` has zero callers today (future-phase MUSC
  gap), so it is not a present regression surface; the kira 0.10.8
  registry source was not unpacked locally to confirm streaming-handle
  drop semantics, so no claim is made either way. Pinned as a
  future-caller contract instead.
- **`linear_volume_to_db` threshold `volume > 0.0001`** — benign: the
  cutoff is ~-80 dB linear, well below the `SILENCE_DB = -60.0` clamp;
  no audible or correctness effect.

## Dedup Status

`/tmp/audit/audio/issues.json` (300 most recent, all states) scanned for
`audio`, `kira`, `sound`, `reverb`, `listener`, `music`, `oneshot`,
`footstep`, `attenuat`, `aud-`. Three matches, all CLOSED and all
already reflected in current source:

- #1615 (AUD-2026-06-14-04) — stale `resolve_footstep_sound` docstring →
  fixed (fn name corrected; but the *file path* in the same block is now
  the NEW finding above).
- #1613 (AUD-2026-06-14-02) — `bytes_estimate` present-tense stats claim
  → fixed (docstrings hedge future tense: "once a `stats` consumer wires
  `SoundCache`", "Not yet wired").
- #1612 (AUD-2026-06-14-01) — reversed `Attenuation` panic → fixed
  (`distance_range()` normalizes `min<=max`; guard
  `reversed_attenuation_normalizes_instead_of_panicking`).

The prior audio report `docs/audits/AUDIT_AUDIO_2026-05-05.md` (findings
#843–#859) is superseded — all its findings are closed and guarded.

## Suggested Next Action

```
/audit-publish docs/audits/AUDIT_AUDIO_2026-07-02.md
```

One LOW doc-rot finding — a one-line docstring path edit. The audio
subsystem is otherwise in a clean, fully-guarded state: headless boot
PASSES, all prior findings are fixed, and no CRITICAL/HIGH/MEDIUM issues
were found across the 7 dimensions.
