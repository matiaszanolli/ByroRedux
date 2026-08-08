# Audio Subsystem Audit (M44) — 2026-08-07

- **Command**: `/audit-audio` → all 7 dimensions, `--depth deep` (one leg of a
  `comprehensive` audit-suite sweep)
- **Branch**: main · **HEAD**: `79bfc76e` (2026-08-07)
- **kira**: pinned `0.10` (workspace `Cargo.toml`, unchanged)
- **Method**: Full independent re-verification via 7 dimension sub-agents (one
  per checklist area), each re-reading `crates/audio/src/lib.rs` in full
  (1293 lines) and `crates/audio/src/tests.rs`, plus the engine-side
  consumers (`byroredux/src/systems/audio.rs`, `byroredux/src/scene.rs`,
  `byroredux/src/asset_provider/texture.rs`, `byroredux/src/boot.rs`). Each
  agent independently re-derived every invariant from live line numbers
  rather than trusting the prior report's table. Dedup baseline: `gh issue
  list` (JSON snapshot, `/tmp/audit/audio/issues.json`, 94 entries) + the full
  prior-report chain back to `_05-05`.
- **Session note**: Dimensions 2 (Listener Pose & Attenuation) and 3
  (SoundCache Growth) were run to completion in an earlier attempt at this
  same sweep before an unrelated session interruption; their results
  (`/tmp/audit/audio/dim_2.md`, `dim_3.md`, both PASS/no-findings, methodology
  matching this cycle's bar) were read, verified complete and current against
  live HEAD, and incorporated below without re-running. Dimensions 1, 4, 5, 6,
  7 were run fresh in this session.

---

## Delta Analysis (`1ae86f62..HEAD`, scope-relevant files only)

| File | Change | Audio-relevant? |
|---|---|---|
| `crates/audio/src/lib.rs` | **none** | — byte-identical to the 2026-08-03 audit's HEAD (last functional commit `71068d05`, 2026-07-24, predates that audit) |
| `crates/audio/src/tests.rs` | none | — |
| `byroredux/src/systems/audio.rs` | none | — |
| `byroredux/src/components.rs` | none in audio structs | — |
| `byroredux/src/scene.rs` | none touching the `AudioListener`/`FootstepEmitter` camera opt-in | — |
| `byroredux/src/boot.rs` | none touching audio stage registration | — |
| `byroredux/src/asset_provider/texture.rs` | none | — |

**Net: zero in-scope changes since the last audit cycle.** The interval's
commits (`716b7ee9` physics collision compat, `8ee151e0` collision authoring
summary, `7a851ab9` day-night cycle, `0775df28` quest observability) are all
outside the audio crate and its two live engine consumers. This is the second
consecutive fully-quiet interval for the audio subsystem.

---

## Executive Summary

**7 dimensions run, 1 NEW finding (LOW), 1 Existing cross-audit finding
re-confirmed present (MEDIUM, tracked as #2394 — not re-filed).** Both
findings sit in Dimension 1/6's shared entry point
(`dispatch_new_oneshots`/reverb-send gate); no other dimension produced any
finding.

- **Headless-mode boot**: PASS — `audio_world_constructs_without_panic_on_any_environment`
  green; graceful-degradation `Option<AudioManager>` path confirmed, zero
  `.unwrap()` calls anywhere in `crates/audio/src/lib.rs` on the manager
  option.
- **Guards re-run live on HEAD `79bfc76e`**:

| Suite | Result |
|---|---|
| `cargo test -p byroredux-audio` | **19 passed, 0 failed, 6 ignored** (ignored = real-audio-device + vanilla-FNV-data tests, gated on hardware/game-data, not broken) |

- **Shipped surface** (Phases 1–6, re-confirmed line-by-line): `AudioWorld`
  graceful degradation (`Option<AudioManager<DefaultBackend>>`,
  `SUB_TRACK_CAPACITY=512`/`SEND_TRACK_CAPACITY=32` above kira defaults);
  `AudioListener`/`AudioEmitter`/`OneShotSound` (all `SparseSetStorage`);
  `audio_system` (`sync_listener_pose` → `drain_pending_oneshots` →
  `dispatch_new_oneshots` → `prune_stopped_sounds`); `load_sound_from_bytes` +
  `SoundCache` (case-insensitive path keys, manual `clear()`-only eviction);
  spatial sub-track playback via both the entity (`OneShotSound`+`AudioEmitter`)
  and queue (`play_oneshot`, `VecDeque` cap 256, drop-oldest via `pop_front`)
  paths; looping emitters + tweened-`stop()` despawn truncation (looping AND
  non-looping, `stop_issued` debounce); single-slot streaming music on the main
  (non-spatial) track; global reverb send track (`feedback 0.85`/`damping
  0.6`/`stereo_width 1.0`/`Mix::WET`, `f32::NEG_INFINITY` dry default, `>-60.0`
  gate). Engine consumers: `footstep_system` (the only `play_oneshot` caller)
  and `reverb_zone_system` (the only `set_reverb_send_db` caller).

**Pending (future-phase, not flagged as missing)**: Phase 3.5b FOOT → per-
material sound, REGN ambient soundscapes, MUSC routing, per-cell-acoustics
reverb (detector is binary interior/exterior only), raycast occlusion
attenuation.

**MUSC parse→play gap (confirmed still absent, by design)**: cell-music
FormIDs are parsed (`default_music`/ZNAM, `music_type_form`/XCMO in
`crates/plugin/src/esm/cell/`) but no engine caller invokes `play_music` —
`grep play_music byroredux/` returns zero hits, re-confirmed live this cycle.
Single-slot / main-track invariants remain pinned for the eventual caller.

**Cross-audit correlation**: this cycle's one MEDIUM item (`OneShotSound`
marker surviving both dispatch-failure arms in `dispatch_new_oneshots`) was
independently rediscovered by Dimensions 1 and 6, but is **not new** — it was
already filed as **#2394** the same day (2026-08-07) by the concurrent
`/audit-ecs` sweep (`docs/audits/AUDIT_ECS_2026-08-07.md`, Dimension 7,
`ECS-D7-2026-08-07-01`) against the same commit. Reported here as
**Existing**, not re-filed, per the dedup protocol.

---

## Lifecycle Invariant Matrix

Owned by Dimension 6, with pointers from Dims 1/4/5 collapsed here per the
skill's dedup instruction. All re-derived independently this cycle.

| Invariant | State | Anchor |
|---|---|---|
| `AudioWorld` field-drop order (`active_sounds` → `pending_oneshots` → `music` → `reverb_send` → `reverb_send_db` → `listener` → `manager` → `multi_listener_warned`) | HOLDS | `lib.rs:234-274` |
| Manager capacities exceed kira defaults (`SUB_TRACK_CAPACITY=512`, `SEND_TRACK_CAPACITY=32`) | HOLDS | `lib.rs:170-171`, applied `lib.rs:288-295` |
| Lazy listener creation, no frame-1 cold-start panic | HOLDS | `sync_listener_pose`, `lib.rs:699-761` (4 sequential early-return gates) |
| Sticky listener (never cleared on entity churn, #849) | HOLDS | `lib.rs:751/757`; no write site clears `listener` back to `None` |
| Multi-listener diagnostic debounce (#843) | HOLDS | `lib.rs:714-727`, `multi_listener_warned` never reset |
| Orientation/coordinate-frame contract (Z-up→Y-up already resolved upstream) | HOLDS | `lib.rs:737`; verified against NIFAL `coord.rs::zup_matrix_to_yup_quat` (import-time, before any `Transform` exists) |
| Attenuation curve — `RangeInclusive`, `min<=max` normalized, defense-in-depth | HOLDS | `Attenuation::distance_range()` `lib.rs:555-567`; test `reversed_attenuation_normalizes_instead_of_panicking` (`tests.rs:1162-1177`) |
| `add_listener` failure — transient-retry, not permanent lockout | HOLDS | `lib.rs:753-755`, retries unconditionally next frame |
| `ActiveSound._track` held for Drop side-effect (underscore-name intact) | HOLDS | `lib.rs:190`, `826-829`, `961-967` |
| Two dispatch paths gate identically on `listener_id` + reverb-send gate | HOLDS, byte-identical | `lib.rs:769`/`852` (listener gate), `805-809`/`923-927` (send gate) |
| `looping`/`loop_region(..)` applied ONLY in the entity path | HOLDS | `lib.rs:944-950`; `PendingOneShot` (`213-218`) has no `looping` field at all — structurally incapable |
| Volume→dB conversion centralized (`linear_volume_to_db`) | HOLDS | `lib.rs:144-150`, called from `817`, `942`, `454` |
| `Arc<StaticSoundData>` clone is cheap (Arc-share, not PCM deep-copy) | HOLDS, verified against vendored `kira-0.10.8` source | `frames: Arc<[Frame]>`, `#[derive(Clone)]`, `data.rs:167-171` |
| Drain cap (>32/tick warns), producer cap (256, `VecDeque::pop_front`), drain-gate-before-`mem::take` (#851/#852/#853) | HOLDS | `lib.rs:789-796` (warn), `394-421` (cap), `785-788` (gate-before-take) |
| **`OneShotSound` cleared only on the `started`-success path — both dispatch failure arms leak the marker** | **KNOWN GAP** (Existing #2394, MEDIUM — not a regression, independently re-derived by Dims 1 & 6) | `lib.rs:909-980`, `Err` arms at `928-936`/`951-959` `continue` before `started.push` at `968` |
| Despawn truncation — tweened `stop()` on emitter removal, looping + non-looping (#845/#858/SAFE-23), `stop_issued` debounce (#844) | HOLDS | `prune_stopped_sounds`, `lib.rs:987-1064` |
| `SoundCache` lowercase-once (`get`/`insert`/`get_or_load`), `clear()` doesn't invalidate live `Arc`s, dormant in engine (`grep SoundCache byroredux/` = 0 hits, #859) | HOLDS | `lib.rs:1207-1209/1215-1220/1228-1247` (lowercase), `1267-1269` (clear); `try_load_default_footstep` bypasses cache |
| `bytes_estimate` telemetry helper exists, exercised by regression test, no `stats` console consumer yet (#850 accepted resolution — not re-litigated) | HOLDS as documented/accepted | `lib.rs:1281-1287`; test `tests.rs:279-327` |
| `get_or_load` invokes loader only on a genuine miss | HOLDS | `lib.rs:1228-1247`; test `sound_cache_get_or_load_invokes_loader_only_on_miss` (`tests.rs:222-268`) |
| Single-slot music, main-track (not spatial), streaming (not buffered) types, fade-then-drop `stop_music` | HOLDS, cross-checked against vendored kira source (`AudioManager::play` = `main_track().play`; no `impl Drop` on `StreamingSoundHandle`) | `lib.rs:249` (field), `435-465` (`play_music`), `1134-1149` (streaming loaders), `469-483` (`stop_music`, `fade_out_secs.max(0.0)`) |
| `is_music_active` reports inactive immediately after `stop_music` (deliberate — unblocks a legit re-`play_music`), no leak on natural-end tracks | HOLDS | `lib.rs:488-493` |
| Reverb send-track creation None-safe, default `NEG_INFINITY`, `>-60.0` gate, construction-time-only (#847, documented not a bug) | HOLDS | `lib.rs:319-337`, `343`, `495-516` |
| **Reverb-send gate duplicates `-60.0` literal instead of `SILENCE_DB`, copy-pasted across both dispatch sites** | **NEW LOW finding** (AUD-2026-08-07-D5-01) | `lib.rs:138` (`SILENCE_DB`), `806`/`924` (duplicated gate) |
| Scheduler stages/order (`PostUpdate` footstep → `Late` reverb-then-audio) | HOLDS, structurally guaranteed by two-phase `Scheduler::run` (parallel batch completes before any exclusive system in the same stage) | `boot.rs:837` (footstep, `PostUpdate`), `boot.rs:1046-1052`/`1070` (reverb parallel / audio exclusive, both `Late`); `crates/core/src/ecs/scheduler.rs:475-514` |
| `AudioWorld::new()` called exactly once, at boot; never re-invoked on cell transition | HOLDS | single call site, `boot.rs:375`; grep confirms `streaming.rs`/`cell_loader/{load,unload}.rs` have zero `AudioWorld`/`audio_system`/`SoundCache` references |
| Footstep stride accumulation (XZ-only, reset-not-remainder on fire, #single-jump guard) | HOLDS | `byroredux/src/systems/audio.rs:146-154`; test `single_large_jump_fires_one_footstep_only` |
| First-tick seed without firing (#848) | HOLDS | `audio.rs:140-144`; test `first_tick_seeds_last_position_without_firing` |
| `FootstepScratch` Vec reuse on both success and `AudioWorld`-absent bail paths (#932) | HOLDS | `audio.rs:127` (clear), `166-167` (take+drop), `174-176`/`197-199` (restore, both paths) |
| Lock-drop ordering — no two resource-mut locks held simultaneously across `play_oneshot` | HOLDS | `audio.rs:124-178` |
| Footstep attenuation tighter than default (`{0.5,12.0}` vs `{2.0,30.0}`) | HOLDS | `audio.rs:183-189` vs `lib.rs:569-577` |
| Camera `AudioListener` + `FootstepEmitter` opt-in, component-driven (no hardcoded entity) | HOLDS | `scene.rs:636-644`; `footstep_system` walks `query_mut::<FootstepEmitter>()` generically |
| `try_load_default_footstep` no-ops cleanly on missing BSA/arg/file, bypasses `SoundCache` | HOLDS | `byroredux/src/asset_provider/texture.rs:79-119` |
| `reverb_zone_system` constants, bit-equality gate, dual no-op safety, runs before `audio_system` in `Stage::Late` | HOLDS | `audio.rs:40-76`; `INTERIOR_REVERB_SEND_DB=-12.0`/`EXTERIOR=NEG_INFINITY`, `.to_bits()` gate |

---

## Findings

### AUD-2026-08-07-D5-01: Reverb-send gate duplicates `-60.0` as a literal instead of reusing `SILENCE_DB`
- **Severity**: LOW
- **Dimension**: Reverb Send & Routing
- **Location**: `crates/audio/src/lib.rs:138` (`SILENCE_DB` definition),
  `crates/audio/src/lib.rs:806` and `crates/audio/src/lib.rs:924` (duplicated gate)
- **Status**: NEW
- **Description**: `SILENCE_DB: f32 = -60.0` already exists in this file
  (`lib.rs:138`) as the named "below this = inaudible" threshold, used by
  `linear_volume_to_db`'s clamp. The reverb-send gate at both dispatch sites
  (`drain_pending_oneshots` and `dispatch_new_oneshots`) re-expresses the same
  semantic threshold as a bare `-60.0` literal instead of referencing the
  constant, and the whole 4-line gate-and-apply block is copy-pasted verbatim
  across the two dispatch functions rather than factored into one shared
  helper.
  ```rust
  // lib.rs:805-809 (drain_pending_oneshots) and lib.rs:923-927 (dispatch_new_oneshots) — identical
  if let Some(reverb) = audio_world.reverb_send.as_ref() {
      if audio_world.reverb_send_db.is_finite() && audio_world.reverb_send_db > -60.0 {
          track_builder = track_builder.with_send(reverb.id(), audio_world.reverb_send_db);
      }
  }
  ```
- **Impact**: None today — the two sites are verified byte-identical, and nine
  consecutive prior audit cycles (2026-05-05 through 2026-08-03) have each
  manually re-confirmed they stay in sync. That track record is the tell: the
  invariant is held by repeated manual diffing, not by the compiler making
  drift impossible. A future edit to one site landing without the other would
  silently desync reverb wetness between queue-driven one-shots (footsteps)
  and entity-driven one-shots (emitters).
- **Related**: Direct precedent for this exact fix pattern already exists in
  this file — AUD-2026-06-23-01 (closed) extracted three inlined
  `20*log10(volume)` conversions into the shared `linear_volume_to_db` helper
  for the identical divergence-risk reason.
- **Suggested Fix**: Extract a small private helper, e.g. `fn
  apply_reverb_send(builder: SpatialTrackBuilder, audio_world: &AudioWorld) ->
  SpatialTrackBuilder`, called from both dispatch sites, referencing
  `SILENCE_DB` instead of the bare `-60.0` literal.

### Existing: #2394 — `OneShotSound` marker not cleared on the two `dispatch_new_oneshots` failure arms
- **Severity**: MEDIUM (per the open issue; independently concurred by both
  Dimension 1 and Dimension 6 this cycle)
- **Dimension**: Spatial Sub-Track Lifecycle / Manager Lifecycle & ECS/Cell Streaming
- **Location**: `crates/audio/src/lib.rs:909-980` (`Err` arms at `928-936` and
  `951-959`)
- **Status**: Existing — filed 2026-08-07 by the concurrent `/audit-ecs` sweep
  (`docs/audits/AUDIT_ECS_2026-08-07.md`, Dimension 7, `ECS-D7-2026-08-07-01`),
  still OPEN. **Not re-filed here** — reporting it again would create a
  duplicate GitHub issue on `/audit-publish`.
- **Description**: In `dispatch_new_oneshots`'s per-entity dispatch loop, both
  fallible calls (`mgr.add_spatial_sub_track(...)` and `track.play(sound)`)
  `continue` on `Err` *before* `started.push(p.entity)`. Only entities in
  `started` have their `OneShotSound` marker removed. An entity whose dispatch
  fails on either arm keeps `OneShotSound` forever, gets re-collected every
  subsequent frame, and re-attempts dispatch. On the `track.play` failure
  specifically, the freshly-allocated `track` local is never pushed into
  `active_sounds` — it drops at the `continue`, immediately tearing down the
  just-created spatial sub-track per the `_track` Drop-side-effect contract.
  ```rust
  let mut track = match mgr.add_spatial_sub_track(listener_id, p.position, track_builder) {
      Ok(t) => t,
      Err(e) => { log::warn!(...); continue; }   // OneShotSound NOT removed
  };
  let handle = match track.play(sound) {
      Ok(h) => h,
      Err(e) => { log::warn!(...); continue; }   // OneShotSound NOT removed
  };
  started.push(p.entity);   // only success reaches here
  ```
- **Impact**: Transient — a burst hitting kira's sub-track resource limit
  (`SUB_TRACK_CAPACITY=512`) during heavy footstep/combat traffic causes every
  marked entity to retry every frame at 60 Hz with one `warn!` each, until a
  track frees. Persistent (theoretical) — an entity whose `AudioEmitter.sound`
  is structurally unplayable holds `OneShotSound` for the rest of the session:
  per-frame allocate/free churn plus an unbounded warning stream. Bounded by
  the tagged-entity count, not unbounded memory growth.
- **Contrast**: The queue path (`drain_pending_oneshots`) has the analogous
  `continue` on its own failure arms but is **not** a leak there — the
  `PendingOneShot` was already consumed out of the `VecDeque` by the
  `mem::take` before the per-item loop runs, so a failed queue item simply
  drops with no retry and no stale ECS marker. This asymmetry (entity path can
  leak a marker, queue path structurally cannot) is worth keeping in mind if
  the eventual fix targets only one path.
- **Suggested Fix** (concurs with #2394's own): push `p.entity` onto `started`
  (or a separate `consumed` vec) on both error arms so the marker drops
  regardless of dispatch outcome. If retry-until-success is deliberate, bound
  it with a per-marker attempt counter and rate-limit the `warn!`. No
  regression test exists yet for either fallible call in
  `dispatch_new_oneshots` — confirmed via a scan of all test names in
  `tests.rs`.

---

## Items explicitly NOT re-flagged (confirmed still closed/guarded)

- `#843` (multi-listener debounce), `#844` (`stop_issued` debounce), `#845`/`#858`
  (despawn truncation, looping+non-looping), `#847` (construction-time-only
  reverb send, documented limitation), `#849` (sticky listener), `#850`
  (`SoundCache` manual eviction accepted resolution), `#851` (drain-ordering),
  `#852`/`#853` (`VecDeque` producer cap + manager-None early return), `#859`
  (`SoundCache` dormant-API reality), `#932` (`FootstepScratch` Vec reuse) —
  all independently re-verified HOLDS this cycle against live line numbers,
  none regressed.
- `AUD-2026-06-23-01` (centralized `linear_volume_to_db`) — HOLDS, no
  reintroduced inlined copy at any of the three play sites.
- `AUD-2026-07-25-01` (footstep docstring path) — remained fixed (confirmed
  again in the 2026-08-03 cycle; unchanged this cycle).
- `AUD-2026-07-02-01` / #1859 (docstring path drift for
  `try_load_default_footstep`) — closed 2026-07-15, unchanged this cycle.

---

## Future-Phase Readiness (invariants pinned for the next phase)

- **FOOT / 3.5b (per-material footstep sound)**: `FootstepConfig.default_sound`
  decouple + `FootstepScratch` Vec-reuse survive unchanged; a producer can
  wire per-material sounds without touching `footstep_system`'s stride/seed/
  attenuation logic (`{0.5, 12.0}` tight falloff).
- **REGN (ambient soundscapes)**: sub-track capacity (512) still exceeds the
  ~400-emitter populated-interior projection; sticky-listener + despawn-
  truncation guards cover mass emitter churn on cell streaming.
- **MUSC routing**: single-slot / main-track / streaming-type invariants
  pinned; the eventual caller must gate on FormID equality (parse→play wiring
  confirmed absent — zero `play_music` callers in `byroredux/`).
- **SoundCache producer**: decoupled API + tests survive so the first
  consumer can land — but it MUST also wire eviction (no automatic LRU;
  `bytes_estimate` telemetry exists for the growth-regression signal but has
  no console-`stats` consumer yet, per #850's accepted resolution — confirmed
  still dormant this cycle, not re-litigated).
- **Reverb per-cell acoustics**: detector is binary interior/exterior; the
  bit-equality-gated transition (`reverb_zone_system`) is the extension point.
  Landing this alongside AUD-2026-08-07-D5-01's suggested `apply_reverb_send`
  helper would keep the two knobs (per-cell detector value, per-dispatch gate)
  from drifting apart as more callers appear.
- **`OneShotSound` failure-path fix (#2394)**: whoever picks this up should
  add a forced-failure regression test for both fallible calls in
  `dispatch_new_oneshots` — confirmed absent from `tests.rs` this cycle.

---

## Delta vs prior report

This report supersedes `AUDIT_AUDIO_2026-08-03.md`. That cycle closed with
**zero findings** — the first fully clean cycle in the report chain. This
cycle:

- Confirmed the audio crate (`crates/audio/src/lib.rs`) has had **zero**
  commits since the prior audit's HEAD (`1ae86f62`) — a true no-op interval
  for the crate itself, and for its two live engine consumers.
- Re-verified all 7 dimensions independently against live source (Dims 2/3
  carried forward from an interrupted same-day attempt at this sweep, both
  confirmed complete/current before reuse; Dims 1/4/5/6/7 run fresh).
- Surfaced one new LOW finding (AUD-2026-08-07-D5-01, reverb-send gate
  duplication) that the 2026-08-03 report's Dimension 5 pass had recorded as
  "HOLDS, byte-identical" without flagging the duplication itself as a
  maintainability risk — this cycle's Dimension 5 agent made that call
  explicitly, citing the direct `linear_volume_to_db` precedent.
- Cross-correlated one MEDIUM item (`OneShotSound` marker leak on
  `dispatch_new_oneshots` failure arms) independently rediscovered by both
  Dimension 1 and Dimension 6, and identified it as already filed same-day by
  the concurrent `/audit-ecs` sweep as #2394 — reported here as Existing, not
  re-filed, avoiding a duplicate issue.

---

## Severity Counts

- **CRITICAL**: 0
- **HIGH**: 0
- **MEDIUM**: 1 (Existing: #2394 — not to be re-filed)
- **LOW**: 1 (NEW: AUD-2026-08-07-D5-01)
