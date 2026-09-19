---
description: "Deep audit of the M44 audio subsystem — kira backend, spatial sub-tracks, listener/unit seam, reverb send, underwater filter, streaming music — plus its engine callers (footsteps, water audio, reverb zones, REGN ambient music)"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Audio Subsystem Audit (M44)

Read `.claude/commands/_audit-common.md` (delta-first scoping, dedup, finding format) and `_audit-severity.md` for shared protocol.

Audits `crates/audio/` (kira `0.10`; `src/lib.rs` + `src/tests.rs`) and the engine systems that are the ONLY callers of its API. Orchestrator; one Task agent per dimension (max 3 concurrent). Verify the shipped surface against the `lib.rs` module docstring and `docs/feature-matrix.md` (Audio); if `lib.rs` splits, re-derive each dimension's paths first.

## Scope

- **Crate**: `AudioWorld` (`Option<AudioManager>` resource; `headless()` is the no-device constructor), `AudioListener` / `AudioEmitter` / `OneShotSound` components, `Attenuation`, `audio_system`, `SoundCache`, `load_*sound*`.
- **Engine side**: `byroredux/src/systems/audio.rs` (`footstep_system`, `reverb_zone_system`, `water_audio_system`), the footstep / water-audio / `RegionAmbientRes` resources in `byroredux/src/components.rs`, `byroredux/src/asset_provider/{audio,texture}.rs` (`dispatch_region_ambient_music`, `SoundArchiveProvider`, `sound_is_folder`, `try_load_default_footstep`), wired from `cell_loader/load.rs` and `scene::apply_cell_region_ambient`.
- **Handoffs**: water *state* (`SubmersionState`, splash/ripple production) is `/audit-physics` Dim 5; stage/lock shape is `/audit-ecs` / `/audit-concurrency` (report audio-specific ordering only).

**Not shipped as of 2026-09-19 (re-verify against `docs/feature-matrix.md`; audit the mechanism that will carry them)**: FOOT records → per-material footstep sound (3.5b), CELL-level MUSC routing (`default_music`/`music_type_form` are parsed; every construction site still hardcodes `music_type_form: None`), per-cell acoustics beyond binary interior/exterior reverb, occlusion, REGN `incidental`/ambient-loop selection (#3301 open).
**Known-open** (2026-09-19): #3816 (MUSC/MUST/MSET/RDMD decode: REGN `music_form` never resolves as a `SOUN` on any supported game, so the dispatch *mechanism* is live and tested but plays nothing in production; audit the mechanism, cite #3816 for the gap), #3086 (entity-path spatial position frozen at dispatch, `AudioEmitter` docs promise per-frame), #4146 (reverb-zone ordering comment drift; the comment now names the parallel-vs-exclusive mechanism, check the issue state before re-filing).

## Parameters

`--focus <dims>` (default all 5) · `--depth shallow|deep` (`deep` traces per-frame data flow + lifecycle).
**Extra field**: **Dimension**: Spatial Dispatch & Listener | Send Graph (Reverb + Underwater) | Music & SoundCache | Manager & ECS Lifecycle | Engine Consumers.

## Phase 1: Setup

1. `mkdir -p /tmp/audit/audio`; dedup per `_audit-common.md`; read the latest `docs/audits/AUDIT_AUDIO_*.md` (date D = delta baseline). A re-flag of a closed finding is a regression claim: check its guard is gone before reporting.
2. `cargo test -p byroredux-audio` and `cargo test -p byroredux audio`; record counts. **Four lifecycle guards are `#[ignore]`d** (need an audio device + FNV data): `looping_emitter_survives_natural_duration_and_stops_on_emitter_remove`, `non_looping_emitter_stops_on_emitter_remove_regression_858`, `play_music_looping_survives_track_end`, `play_music_drives_streaming_playback_on_real_ogg`. Despawn truncation and music looping are unguarded in the default lane: run `-- --ignored` or say so.

## Phase 2: Dimensions

### Dim 1: Spatial Dispatch, Listener & the Unit Seam
**Paths**: `crates/audio/src/lib.rs` (`dispatch_new_oneshots`, `drain_pending_oneshots`, `prune_stopped_sounds`, `sync_listener_pose`, `bu_to_audio_space`, `linear_volume_to_db`, `ActiveSound`)
**First step**: `git log --since=D -- crates/audio/src/lib.rs`
**Guards** (default lane): `tests.rs::{every_kira_position_site_goes_through_the_unit_seam, emitter_at_max_distance_in_bu_lands_on_max_distance_in_audio_space, drain_pending_oneshots_drains_in_place_instead_of_stranding_capacity, play_oneshot_queue_caps_at_max_pending_when_active, oneshot_marker_is_consumed_on_both_dispatch_failure_arms}`.
- Two dispatch paths stay observably equivalent: entity path (`OneShotSound`+`AudioEmitter` → per-emitter `SpatialTrackHandle`, marker removed, `AudioEmitter` kept) and queue path (`play_oneshot` → `PendingOneShot`, entity `None`, never loops). Both early-return without a listener id.
- `ActiveSound::_track` is held for its Drop side effect: the `_` name is load-bearing and the track must land in `active_sounds` before the helper returns. `Arc<StaticSoundData>` is shared, never deep-cloned; volume applies via `(*sound).clone().volume(db)`.
- Producer queue caps at 256 with O(1) `pop_front` and drops up front when the manager is `None`; the manager-active gate precedes the drain; a drain of more than 32 warns once per tick (footstep tempo gone wrong). `linear_volume_to_db` is the single dB conversion (floor `SILENCE_DB` -60).
- Entity path reads post-propagation `GlobalTransform`, not `Transform`. World is Bethesda units (70/m), kira is metres: `bu_to_audio_space` is the only seam, applied to the listener pose and both dispatch paths; `Attenuation` is authored in metres (`min <= max`, `RangeInclusive`) and callers must never pre-scale by 70. A missed site is silent (inaudible past ~43 cm, no log).
- Listener: created lazily on the first frame an `AudioListener` has a resolved `GlobalTransform`; the handle is **sticky** (never cleared on despawn: kira listener capacity is 8, a clear/re-add loop exhausts it); a failed `add_listener` retries next frame; multi-listener warns once and uses the first. Pose uses `Tween::default()` (smooth follow); orientation is already renderer-space (no second Z-up conversion).
**Output**: `/tmp/audit/audio/dim_1.md`

### Dim 2: Send Graph: Reverb Send & Underwater Filter
**Paths**: `crates/audio/src/lib.rs` (`AudioWorld::new`, `apply_reverb_send`, `reverb_send_gate_open`, `set_reverb_send_db`, `apply_underwater_filter`, `update_underwater_filters`, `set_underwater`)
**First step**: `git log --since=D -- crates/audio/src/lib.rs` (send/filter regions)
**Guards**: `tests.rs::{reverb_send_defaults_to_silent, reverb_send_gate_matches_silence_db_boundary, above_water_filter_state_is_a_dry_bypass, above_water_cutoff_never_reaches_the_kira_clamp_ceiling, both_dispatch_paths_build_the_filter_through_one_call_site}`.
- One global reverb send is created in `AudioWorld::new` from `REVERB_FEEDBACK`/`REVERB_DAMPING`/`REVERB_STEREO_WIDTH` (0.85/0.6/1.0). They have **no recorded provenance** and are not kira's defaults: flag the doc gap, never invent a rationale. `reverb_send: None` never cascades into an `unwrap()`.
- Send opt-in is build-time only (`with_send` at track construction, gated `is_finite() && > -60 dB`) and both paths use the shared helper. Default level `NEG_INFINITY` (dry). Playing sounds keep their construction-time level: the documented "next-dispatch knob" contract, not a bug.
- Every spatial sub-track carries a low-pass built by ONE shared `apply_underwater_filter`; `update_underwater_filters` retargets live tracks when the listener's submersion changes. Above water is a genuine bypass via `Mix::DRY` (a default `FilterBuilder` is fully wet and colours the signal); the parked 20 kHz cutoff stays below kira's Nyquist clamp (beyond it the filter degenerates rather than turning transparent). `set_underwater` is written each frame by `water_audio_system` (Dim 5), before `audio_system`.
**Output**: `/tmp/audit/audio/dim_2.md`

### Dim 3: Streaming Music & SoundCache
**Paths**: `crates/audio/src/lib.rs` (`play_music`, `stop_music`, `is_music_active`, `load_streaming_sound_from_*`, `SoundCache`)
**First step**: `git log --since=D -- crates/audio/src/lib.rs`
**Guards**: `tests.rs::{play_music_no_op_when_inactive, sound_cache_hits_are_case_insensitive_and_share_arc, sound_cache_get_or_load_invokes_loader_only_on_miss, sound_cache_clear_drops_entries_and_bytes_estimate_tracks_pcm_size}`.
- Single music slot: `play_music(streaming_sound, volume, fade_in_secs, looping)` (4 parameters, `looping` since #3775; re-verify the arity before citing) crossfades any existing handle. Music routes through the **main track**, never a spatial sub-track. It uses `StreamingSoundData` (`from_cursor`/`from_file`), never `StaticSoundData` (full PCM decode = OOM on long tracks).
- `stop_music` fades for `fade_out_secs.max(0.0)` then drops the handle (an instant stop clicks); `is_music_active` goes false the moment `stop_music` returns (the slot is cleared while kira renders the fade tail): documented, so a caller must not read it as "audibly occupied".
- `SoundCache` keys lowercase once per entry point; `clear()` never invalidates `Arc`s held by live `ActiveSound`s. The engine installs **no** `SoundCache` (`byroredux/src/ownership_sample.rs` samples `len()` from a resource nothing inserts; the footstep/splash loaders write their `Arc` straight into `FootstepConfig`/`WaterAudioConfig`), so steady-state `len() == 0` and unbounded growth is future-only: flag a first consumer that lands without eviction, and check `bytes_estimate` (unsampled today) reaches telemetry then.
**Output**: `/tmp/audit/audio/dim_3.md`

### Dim 4: Manager, ECS Lifecycle & Schedule
**Paths**: `crates/audio/src/lib.rs` (struct, `AudioWorld::new`/`headless`, `audio_system`, `prune_stopped_sounds`), `byroredux/src/boot/schedule/late.rs`, `byroredux/src/cell_loader/{load,unload}.rs`
**First step**: `git log --since=D -- crates/audio/src/lib.rs byroredux/src/boot/schedule/late.rs`
**Guards**: `tests.rs::{audio_world_constructs_without_panic_on_any_environment, explicit_headless_world_discards_playback_without_retaining_sound, manager_capacities_exceed_kira_defaults}`; `scheduler_access_tests.rs::{footstep_runs_after_camera_follow_in_late, reverb_zone_is_parallel_and_audio_is_exclusive_in_late}`.
- Graceful degradation: `AudioManager::new` failure leaves `manager = None`; every public API gates on it with no `unwrap()`; headless/CI boot must succeed (report as PASS/FAIL in the summary). `AudioWorld::headless()` never contacts a device; tests use it, not the device-dependent `default()`.
- `SUB_TRACK_CAPACITY` (512) and `SEND_TRACK_CAPACITY` (32) exceed kira's defaults and are applied in `new()`.
- **Field-drop order** (single source of truth): `active_sounds` → `pending_oneshots` → `music` → `reverb_send` → `reverb_send_db` → `listener` → `manager`, then the two trailing flags. A reorder that moves `manager` up asserts in kira's Drop. `AudioWorld::new()` is boot-only, never on cell transition or resize.
- **Schedule** (all `Stage::Late`): `audio_system` is an *exclusive* registered after `footstep_system` (exclusive, pinned to Late by #4185's test) and `water_audio_system` (exclusive, after `submersion_system`); `reverb_zone_system` is a *parallel* registration, and the parallel batch finishing before any exclusive (not registration order) is what sets the send level before new tracks are built. `audio_system` body order: `sync_listener_pose` → `update_underwater_filters` → `drain_pending_oneshots` → `dispatch_new_oneshots` → `prune_stopped_sounds`.
- Despawn truncation: an entity that loses `AudioEmitter` gets a tweened `stop()` over `ActiveSound.unload_fade_ms` (10 ms default, looping and not); `stop_issued` debounces; retain drops only on `Stopped`; queue sounds (`entity == None`) run to natural end. Guards are `#[ignore]`d (Phase 1): trace the code.
- Cross-cell continuity: `dispatch_region_ambient_music` runs only when `music_form` changed (both `cell_loader/load.rs` paths and `scene::apply_cell_region_ambient` compare against the prior `RegionAmbientRes`); an unconditional redispatch audibly restarts the track on every connected-cell crossing. A future CELL-level MUSC caller needs its own gate.
**Output**: `/tmp/audit/audio/dim_4.md`

### Dim 5: Engine-Side Consumers
**Paths**: `byroredux/src/systems/audio.rs`, `byroredux/src/components.rs` (footstep/water-audio/region resources), `byroredux/src/asset_provider/{audio,texture}.rs`, `byroredux/src/scene.rs`
**First step**: `git log --since=D -- byroredux/src/systems/audio.rs byroredux/src/asset_provider/audio.rs`
**Guards** (`systems/audio.rs` tests): `first_tick_seeds_last_position_without_firing`, `stride_threshold_is_bethesda_units_not_metres`, `walking_for_one_second_fires_a_walking_cadence_not_one_per_frame`, `single_large_jump_fires_one_footstep_only`, `water_splash_event_reaches_audio_dispatcher`, `interior_to_exterior_transition_resets_send_to_dry`, plus the `no_*_is_safe_noop` pair. Highest-yield dimension in the 2026-08/09 reports.
- **Callers**: `footstep_system` and `water_audio_system` are the live `play_oneshot` callers, `reverb_zone_system` the only `set_reverb_send_db` caller, `dispatch_region_ambient_music` the only `play_music`/`stop_music` caller. Re-grep for any new caller (`git grep -n 'play_oneshot\|play_music\|set_underwater' -- byroredux/src`) and audit it.
- **Footsteps**: XZ-only stride accumulation against `FootstepEmitter.stride_threshold`, authored in **Bethesda units** (`0.75 * BETHESDA_UNITS_PER_METER`), compared to `GlobalTransform` deltas with no conversion at the compare site; reset to 0 on fire (subtract-remainder multiplies footsteps on a teleport); first tick seeds `last_position` and fires nothing. Attenuation `{0.5, 12.0}` in metres (no pre-scaling by 70, no widening). The `FootstepScratch` `Vec` (capacity 32) is restored on the success and the `AudioWorld`-absent path; no component lock is held across `play_oneshot` and the scratch lock drops before `AudioWorld` is taken. Every missing resource is a silent no-op (no per-frame log). Opt-in is component-driven, not tied to the camera entity.
- **Water audio**: `water_audio_system` writes `AudioWorld::set_underwater` from the active camera's `SubmersionState.head_submerged` *every* frame (before the config early-return), then plays `SplashEvent` one-shots (range 1-24 m) and, when no splash fired, one cooldown-limited `RippleEvent` (per-surface cooldown, 0.45 damping). The range and the 0.45 factor are uncited (no authored SOUN range): note as unsourced, never invent one. The system's doc comment says ripples "remain presentation data" while the body plays them: reconcile. `WaterAudioConfig.splash_sound` is `None` without an archive (valid headless state); `try_load_default_footstep` and `try_load_default_water_splash` (`asset_provider/texture.rs`) no-op cleanly when the arg or archive is absent.
- **`reverb_zone_system`**: `-12 dB` interior / `NEG_INFINITY` exterior; the transition is bit-equality gated (`to_bits`), no-ops without `CellLightingRes` (pre-load) or `AudioWorld` (headless).
- **`dispatch_region_ambient_music`**: stops outstanding playback on every failure layer (no provider, no `--sounds-bsa`, unresolved FormID, folder FNAM, archive miss, decode failure, `music_form: None`) because what is audible belonged to the previous directive; the authored-but-unresolved log fires once per process (`std::sync::Once`); `looping` threads from `sound_loops` (SOUN Loop bit) into `play_music`. `SounRecord::sound_path` is a file **or a trailing-separator folder** (about half of FNV's FNAM-bearing records): `sound_is_folder` fails closed before any archive lookup, and any future `resolve_sound_path` consumer must gate on it (variant selection is an unmade policy, so no "extract any file under this folder" fallback exists).
- **Archive precedence**: `SoundArchiveProvider::extract` iterates `.rev()` (last-listed wins, as mesh/texture/material; FNV's `Fallout - Sound.bsa` + `Update.bsa` overlap with differing bytes): forward iteration is the regression. `ScriptProvider` is the deliberate first-wins exception.
**Output**: `/tmp/audit/audio/dim_5.md`

## Phase 3: Merge

Combine `/tmp/audit/audio/dim_*.md` into `docs/audits/AUDIT_AUDIO_<TODAY>.md` (header per `_audit-common.md` Report finalization): Executive Summary (severity counts, headless-boot PASS/FAIL), **Lifecycle Invariant Matrix** (field-drop order, sticky listener, despawn truncation, schedule order: verified or drifted), Findings (deduplicated), **Future-Phase Readiness** (FOOT, MUSC/MUST/MSET, occlusion). Then `rm -rf /tmp/audit/audio`; suggest `/audit-publish docs/audits/AUDIT_AUDIO_<TODAY>.md` (labels `audio`, plus `game:*` when one title's sound data is the cause).
