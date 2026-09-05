---
description: "Deep audit of the M44 audio subsystem — kira backend, spatial sub-tracks, listener pose, reverb send, streaming music"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Audio Subsystem Audit (M44)

Audit the `byroredux-audio` crate (M44, Phases 1–6 shipped) for correctness
across the kira `0.10` integration: spatial sub-track lifecycle + leaks,
listener pose / attenuation correctness, `SoundCache` growth, streaming-music
handle lifecycle, global reverb send routing, graceful-degradation manager, and
the ECS lifecycle of audio components across cell streaming. Plus the only live
engine-side consumers (`footstep_system`, `reverb_zone_system`).

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, methodology,
deduplication, context rules, and finding format. See
`.claude/commands/_audit-severity.md` for the severity scale. Do NOT duplicate
those here.

## Scope

**Crate**: `crates/audio/src/lib.rs` (single-file production module; tests split
into `crates/audio/src/tests.rs` post-Session-34). kira version pinned at
`0.10` in the workspace `Cargo.toml`. If `lib.rs` splits into submodules, update
the per-dimension entry points before launching.

**Engine-side consumers** (Dimension 7 — outside the crate):
`byroredux/src/systems/audio.rs` (`footstep_system` + `reverb_zone_system`),
`byroredux/src/components.rs` (`FootstepEmitter` / `FootstepConfig` /
`FootstepScratch`), `byroredux/src/asset_provider/texture.rs`
(`try_load_default_footstep` populates `FootstepConfig.default_sound`), and
**`byroredux/src/asset_provider/audio.rs`** (`dispatch_region_ambient_music`,
`stop_region_ambient_music`, `SoundArchiveProvider`, `resolve_sound_path`,
`sound_loops`, `sound_archive_path`, `build_sound_archive_provider` — the
REGN ambient-music dispatch, wired from `cell_loader/load.rs` and
`scene::apply_cell_region_ambient`). These are the ONLY live callers of
`play_oneshot` / `play_music` / `stop_music` / `set_reverb_send_db`, so the
crate-API audit is incomplete without them.

**Ground truth — read these before auditing**:
- The `crates/audio/src/lib.rs` module docstring enumerates Phases 1–6.
- `docs/feature-matrix.md` "Audio (M44 — Phases 1–6 complete)" section is the
  authoritative runtime-status table.
- The M44 row in `ROADMAP.md` (active milestones) carries the per-phase shipped
  detail and the pending-phase list.

**Confirmed-shipped surface (verify against the live API, do not assume)**:
- `AudioWorld` resource — graceful-degradation `Option<AudioManager<DefaultBackend>>`;
  public API: `new` / `default`, `is_active`, `manager_mut`, `active_sound_count`,
  `pending_oneshot_count`, `play_oneshot`, `play_music`, `stop_music`,
  `is_music_active`, `set_reverb_send_db`, `reverb_send_db`. **`play_music`
  gained a 4th parameter, `looping: bool` (#3775)** — signature is now
  `play_music(streaming_sound, volume, fade_in_secs, looping)`; re-verify the
  arity before citing it.
- Components: `AudioListener`, `AudioEmitter` (`sound` / `attenuation` / `volume`
  / `looping` / `unload_fade_ms`), `OneShotSound`. All `SparseSetStorage`.
- `Attenuation { min_distance, max_distance }`; `DEFAULT_UNLOAD_FADE_MS = 10.0`.
- Free fns: `audio_system`, `spawn_oneshot_at`, `load_sound_from_bytes`,
  `load_streaming_sound_from_bytes`, `load_streaming_sound_from_file`.
- `SoundCache` resource — path-keyed `Arc<StaticSoundData>` cache:
  `get` / `insert` / `get_or_load` / `len` / `is_empty` / `clear` /
  `bytes_estimate`.
- Re-exports: `Sound` (= `StaticSoundData`), `SoundSettings`, `Frame`.

**Future phases (NOT shipped — do not flag as missing unless scope says so)**:
Phase 3.5b FOOT records → per-material sound, MUSC/MUST decode + Oblivion's
RDMD enum (#3816), per-cell-acoustics reverb (current detector is binary
interior/exterior only), raycast occlusion attenuation. **REGN ambient
soundscapes are a split case, not simply "not shipped"**: the dispatch
mechanism (`dispatch_region_ambient_music`, crossfade, `looping`, stop-on-miss,
archive resolution) is live and tested (Dim 7), but the REGN→SOUN resolution
it depends on is confirmed structurally dead on every supported game —
Oblivion `RDMD` is a music-category enum (not a FormID), Skyrim `RDMO` targets
`MUSC`, FNV `RDSB`/`RDSI` target `MSET` (#3787/#3811) — so no authored REGN
data can ever reach it until #3816's MUSC/MUST/MSET decode work lands. Audit
the dispatch mechanism as shipped; audit the resolution gap as the open item.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3,5`). Default: all 7.
- `--depth shallow|deep`: `shallow` = check API contracts; `deep` = trace per-frame data flow + lifecycle. Default: `deep`.

## Extra Per-Finding Fields

- **Dimension**: Spatial Sub-Track Lifecycle | Listener Pose & Attenuation | SoundCache Growth | Streaming Music Lifecycle | Reverb Send & Routing | Manager Lifecycle & ECS/Cell Streaming | Gameplay Audio Wiring

## Phase 1: Setup

1. Parse `$ARGUMENTS` for `--focus`, `--depth`
2. `mkdir -p /tmp/audit/audio`
3. Fetch dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/audio/issues.json`
4. **Read the most recent `docs/audits/AUDIT_AUDIO_*.md` report** (sort by date —
   do not hardcode a filename here, it rots every cycle). Most of the original
   report's findings (#843–#859) are CLOSED and now live as regression guards in
   `crates/audio/src/tests.rs` + `byroredux/src/systems/audio.rs`. A re-flag of
   any of those is a regression claim, not a new finding; verify the guard is
   gone before reporting.
5. Read the `crates/audio/src/lib.rs` module docstring to confirm Phases 1–6 are
   shipped vs. inferred. If the docstring drifts from the user-visible API,
   that's a finding in itself — e.g. AUD-2026-07-02-01 (#1859, closed 2026-07-15
   by `37394005`) was the docstring citing a pre-Session-34 path
   (*byroredux/src/asset_provider.rs*) for `try_load_default_footstep` after it
   moved to `byroredux/src/asset_provider/texture.rs`. That specific drift is
   fixed — only re-flag if the path drifts again (check the live docstring
   against the live file location, don't assume the old finding still applies).

## Phase 2: Launch Dimension Agents

Dimensions are ordered by audio risk: track-handle leaks and spatial correctness
first, manager/ECS lifecycle and gameplay wiring last.

### Dimension 1: Spatial Sub-Track Lifecycle & Leaks (highest risk)
**Entry points**: `crates/audio/src/lib.rs` — `ActiveSound`, `dispatch_new_oneshots`,
`drain_pending_oneshots`, `prune_stopped_sounds`, `PendingOneShot`,
`AudioWorld::play_oneshot`, `spawn_oneshot_at`
**Checklist**:
- Two dispatch paths exist and MUST stay observably equivalent:
  - **Entity path** (`dispatch_new_oneshots`): `OneShotSound + AudioEmitter` on
    an entity → per-emitter `SpatialTrackHandle`, plays the sound, removes
    `OneShotSound` (keeps `AudioEmitter` so callers can query active state),
    `ActiveSound.entity = Some(..)`.
  - **Queue path** (`drain_pending_oneshots`): `play_oneshot(sound, position,
    attenuation, volume)` enqueues `PendingOneShot` → drained at frame start
    through the same spatial-sub-track shape, `ActiveSound.entity = None`.
- Both paths gate on `listener_id` (`listener.as_ref().map(|l| l.id())`) and
  early-return if absent — `add_spatial_sub_track` requires a listener id. Verify
  both helpers short-circuit when the listener hasn't been created yet (frame-1).
- **`ActiveSound._track: SpatialTrackHandle` is held for Drop side effect only.**
  Dropping it tears down playback while `handle` still ticks. Verify the field
  keeps its `_track` name (an underscore strip / accidental `drop()` is the leak
  class). The track must land in `active_sounds` before the helper returns, never
  in a local that drops at end of scope.
- `looping: bool` (Phase 4) → `loop_region(..)` applied at dispatch in the entity
  path only (queue one-shots never loop). Verify the queue path never sets it.
- **Drain cap**: `drain_pending_oneshots` warns at WARN when one tick drains > 32
  items (footstep-tempo gone wrong — a per-frame retrigger). Confirm the threshold.
- **Producer-side queue cap (regression guard, #852/#853)**: `play_oneshot` drops
  oldest at 256 entries via `VecDeque::pop_front` (O(1)), and returns early when
  `manager.is_none()` so the queue can't fill on headless. Re-verify the
  `VecDeque` (not `Vec::remove(0)`) and the up-front manager-`None` drop.
- **Drain ordering (regression guard, #851)**: the manager-active gate sits
  BEFORE the drain of `pending_oneshots`. A gate-after-drain reorder would
  silently lose drained items. Confirm the gate precedes the drain. **The
  drain mechanism itself changed (#3521)**: `drain_pending_oneshots` now uses
  `VecDeque::drain(..)` in place, not the prior `std::mem::take(&mut
  pending_oneshots)` — that swapped in a fresh zero-capacity `VecDeque` and
  dropped the capacity-holding original at scope end, paying an allocate+free
  pair on every tick that drained anything (same class as #932/#3059/#3257).
  Verify the field keeps its allocated capacity across a drain — a
  `mem::take` reappearing here is the regression.
- `Arc<StaticSoundData>` clone semantics: same `Arc` backs many emitters without
  re-decoding. Entity path uses `Arc::clone(&emitter.sound)`; volume is applied
  via `(*sound).clone().volume(db)` which reuses the underlying `Arc<[Frame]>`.
  Flag any path that deep-clones the PCM.
- Source-position resolution: queue path takes `position: Vec3` directly; entity
  path reads `GlobalTransform.translation`. Verify the entity path does NOT fall
  back to interpolation-state `Transform` (must use post-propagation pose).
- Volume→dB conversion (`20*log10(volume)`, clamped to `SILENCE_DB` = -60 dB for
  ≤0) is centralized in the `linear_volume_to_db` helper (AUD-2026-06-23-01 —
  previously inlined verbatim at three play sites). `drain_pending_oneshots`,
  `dispatch_new_oneshots`, and `play_music` all call the shared helper. Flag any
  site that reintroduces an inlined divergent copy instead of calling it.
**Output**: `/tmp/audit/audio/dim_1.md`

### Dimension 2: Listener Pose & Spatial Attenuation Correctness
**Entry points**: `crates/audio/src/lib.rs` — `sync_listener_pose`,
`AudioListener`, `Attenuation`
**Checklist**:
- Listener is created **lazily** on the first frame an `AudioListener`-tagged
  entity has a resolved `GlobalTransform` (`add_listener(pose.0, pose.1)`). Before
  that, `sync_listener_pose` early-returns at the first failed `iter.next()` /
  `query` / `get`. Verify no panic on the frame-1 cold-start race (entity exists,
  `GlobalTransform` not yet propagated).
- **Listener handle is sticky across entity churn (regression guard, #849)**: the
  `listener` field is created once and NEVER cleared, even when the marker entity
  despawns (third-person swap, fly-cam destroy, save-load). On respawn the
  existing handle's pose is updated, not a fresh `add_listener`. This is
  deliberate — kira's listener capacity is the default (8); a clear-on-missing /
  re-add-on-respawn loop would exhaust it. Flag any code that clears `listener`
  on missing entity.
- **Multi-listener diagnostic (regression guard, #843)**: when count > 1,
  `sync_listener_pose` warns ONCE (`multi_listener_warned` debounce) and uses the
  first iteration entity. Verify the warn is debounced (no per-frame spam during
  the brief two-listener window of a camera transition).
- `set_position` / `set_orientation` use `Tween::default()` every frame (smooth
  camera follow). Flag any switch to immediate (audible spatial jump on
  warp/fast-travel).
- **Orientation contract**: kira expects a `Quat`; `GlobalTransform.rotation` is
  passed straight through. Verify the Z-up(Gamebryo)→Y-up(renderer) conversion
  has already happened upstream (NIFAL `coord.rs`) so the quat handed to kira is
  in renderer space — a residual coordinate-frame mismatch is left/right channel
  inversion across the whole soundscape, subtle and lethal.
- **Attenuation curve**: `Attenuation` is `min_distance..=max_distance` linear
  falloff via `SpatialTrackBuilder::distances`, in **metres**. Default
  `{2.0, 30.0}`. Verify the range is passed as `RangeInclusive` (the exclusive
  `..` doesn't impl `Into<SpatialTrackDistances>`) and that `min <= max` is never
  violated by a caller (footsteps use `{0.5, 12.0}`).
- **Units (#3178)**: the engine's world space is Bethesda units
  (`BETHESDA_UNITS_PER_METER` = 70) and kira is metre-scaled. `bu_to_audio_space`
  is the single seam, applied to the listener pose and to **both**
  `add_spatial_sub_track` dispatch paths. Verify no position reaches kira
  unconverted — a missed site is inaudible past ~43 cm and produces no log line,
  no counter, and no failing test. Note this was invisible for eleven audit
  cycles because the only emitter was co-located with the listener, so distance
  was always exactly 0.
- `add_listener` failure is logged WARN and leaves `listener = None`; the next
  frame retries (lazy create is idempotent on `None`). Verify the retry — a
  transient init failure must not permanently break audio for the session.
**Output**: `/tmp/audit/audio/dim_2.md`

### Dimension 3: SoundCache Growth & Eviction
**Entry points**: `crates/audio/src/lib.rs` — `SoundCache` (`get`, `insert`,
`get_or_load`, `len`, `clear`, `bytes_estimate`), `load_sound_from_bytes`
**Checklist**:
- Path keys are interned lowercased at `insert` / `get` / `get_or_load` time
  (`to_ascii_lowercase`) to match the engine-wide case-insensitive asset
  convention. Verify every key path lowercases exactly once (no double-lowercase,
  no missed call site).
- **Eviction is manual via `clear()` — no automatic LRU.** That's documented as
  acceptable (#850): the cache is intended to dedupe by path, mirroring
  `AnimationClipRegistry`. The audit hazard is unbounded growth IF a real
  consumer lands. Verify `clear()` does NOT invalidate `Arc`s held by live
  `ActiveSound` entries (kira holds its own clone; the cache is not read through
  after the play call) — this is the safety property a future cell-unload
  `clear()` relies on.
- **Dormant-API reality (#859)**: the engine binary has ZERO `SoundCache` call
  sites today — `try_load_default_footstep` writes the decoded `Arc` straight
  into `FootstepConfig.default_sound`, bypassing the cache. Steady-state
  `len() == 0`. Confirm this (grep `SoundCache` across `byroredux/`); the
  "unbounded growth" concern is FUTURE-phase, not a present leak. The audit task
  is to pin that the decoupled API + tests survive so a producer can land without
  rewriting call sites — and to flag if anyone wires the first consumer WITHOUT
  also wiring eviction.
- `bytes_estimate` sums `frames.len() * size_of::<kira::Frame>()` (8 B/frame
  stereo) for telemetry. Verify it's wired to a `stats`-style console path (or
  flag as dead if not) so an unbounded-growth regression surfaces in telemetry,
  not at OOM.
- `get_or_load` invokes the loader only on a miss and returns `None` on
  decode-fail (logs WARN). Verify the loader isn't called on a hit (BSA-extract
  cost paid lazily).
**Output**: `/tmp/audit/audio/dim_3.md`

### Dimension 4: Streaming Music Lifecycle
**Entry points**: `crates/audio/src/lib.rs` — `play_music`, `stop_music`,
`is_music_active`, `load_streaming_sound_from_bytes`,
`load_streaming_sound_from_file`; field `music: Option<StreamingSoundHandle<FromFileError>>`
**Checklist**:
- **Single-slot music**: `play_music` fades out any existing `music` handle and
  replaces it (crossfade — both fades use the same `fade_in_secs` tween). Verify
  exactly ONE music slot. A regression adding a second slot would fail no test
  today — pin the single-slot invariant.
- Music routes through the **main track** via `mgr.play(...)`, NOT a spatial
  sub-track. Flag any path that puts music into the spatial scene (it would
  attenuate with listener distance — wrong for non-diegetic music).
- **Streaming, not buffered**: `load_streaming_sound_from_bytes` /
  `_from_file` use `StreamingSoundData::from_cursor` / `::from_file`, NOT
  `StaticSoundData` — the latter buffers full decompressed PCM (OOM on
  multi-minute tracks). Verify the streaming types.
- `stop_music` issues a fade-out then drops the handle (`music = None`); kira
  keeps the sound alive internally until the fade completes. Verify the fade
  duration is `fade_out_secs.max(0.0)` (instant stop = audible click is a
  regression) and that dropping the handle doesn't cut the fade short.
- `is_music_active` returns `false` once the handle reports `Stopped`. Verify it
  doesn't report active during the fade-out tail in a way that blocks a legit
  re-`play_music`.
- **CELL-level MUSC parse→play gap (re-scope — do NOT audit as a live path)**:
  cell-music FormIDs ARE parsed — `default_music` (ZNAM,
  `crates/plugin/src/esm/cell/wrld.rs` ~ZNAM arm; field
  `crates/plugin/src/esm/cell/mod.rs` `default_music: Option<u32>`) and
  `music_type_form` (XCMO, `wrld.rs` XCMO arm; field `cell/mod.rs`
  `music_type_form: Option<u32>`) — but every construction site
  (`scene/world_setup.rs`, `cell_loader/exterior.rs`, `cell_loader/lod_support.rs`,
  the test fixtures) still hardcodes `music_type_form: None` and nothing reads
  `default_music` either. Treat CELL-level MUSC routing as a FUTURE-phase gap.
  **This is a narrower claim than "no caller invokes `play_music`" — that is
  no longer true** (see below); the gap here is specifically the CELL
  ZNAM/XCMO path, which stays unwired independent of the REGN one.
- **`play_music` DOES now have a live caller — the REGN ambient path, a
  DIFFERENT FormID than CELL's MUSC (#3775, live in Dim 7)**:
  `dispatch_region_ambient_music` (`byroredux/src/asset_provider/audio.rs`)
  resolves `RegionAmbientRes::music_form` (REGN's `RDAT` Sound payload,
  RDSB/RDMO/RDMD depending on game) and calls `play_music` with the #3775
  `looping` flag. It is wired from both `cell_loader/load.rs` load paths and
  `scene::apply_cell_region_ambient`. Audit the dispatch mechanism itself as
  shipped (crossfade, looping, stop-on-miss/decode-fail, once-only unsupported
  log) — see Dim 7 — but its FormID resolution is confirmed structurally dead
  on every supported game (#3787/#3811: Oblivion RDMD is an enum not a
  FormID; Skyrim RDMO targets MUSC; FNV RDSB/RDSI target MSET), so it never
  actually plays anything in production until #3816 lands the MUSC/MUST/MSET
  decode. Do not report "no caller" for `play_music` any more — report the
  resolution gap instead, and cite #3816, not this dimension, as the tracking
  issue.
- Music handle field-drop: `music` drops before `manager` (Dim 6 field-drop invariant).
**Output**: `/tmp/audit/audio/dim_4.md`

### Dimension 5: Reverb Send & Routing (Phase 6)
**Entry points**: `crates/audio/src/lib.rs` — `AudioWorld::new` (send-track
creation), `set_reverb_send_db`, `reverb_send_db`, both `with_send` sites
(`drain_pending_oneshots` + `dispatch_new_oneshots`)
**Checklist**:
- One global send track is created in `AudioWorld::new` via
  `SendTrackBuilder::new().with_effect(ReverbBuilder::new()
  .feedback(REVERB_FEEDBACK).damping(REVERB_DAMPING)
  .stereo_width(REVERB_STEREO_WIDTH).mix(Mix::WET))` — named constants
  (0.85 / 0.6 / 1.0, `f64` to match kira's `Value<f64>` param type) as of
  #3780, replacing the bare literals. **No recorded provenance**: the values
  landed with the M44 Phase 6 commit (`e191d9f9`) with no rationale, and are
  NOT kira's own `ReverbBuilder::default()` (0.9/0.1/1.0 in kira 0.10.8), so
  this was a deliberate but undocumented choice — the constants exist so a
  future interior-acoustics tuning pass (#847) has a greppable baseline, not
  because they're validated against a reference. Do not invent or assume a
  rationale for these three numbers; flag the doc gap, don't paper over it.
  `reverb_send: Option<SendTrackHandle>` is `None` if the manager was
  inactive or `add_send_track` failed — verify the `None` path never
  cascades into a later `unwrap()`.
- **Per-new-track send opt-in**: each `SpatialTrackBuilder` opts into the send
  via `.with_send(reverb.id(), reverb_send_db)` at construction time, gated on
  `reverb_send_db.is_finite() && reverb_send_db > -60.0`. kira 0.10 has no
  retroactive send-level setter, so this is build-time only. Verify BOTH dispatch
  paths apply the gate identically (drift = inconsistent wetness between footsteps
  and entity emitters).
- **Default `reverb_send_db = f32::NEG_INFINITY`** ("reverb off"). Engine boots
  dry. Flag any finite default (every cell wet from frame 1).
- **Construction-time send level is a known limitation (#847), not a bug.**
  Already-playing sounds keep their build-time level; the change applies to
  NEW sounds. Audit findings claiming "level changes don't take effect
  immediately" have a stale premise — verify they actually need immediate
  application before escalating; the documented contract is "next-dispatch knob,
  not a live fader."
- `set_reverb_send_db(f32::NEG_INFINITY)` disables routing for new tracks (the
  `> -60.0` gate means `-inf` never reaches `with_send`). Verify the gate so the
  silent default is truly silent and never leaks a finite floor.
- **Cell-load reverb toggle lives in Dim 7** (`reverb_zone_system`, #846). Note
  here as a cross-dim pointer; keep the full detector audit in Dim 7.
- Reverb-send field-drop ordering: `reverb_send` drops between `music` and
  `listener` (Dim 6 field-drop invariant).
**Output**: `/tmp/audit/audio/dim_5.md`

### Dimension 6: Manager Lifecycle, ECS Lifecycle & Cell Streaming
**Entry points**: `crates/audio/src/lib.rs` — `AudioWorld::new` / `default` /
`is_active` / `manager_mut`, struct field order, `audio_system`,
`prune_stopped_sounds`; cross-cut: `byroredux/src/streaming.rs`,
`byroredux/src/cell_loader/load.rs`, `byroredux/src/cell_loader/unload.rs`
**Checklist**:
- **Graceful degradation**: `AudioManager::new` failure leaves `manager = None`,
  no panic. Booting headless / CI / broken-driver MUST succeed. Every public API
  gates on `manager.is_some()` (or the `audio_system` `is_active()` early-return)
  with no `unwrap()` on the `Option<AudioManager>`. Regression guard:
  `audio_world_constructs_without_panic_on_any_environment` (`tests.rs`).
- **Capacities (regression guard, #842)**: `AudioWorld::new` sets
  `sub_track_capacity = SUB_TRACK_CAPACITY (512)` and `send_track_capacity =
  SEND_TRACK_CAPACITY (32)`, both above kira's defaults — populated interiors
  (~400 emitters once FOOT/REGN land) blow past the 128 default with silent-drop
  on `ResourceLimitReached`. Verify the consts and that `new()` actually applies
  them. Guard: `manager_capacities_exceed_kira_defaults`.
- **Field-drop order (single source of truth — collapse Dims 1/4/5 here)**: Rust
  drops struct fields in declaration order. `AudioWorld` MUST declare in
  dependency order so handles drop before the manager: `active_sounds`
  (owns `SpatialTrackHandle`s) → `pending_oneshots` → `music` → `reverb_send` →
  `reverb_send_db` → `listener` → `manager`. A "readability" reorder that moves
  `manager` up would make kira assert-fail in Drop. Verify the declaration order.
- `AudioWorld` / `SoundCache` are `Resource` (interior mutability via `&self`
  resource access). Flag any `&mut World` requirement that snuck back in. `new()`
  is boot-only — flag any call on cell transition / window resize (re-acquiring
  the OS audio device is expensive and may fail).
- **`audio_system` stage**: registered `add_exclusive(Stage::Late,
  byroredux_audio::audio_system)` in `byroredux/src/boot.rs` (after transform
  propagation produces final poses). Running before propagation reads stale
  `GlobalTransform`. Verify the stage hasn't moved. `audio_system` body order is:
  `sync_listener_pose` → `drain_pending_oneshots` → `dispatch_new_oneshots` →
  `prune_stopped_sounds`.
- **Cross-stage producer→consumer (footstep → audio)**: `footstep_system` is
  exclusive at `Stage::PostUpdate` and ENQUEUES `play_oneshot`; `audio_system`
  DRAINS at `Stage::Late`, the SAME frame. Verify `PostUpdate` precedes `Late` so
  a footstep is heard the same tick (never lags / drops on a stage reorder). Flag
  any move of either system out of its stage.
- **`OneShotSound` lifecycle**: removed by `dispatch_new_oneshots` after dispatch
  (single-frame). Flag any path retaining the marker across frames (re-triggers
  every frame).
- **Despawn truncation (regression guard, #845/#858/SAFE-23)**: when an entity
  loses its `AudioEmitter` (cell unload / explicit remove), `prune_stopped_sounds`
  issues a tweened `stop()` using `ActiveSound.unload_fade_ms` (captured at
  dispatch from `AudioEmitter.unload_fade_ms`, default `DEFAULT_UNLOAD_FADE_MS =
  10.0` ms). Applies to looping AND non-looping (#858 extended it from
  looping-only). `stop_issued` debounces the re-walk during the async fade window
  (#844). Queue-driven sounds (`entity == None`) are exempt — they run to natural
  termination. Verify: (a) entity-presence check via `AudioEmitter` query, (b)
  `stop_issued` set after the stop, (c) the `retain` drops only on
  `PlaybackState::Stopped`, (d) `AudioEmitter` removed on completion. Guards:
  `looping_emitter_survives_natural_duration_and_stops_on_emitter_remove`,
  `non_looping_emitter_stops_on_emitter_remove_regression_858`.
- Listener-entity despawn: `sync_listener_pose` early-returns; the handle stays
  (see Dim 2 sticky-listener guard) and drops with `AudioWorld` at shutdown — a
  despawn/respawn cycle must not leak listener handles.
- **Cross-cell music continuity now has a live implementation to audit, not
  just a future contract (#3775 — see Dim 4/7).**
  `dispatch_region_ambient_music` gates on `music_form` FormID equality
  already: both call sites (`cell_loader/load.rs`'s two load paths,
  `scene::apply_cell_region_ambient`) compare against
  `RegionAmbientRes`'s prior value and only redispatch on an actual change —
  re-decoding/re-streaming the same track on every connected-cell crossing
  sharing one tagging region would otherwise restart it audibly. Verify that
  gate still holds. The still-open future contract is narrower: a CELL-level
  MUSC (ZNAM/XCMO) caller, whenever one lands, must implement the same
  equality gate independently (it targets a different FormID field than
  REGN's `music_form`).
**Output**: `/tmp/audit/audio/dim_6.md`

### Dimension 7: Gameplay Audio Wiring (Engine-Side Consumers)
**Entry points**: `byroredux/src/systems/audio.rs` — `footstep_system`,
`reverb_zone_system`; `byroredux/src/components.rs` — `FootstepEmitter`,
`FootstepConfig`, `FootstepScratch`; `byroredux/src/scene.rs` (camera opt-in,
`apply_cell_region_ambient`); `byroredux/src/asset_provider/texture.rs` —
`try_load_default_footstep`; `byroredux/src/asset_provider/audio.rs` —
`dispatch_region_ambient_music`, `stop_region_ambient_music`,
`SoundArchiveProvider`, `resolve_sound_path`, `sound_loops`,
`sound_archive_path`, `build_sound_archive_provider`; wired from
`byroredux/src/cell_loader/load.rs`
**Why this dimension**: the M44 CRATE (Dims 1–6) is the producer of the
`play_oneshot` / `play_music` / reverb API; the consumers that DRIVE it live
outside the crate. `footstep_system` is the ONLY live `play_oneshot` caller;
`reverb_zone_system` is the ONLY `set_reverb_send_db` caller;
`dispatch_region_ambient_music` is the ONLY `play_music`/`stop_music` caller
(#3775). None is covered by the crate dimensions.
**Checklist**:
- **Stride accumulation**: `footstep_system` accumulates XZ-plane (horizontal
  only — Y is not a step) movement against `FootstepEmitter.stride_threshold`,
  fires one `play_oneshot` per crossing, and RESETS `accumulated_stride = 0.0` on
  fire (a subtract-remainder would multiply footsteps on a large teleport).
  Guard: `single_large_jump_fires_one_footstep_only`. **`stride_threshold` is
  authored in Bethesda units, not metres (#3520)** — `0.75 *
  BETHESDA_UNITS_PER_METER` (52.5), compared directly against
  `GlobalTransform.translation` deltas (also BU); no conversion at the
  compare site, `bu_to_audio_space` stays the subsystem's only unit seam
  (#3178). Before the fix the field was `1.5` with a "~1.5m" docstring —
  effectively 1.5 BU (2.1 cm), so the `if`-not-`while` fire branch fired
  once per frame while moving (60 Hz vs. a ~1.8 Hz human cadence). Guards:
  `stride_threshold_is_bethesda_units_not_metres`,
  `walking_for_one_second_fires_a_walking_cadence_not_one_per_frame`. A
  reintroduced metre-scale literal is the regression.
- **First-tick seed (#848)**: on the first tick (`!fs.initialised`), seed
  `last_position = pos`, set `initialised = true`, and `continue` WITHOUT
  accumulating — else the cold-start origin→spawn delta fires a phantom step.
  Guard: `first_tick_seeds_last_position_without_firing`.
- **`FootstepScratch` Vec reuse (#932)**: the per-tick trigger buffer lives in the
  `FootstepScratch` resource (`triggers: Vec<Vec3>`, capacity 32) and is
  `clear()`-reused + `std::mem::take`-drained, NOT freshly allocated. Verify the
  buffer's heap allocation is restored to the resource (`try_resource_mut::<
  FootstepScratch>()` re-acquire at fn end) on BOTH the success path AND the
  `AudioWorld`-absent bail path — dropping the moved-out `Vec` strands the capacity.
- **Lock-drop ordering**: Phase 1 holds `GlobalTransform` (read) + `FootstepEmitter`
  (write) concurrently, then RELEASES both before Phase 2 touches `AudioWorld`.
  The `FootstepScratch` mut-lock is `drop()`-ed BEFORE `AudioWorld` is acquired —
  holding two resource-mut locks at once would force the TypeId-sorted
  acquisition contract. Verify no component query lock is held across
  `play_oneshot`.
- **Footstep attenuation**: each footstep uses `Attenuation { min_distance: 0.5,
  max_distance: 12.0 }` — **metres**, tighter than the `{2.0, 30.0}` default
  because footsteps drop off fast. Flag any widening (distant NPC footsteps
  audible across a whole interior), and equally flag any *pre-scaling* into
  Bethesda units: world BU crosses into kira's metre space exactly once, in
  `bu_to_audio_space` at the audio boundary (#3178). A producer multiplying its
  own constants by 70 is the regression, not the fix.
- **Silent no-op contracts**: `footstep_system` returns early and silently when
  `FootstepConfig` is absent, `default_sound` is `None`, `FootstepScratch` is
  absent, the emitter/transform queries fail, or `AudioWorld` is absent. Verify
  NONE panic or log per-frame spam. Guards: `no_default_sound_is_silent_noop`,
  `standing_still_never_fires`.
- **Camera opt-in**: the fly-cam entity gets `FootstepEmitter` in
  `byroredux/src/scene.rs`. Verify the opt-in is component-driven (no hardcoded
  camera-entity assumption inside the system) so NPCs can carry `FootstepEmitter`
  under future REGN/AI work.
- **`try_load_default_footstep`** (`byroredux/src/asset_provider/texture.rs`): populates
  `FootstepConfig.default_sound` from `--sounds-bsa` (canonical FNV dirt-walk
  WAV), bypassing `SoundCache`. Verify it no-ops cleanly when the BSA / arg is
  absent (engine boots with footsteps off).
- **`reverb_zone_system` (#846)** — the interior-reverb detector. Re-confirm:
  `INTERIOR_REVERB_SEND_DB = -12.0` / `EXTERIOR_REVERB_SEND_DB = f32::NEG_INFINITY`
  (both `const` local to the fn); the transition is **bit-equality gated**
  (`reverb_send_db().to_bits() == target_db.to_bits()` short-circuits, so
  `NEG_INFINITY → NEG_INFINITY` never re-touches the field); it no-ops safely when
  `CellLightingRes` is absent (boot pre-cell-load) AND when `AudioWorld` is absent
  (headless); and it is registered `add_exclusive(Stage::Late, ...)` in
  `byroredux/src/boot.rs` BEFORE `audio_system` in the same stage — so a track
  built this tick picks up this tick's send level, not last tick's. Guards:
  `interior_cell_sets_subtle_reverb_send`,
  `interior_to_exterior_transition_resets_send_to_dry`,
  `no_cell_lighting_resource_is_safe_noop`, `no_audio_world_is_safe_noop`.
- **`dispatch_region_ambient_music` (#3775/#3787/#3811)** — the REGN ambient
  track dispatch, the ONLY live `play_music`/`stop_music` caller. Only
  redispatches when `music_form` actually CHANGED from the resource's prior
  value (both call sites — `cell_loader/load.rs`'s two load paths and
  `scene::apply_cell_region_ambient` — compare before calling); an
  unconditional redispatch on every cell load/crossing would restart, with an
  audible crossfade, the same track whenever two connected cells share a
  tagging region. Verify it stops outstanding playback (not leaves it
  running) on every failure layer — no `SoundArchiveProvider`, no
  `--sounds-bsa` supplied, unresolved FormID, missing archive entry, decode
  failure, and `music_form: None` — because whatever was audible belonged to
  the *previous* directive. The `music_form`-authored-but-unresolved case
  logs once via `std::sync::Once` (not per-region-transition) distinguishing
  "no archive supplied" from "authored but this engine build can't resolve
  its target type" — verify the log fires once per process, not once per
  cell. `looping` threads through from `sound_loops` (SOUN's Loop bit) into
  `play_music`'s 4th parameter. Per the Dim 4 cross-reference: this is
  presently unreachable in production because `music_form` never resolves as
  a SOUN on any supported game — audit the mechanism, not a live flow.
**Output**: `/tmp/audit/audio/dim_7.md`

## Phase 3: Merge

1. Read all `/tmp/audit/audio/dim_*.md` files
2. Combine into `docs/audits/AUDIT_AUDIO_<TODAY>.md` with structure:
   - **Executive Summary** — Crate Phases 1–6 + engine consumers shipped
     (footstep gameplay loop via `footstep_system`, per-cell reverb via
     `reverb_zone_system`, REGN ambient dispatch via
     `dispatch_region_ambient_music`) vs pending (3.5b FOOT, MUSC/MUST/MSET
     decode, occlusion). Note the REGN dispatch mechanism is shipped and
     tested but its FormID resolution is structurally dead on every game
     pending #3816, and that CELL-level MUSC (ZNAM/XCMO) is a separate,
     still fully-unwired gap. Findings count by severity. Headless-mode boot
     status (MUST be PASS).
     Delta vs the most recent prior `docs/audits/AUDIT_AUDIO_*.md` report (which of
     #843–#859 are now regression-guarded).
   - **Lifecycle Invariant Matrix** — Field-drop order × verified/drifted;
     per-handle owner; sticky-listener; despawn-truncation guards.
   - **Findings** — Grouped by severity (CRITICAL first), deduplicated.
   - **Future-Phase Readiness** — Which invariants this audit pinned for the next
     phase (FOOT/3.5b material sounds, MUSC/MUST/MSET decode unblocking REGN
     ambient + CELL-level music, occlusion).
3. Remove cross-dimension duplicates: field-drop order is owned by Dim 6 (pointers
   from Dims 1/4/5); `reverb_zone_system` full audit lives in Dim 7 (pointer from
   Dim 5).

## Phase 4: Cleanup

1. `rm -rf /tmp/audit/audio`
2. Inform user the report is ready
3. Suggest: `/audit-publish docs/audits/AUDIT_AUDIO_<TODAY>.md`
   (domain label: `audio`; add the matching `game:*` when the finding is specific to
   one title's sound data).
