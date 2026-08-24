# Audio Subsystem Audit (M44) — 2026-08-24

- **Command**: `/audit-audio` → all 7 dimensions, `--depth deep` (standalone
  run, single-agent — no sub-agent fan-out per explicit task constraint)
- **Branch**: main · **HEAD**: `048a8bd8`
- **kira**: pinned `0.10` (workspace `Cargo.toml`, unchanged) · resolved
  `kira-0.10.8`
- **Method**: single-agent, no sub-agents, no cargo invocation beyond
  targeted `cargo check -p byroredux-audio -p byroredux` and
  `cargo test -p byroredux-audio` / `cargo test -p byroredux --bin byroredux
  systems::audio` / `... asset_provider::audio` (per-crate, since bare
  `cargo test --workspace` fails on an unrelated `E0004` in
  `crates/scripting/examples/fragment_coverage.rs:59`). Every dimension
  re-derived from live source: `crates/audio/src/lib.rs` (1508 lines),
  `crates/audio/src/tests.rs` (1420 lines), `byroredux/src/systems/audio.rs`
  (768 lines), `byroredux/src/asset_provider/audio.rs` (352 lines, new since
  last cycle), `byroredux/src/components.rs` (audio + `RegionAmbientRes`
  sections), `byroredux/src/boot.rs` (scheduler + resource wiring),
  `byroredux/src/cell_loader/load.rs`, `byroredux/src/scene/world_setup.rs`,
  `byroredux/src/app_step.rs`, `byroredux/src/scheduler_access_tests.rs`,
  `crates/plugin/src/esm/records/misc/world.rs` (REGN `RDAT` decode),
  `byroredux/src/save_io/registry_completeness_tests.rs`. Dedup baseline:
  `/tmp/audit/audio/issues.json` (fresh pull, all open issues) + the full
  prior `docs/audits/AUDIT_AUDIO_*.md` chain (12 prior reports,
  2026-05-05 → 2026-08-20).

---

## Delta Analysis (since `AUDIT_AUDIO_2026-08-20.md`, HEAD `bb0b92f2`)

`git log --oneline --since=2026-08-20 -- crates/audio/ byroredux/src/systems/audio.rs
byroredux/src/components.rs byroredux/src/asset_provider/texture.rs
byroredux/src/boot.rs byroredux/src/scene.rs` returns **six** audio-relevant
commits — the busiest four-day window this subsystem has had since M44
shipped:

| Commit | Change | Audio-relevant? |
|---|---|---|
| `5ce2b1c5` "fix(audio): convert BU->metres at the kira boundary, make the above-water filter a real bypass, land submersion after camera follow" | Fixes #3178 (unit seam), #3179 (filter bypass), #3180 (submersion stage order), #3181 (doc drift) — all four findings from the 2026-08-20 report | **Yes — closes last cycle's entire findings list** |
| `f8cfd185` "fix(watal): isolate ripple audio cadence" | Fixes AUD-2026-08-20-D7-01 (ripple position/intensity mismatch + global cooldown), filed as #3189's sibling but never issue-numbered on its own — verified fixed by direct inspection | **Yes — closes the one remaining new finding from last cycle** |
| `3ef05d1b` "Resolve REGN ambient-sound directive live per resident cell/tile" | `RegionAmbientRes` resource + `select_active_region_sound`; resolved live on every interior load and exterior tile crossing | **Yes — new state, no consumer yet at this commit** |
| `ede48ffb` "Play REGN ambient background music through the resolved directive" | `SoundArchiveProvider` + `dispatch_region_ambient_music` (`byroredux/src/asset_provider/audio.rs`, new file); wires `RegionAmbientRes.music_form` → `AudioWorld::play_music` | **Yes — the first genuinely new gameplay-audio producer since the water work, and the first live `play_music` caller ever** |
| `2b6f0b44` "fix(scheduler): declare water and animation reads" | Scheduler access declarations | Touches water systems adjacent to audio; no audio-crate change |
| `ba098198` "perf(watal): index water draw slots" | Render-side water indexing | Not audio |

**Net: last cycle's entire findings list (1 HIGH, 1 MEDIUM, 4 LOW — all six)
is now closed**, each verified fixed by direct source inspection rather than
trusted from the commit message (see Verification section below), **and** a
new gameplay-audio feature shipped: REGN ambient background music. This is
the first time an /audit-audio cycle has opened with a clean slate.

### Verification of last cycle's fixes (not taken on faith)

- **#3178 (unit seam, was HIGH)** — `bu_to_audio_space()` (`lib.rs:198-200`)
  divides by `BETHESDA_UNITS_PER_METER` and is now called at all four
  position sites: `sync_listener_pose`'s `add_listener` (`lib.rs:932`) and
  `set_position` (`lib.rs:947`), and both `add_spatial_sub_track` dispatch
  paths (`lib.rs:1000`, `lib.rs:1134`). **Confirmed fixed.**
- **#3179 (filter not a bypass, was MEDIUM)** — `apply_underwater_filter`
  (`lib.rs:215-225`) now calls `.mix(underwater_mix(underwater))` in addition
  to `.cutoff(...)`; `underwater_mix` (`lib.rs:239-245`) returns `Mix::DRY`
  above water, `Mix::WET` submerged; `update_underwater_filters`
  (`lib.rs:854-868`) tweens both `set_cutoff` and `set_mix` together.
  **Confirmed fixed** — genuine bypass, not a taste tweak.
- **#3180 (submersion stage order, was LOW)** — `submersion_system` moved to
  a `Stage::Late` exclusive (`boot.rs:1372-1386`), registered immediately
  after `camera_follow_system`'s parallel batch and before `water_damage_system`
  / `water_audio_system` / `audio_system`. A live scheduler-introspection test
  (`scheduler_access_tests.rs:480`,
  `submersion_runs_after_camera_follow_and_before_water_audio`) pins the
  order by walking the real `build_scheduler()` output, not a source grep.
  **Confirmed fixed**, and confirmed test-guarded against re-regression.
- **#3181 (doc drift, was LOW)** — crate docstring gained a "Water audio"
  phase block and a "Units" section (`lib.rs:107-127`); `docs/feature-matrix.md`
  gained the underwater-low-pass and splash/ripple rows (lines 143-144);
  `ROADMAP.md`'s M44 row gained the WATAL consumer paragraph. **Confirmed
  fixed for the water surface** — see this cycle's own finding below for the
  REGN-shaped drift that reopened the same class one layer over.
- **AUD-2026-08-20-D7-01 (ripple mismatch, was LOW, unfiled)** —
  `water_audio_system`'s ripple selection now goes through
  `strongest_ready_ripple` (`systems/audio.rs:14-23`), which takes both
  position and intensity from the same `max_by` winner, and per-surface
  cooldowns live in `WaterAudioState.ripple_cooldowns: FxHashMap<EntityId, f32>`
  (`components.rs:1530-1542`) rather than one global scalar. Test
  `ripple_selection_keeps_event_fields_together_and_cooldowns_per_surface`
  (`systems/audio.rs:599-620`) pins both halves. **Confirmed fixed.**

---

## Executive Summary

**7 dimensions run. 1 NEW finding (0 CRITICAL / 0 HIGH / 0 MEDIUM / 1 LOW).**
Every finding from the prior cycle is closed. This is the cleanest state the
subsystem has audited into across twelve cycles.

| # | Dimension | NEW findings |
|---|---|---|
| 1 | Spatial Sub-Track Lifecycle & Leaks | 0 |
| 2 | Listener Pose & Attenuation | 0 |
| 3 | SoundCache Growth & Eviction | 0 |
| 4 | Streaming Music Lifecycle | 0 |
| 5 | Reverb Send & Routing | 0 |
| 6 | Manager Lifecycle, ECS & Cell Streaming | **1** (LOW) |
| 7 | Gameplay Audio Wiring | 0 |

- **Headless-mode boot**: **PASS**. `cargo test -p byroredux-audio` → 29
  passed, 6 ignored (real-device/real-data tests), 0 failed. `cargo test -p
  byroredux --bin byroredux systems::audio` → 12 passed, 0 failed. `cargo
  test -p byroredux --bin byroredux asset_provider::audio` → 13 passed, 0
  failed. `cargo check -p byroredux-audio -p byroredux` clean. Every new
  REGN dispatch path (`dispatch_region_ambient_music`) has an explicit
  no-panic test for each missing-layer case: no `SoundArchiveProvider`
  resource, an empty provider, `music_form: None`, and an unresolvable
  FormID — all four assert no panic and, where `AudioWorld` is present,
  that playback correctly stops rather than leaking a stale track.
- **The new surface this cycle**: REGN ambient background music
  (`ede48ffb` + `3ef05d1b`, 2026-08-23) — `RegionAmbientRes` resolves the
  resident cell/tile's highest-priority `REGN` `Sound` entry's `music`
  FormID (`RDMD`/`RDMO`/`RDSB`, generalizing across Oblivion/Skyrim/FNV),
  and `dispatch_region_ambient_music` (`byroredux/src/asset_provider/audio.rs`,
  new file) resolves it through a persistent `SoundArchiveProvider`
  (`--sounds-bsa`) to `AudioWorld::play_music` with a 3-second crossfade —
  the **first live caller of `play_music` since Phase 5 shipped**. Wired at
  all four cell-apply sites (`cell_loader/load.rs`'s interior path, plus its
  three callers `scene.rs`/`debug_load.rs`/`transition.rs`, and
  `scene/world_setup.rs::apply_cell_region_ambient` for exteriors), each
  change-guarded against the resource's *prior* value so two cells/tiles
  sharing one tagging region don't restart the crossfade on every crossing.
  Traced this dispatch chain end-to-end against five plausible failure
  modes (listed under Verification below); found none.
- **Guards re-verified structurally and where practical by direct test run**
  (not blanket-trusted). Every #842/#843/#844/#845/#848/#849/#851/#852/#853/
  #858/#932/#1612/#2394/#2405/#3178/#3179/#3180/#3181 anchor is intact at
  HEAD. New guards this cycle: `emitter_at_max_distance_in_bu_lands_on_max_distance_in_audio_space`,
  `every_kira_position_site_goes_through_the_unit_seam`,
  `both_dispatch_paths_build_the_filter_through_one_call_site`,
  `above_water_filter_state_is_a_dry_bypass`,
  `above_water_cutoff_never_reaches_the_kira_clamp_ceiling`,
  `underwater_cutoff_matches_the_submersion_state` (all `crates/audio/src/tests.rs`),
  `submersion_runs_after_camera_follow_and_before_water_audio`
  (`byroredux/src/scheduler_access_tests.rs`),
  `ripple_selection_keeps_event_fields_together_and_cooldowns_per_surface`,
  and the eight `dispatch_region_ambient_music`/`SoundArchiveProvider` tests
  in `byroredux/src/asset_provider/audio.rs`, plus three `RegionAmbientRes`
  tests in `components.rs` and seven `select_active_region_sound` tests in
  `crates/plugin/src/esm/records/misc/world.rs`.
- **Prior-cycle carried findings — still OPEN, still present at HEAD**
  (noted and skipped per the dedup protocol):

| Issue | Finding | State at HEAD `048a8bd8` |
|---|---|---|
| **#3086** (LOW) | Entity-path spatial sub-track position is frozen at dispatch; `AudioEmitter`'s docstring promises a per-frame update the code never performs | **Unchanged.** `grep set_position crates/audio/src/lib.rs` still returns only the listener sites (`947`, doc `703`); no emitter reposition exists. Unaffected by REGN (non-spatial main track) or the water fixes. |
| **#3087** (LOW) | Stale scheduler-wiring comments — `audio_system` still described as a "Phase 1 stub"; `reverb_zone_system` registration attributed to *main.rs* | **Unchanged.** `boot.rs:1449` ("The Phase 1 body is a stub") and `systems/audio.rs:40-42` (attributing registration to *main.rs*, when it's `boot.rs`) both still present. |
| **#3189** (LOW) | `try_load_default_water_splash` duplicates the `--sounds-bsa` scan and re-opens the same archive a second time at boot; both loaders bypass `SoundCache` | **Present, and its scope grew.** There are now **three** independent `Archive::open()` calls against the same `--sounds-bsa` path at boot: `try_load_default_footstep` (`asset_provider/texture.rs:111`), `try_load_default_water_splash` (`asset_provider/texture.rs:156`), and the new `build_sound_archive_provider` (`asset_provider/audio.rs:110`). The third is architecturally the *correct* pattern for a FormID-driven repeat-resolution consumer (persistent handle, not ad hoc re-open) — its own doc comment explicitly contrasts itself with the first two. Worth noting on #3189 when it's next worked: the fix should probably migrate the footstep/splash one-off loads onto the same persistent `SoundArchiveProvider` rather than just deduplicating the original two. |

- **Shipped surface, re-confirmed**: `AudioWorld` graceful degradation
  (`SUB_TRACK_CAPACITY = 512` / `SEND_TRACK_CAPACITY = 32`); `AudioListener`
  / `AudioEmitter` / `OneShotSound`; `audio_system` = `sync_listener_pose` →
  `update_underwater_filters` → `drain_pending_oneshots` →
  `dispatch_new_oneshots` → `prune_stopped_sounds`; both dispatch paths
  (queue `VecDeque` cap 256 `pop_front`; entity path with `loop_region(..)`);
  tweened-`stop()` despawn truncation with `stop_issued` debounce; **now
  two** `play_music` behaviours pinned — single-slot / main-track /
  streaming-type invariants, now exercised by a real caller
  (`dispatch_region_ambient_music`) instead of only by test; global reverb
  send (`NEG_INFINITY` dry default); the BU→metre unit seam
  (`bu_to_audio_space`); the genuine dry-bypass underwater filter. Engine
  consumers are now **three**: `footstep_system`, `water_audio_system`, and
  `dispatch_region_ambient_music`; `reverb_zone_system` remains the only
  `set_reverb_send_db` caller.
- **Pending (future-phase, correctly not flagged as missing)**: Phase 3.5b
  FOOT → per-material sound, REGN `incidental`/`sounds` (RDSI, the
  chance-based ambient-loop list — still blocked on the unresolved
  `chance_raw` fixed-point scale, per the no-guessing policy), MUSC/ZNAM/XCMO
  cell-level music FormIDs (still zero consumers — `grep -rn "ZNAM\|music_type_form"`
  shows the fields are parsed into `CellData` but nothing reads them; REGN's
  `play_music` caller is a *different* mechanism, not MUSC wired up), per-cell
  acoustics beyond binary interior/exterior, raycast occlusion attenuation.

---

## Lifecycle Invariant Matrix

Owned by Dimension 6 per the skill's dedup instruction (Dims 1/4/5 point here).

| Invariant | State | Anchor |
|---|---|---|
| `AudioWorld` field-drop order (`active_sounds` → `pending_oneshots` → `music` → `reverb_send` → `reverb_send_db` → `listener` → `manager` → `multi_listener_warned` → `underwater`) | **HOLDS** — unchanged since 08-20 | `lib.rs:372-415` |
| `ActiveSound` field order (`entity` → `handle` → `_track` → `underwater_filter` → `underwater` → `unload_fade_ms` → `stop_issued`) | **HOLDS** | `lib.rs:321-345` |
| Manager capacities exceed kira defaults (512 / 32) | HOLDS | `lib.rs:304-305`, applied `431-432` |
| `ActiveSound._track` underscore name intact, Drop-side-effect only | HOLDS | `lib.rs:324` |
| Lazy listener creation, no frame-1 cold-start panic | HOLDS | `lib.rs:888-950` |
| Sticky listener, never cleared on entity churn (#849) | HOLDS | written only at `lib.rs:940`; no clear site exists |
| Multi-listener diagnostic debounced (#843) | HOLDS | `multi_listener_warned` never reset |
| **Spatial positions cross the BU→metre seam at every site (#3178)** | **HOLDS** — pinned by `every_kira_position_site_goes_through_the_unit_seam` (whitespace-normalized so rustfmt re-wrapping can't disarm it) | `lib.rs:198-200`, calls at `932`/`947`/`1000`/`1134` |
| Attenuation `RangeInclusive`, `min<=max` normalized (#1612) | HOLDS | `Attenuation::distance_range()` `lib.rs:724-728` |
| `add_listener` failure is transient-retry, not permanent lockout | HOLDS | `lib.rs:928-945` |
| Both dispatch paths gate on `listener_id` before any work | HOLDS | `lib.rs:958` / `1046` |
| Both dispatch paths apply the reverb gate through one shared helper (#2405) | HOLDS | `lib.rs:990` / `1124` → `apply_reverb_send` `265-276` |
| Both dispatch paths build the underwater filter through one call site (#3179) | HOLDS — `both_dispatch_paths_build_the_filter_through_one_call_site` | `apply_underwater_filter` `lib.rs:215-225`, called `997`/`1131` |
| **Underwater filter above-water state is a genuine `Mix::DRY` bypass, not a wet 20 kHz SVF** | HOLDS — `above_water_filter_state_is_a_dry_bypass` | `lib.rs:239-245` |
| `looping` / `loop_region(..)` in the entity path only | HOLDS | `lib.rs:1154-1160`; `PendingOneShot` has no `looping` field |
| Volume→dB centralized (`linear_volume_to_db`) | HOLDS | `lib.rs:251`, called from `596`, `1009`, `1152` |
| Drain cap (>32/tick warns) · producer cap (256 `pop_front`) · drain-gate-before-`mem::take` (#851/#852/#853) | HOLDS | `lib.rs:546-556`, `957`→`977` |
| `OneShotSound` marker consumed on success **and** both failure arms (#2394) | HOLDS — `oneshot_marker_is_consumed_on_both_dispatch_failure_arms` | `lib.rs:1144`, `1169`, `1182` |
| Despawn truncation, `stop_issued` debounce (#844/#845/#858) | HOLDS | `lib.rs:1223`, `1254` |
| `SoundCache` lowercase-once, dormant in engine (#859/#850) | HOLDS — still zero producers. Three loaders now bypass it (footstep, splash, REGN — though REGN is streaming, not the `StaticSoundData` shape `SoundCache` holds, so it was never a candidate consumer) | — |
| Single-slot music · main track · streaming types · fade-then-drop | **HOLDS, now exercised by a real caller** — `dispatch_region_ambient_music` is the first live `play_music`/`stop_music` caller; the single-slot contract means a rapid multi-region crossing correctly crossfades rather than stacking tracks | `lib.rs:577-625`; `asset_provider/audio.rs:158-211` |
| Reverb `None`-safe · `NEG_INFINITY` default · `> SILENCE_DB` gate | HOLDS | `lib.rs:265-284` |
| Late-stage exclusive order: ragdoll → submersion → water_damage → reconcile_dead → water_interaction → water_audio → audio_system → event_cleanup | **HOLDS, live-scheduler-tested** (#3180) | `boot.rs:1349-1503`; `scheduler_access_tests.rs:480-560` |
| `footstep_system` (`PostUpdate`) enqueues, `audio_system` (`Late`) drains — same frame | HOLDS | `boot.rs:1098`, `1464` |
| `AudioWorld::new()` called exactly once, at boot | HOLDS | single call site `boot.rs` |
| `SoundArchiveProvider` registered at boot, before any cell load reads it | HOLDS | `boot.rs:542`, inside `build_world` (line 372), called before `setup_scene` (`main.rs:350` vs `523`) |
| REGN ambient dispatch change-guarded against `RegionAmbientRes`'s *prior* value at all four call sites (interior ×3, exterior ×1) | HOLDS | `cell_loader/load.rs:539-548`; `scene/world_setup.rs:523-541` |
| Exterior→interior door transition drains `self.streaming` to `None` **before** the interior load, so `apply_cell_region_ambient` (exterior path) cannot fire again until a subsequent exterior session — no dueling writers to `RegionAmbientRes` | HOLDS — traced `step_cell_transition`'s `Interior` arm (`app_step.rs:722-770`) → `drain_streaming_state` (`streaming_helpers.rs:367-374`, `streaming_slot.take()`) → `step_streaming`'s `if self.streaming.is_none() { return; }` guard (`app_step.rs:43-45`) | `app_step.rs:709-771`; `streaming_helpers.rs:367` |
| REGN `Sound` entry priority resolution is a stable sort (region-list order, then within-region entry order, on ties) | HOLDS | `select_active_region_sound` `misc/world.rs:629-641`, uses `Vec::sort_by` (stable) |
| **Shipped audio surface described consistently across crate docstring / `feature-matrix.md` / `ROADMAP.md`** | **PARTIALLY DRIFTED** — the water surface is now correctly documented (#3181 fixed it); the REGN surface that shipped one day later is not — **AUD-2026-08-24-D6-01** | `lib.rs:129-138`; `docs/feature-matrix.md:145-146`; `ROADMAP.md:705` |

---

## Findings

### AUD-2026-08-24-D6-01: REGN ambient background music shipped but all three authoritative status sources still mark it unbuilt, and the audio test count has drifted a fourth time in nine days

- **Severity**: LOW
- **Dimension**: Manager Lifecycle & ECS/Cell Streaming (documentation)
- **Location**: `crates/audio/src/lib.rs:129-138` (module docstring "Future
  work" list), `docs/feature-matrix.md:145-146` (M44 feature table),
  `ROADMAP.md:705` (M44 row's trailing "Phases 3.5b + REGN-driven ambient
  pending" clause), `byroredux/src/systems/audio.rs` (12 tests, ROADMAP
  says 11), `byroredux/src/asset_provider/audio.rs` (13 tests, uncounted
  anywhere in ROADMAP's audio total)
- **Status**: NEW
- **Description**: `ede48ffb` + `3ef05d1b` (2026-08-23, the day before this
  audit) shipped REGN ambient background-music dispatch — a full FormID →
  archive path → streaming decode → `AudioWorld::play_music` pipeline,
  change-guarded and tested end-to-end (see the Executive Summary and the
  Lifecycle Invariant Matrix above; this audit found no defect in the
  feature itself). None of the three status sources the skill designates as
  authoritative reflect it:

  1. **`crates/audio/src/lib.rs`'s "Future work" list** (`lib.rs:129-138`)
     still reads:
     ```
     - FOOT records parser (3.5b) → per-material sound lookup.
     - REGN ambient soundscapes (region-based ambient layers).
     - MUSC + hardcoded music routing with crossfade.
     - Per-cell acoustic reverb zones ...
     ```
     The second bullet is now half-wrong: the `music` field of REGN's
     `Sound` `RDAT` entry has a live dispatch path. `incidental`/`sounds`
     remain genuinely unbuilt (correctly still-pending), so the bullet
     needs splitting, not deleting.
  2. **`docs/feature-matrix.md:146`** — the audio table's last row reads
     `| Region ambient (REGN) | ✗ |`. This is the skill's own designated
     *"authoritative runtime-status table"* and it is flatly wrong: REGN
     music dispatch is `✓` (partial — music only, not incidental/sounds).
  3. **`ROADMAP.md`'s M44 row** (`ROADMAP.md:705`) closes with
     `**Phases 3.5b + REGN-driven ambient pending**: FOOT records → ...;
     REGN region-keyed ambient layers; ...` — same class of error as (1).
     The row's own test-count sentence (refreshed by #3088's fix,
     `5ce2b1c5`, the same day) has already drifted again independently of
     REGN: it states `byroredux/src/systems/audio.rs` adds **"11 more
     (6 footstep_tests + 5 reverb_tests)"** — live is **12** (`f8cfd185`,
     committed 12 hours after the count was "measured," added
     `ripple_selection_keeps_event_fields_together_and_cooldowns_per_surface`
     to `footstep_tests`, making it 7 + 5 = 12). Verified directly: `cargo
     test -p byroredux --bin byroredux systems::audio` → **12 passed**.
     The ROADMAP sentence's own closing line — *"This figure has drifted
     three times in five days because it is prose: prefer the two commands
     above"* — undersold the trend; it is now a fourth drift in nine days,
     and a fifth data point the sentence doesn't cover at all: REGN's own
     consumption side, `byroredux/src/asset_provider/audio.rs`, carries
     **13** tests (verified: `cargo test -p byroredux --bin byroredux
     asset_provider::audio` → **13 passed**) that no ROADMAP sentence
     mentions in the audio total at all. The true total across the three
     files this audit actually exercised is **60** (35 crate + 12
     systems/audio.rs + 13 asset_provider/audio.rs), not the ROADMAP row's
     "46."
- **Evidence**: `grep -n "REGN" crates/audio/src/lib.rs` returns only the
  stale "Future work" bullet — no mention of `dispatch_region_ambient_music`
  or `SoundArchiveProvider` anywhere in the crate (expected, since the
  dispatcher lives in the engine binary, not the crate — but the crate's own
  `play_music` doc at `lib.rs:565-567` still frames music purely in terms of
  Phase 5's original design, with no note that it now has a live caller).
  `grep -n "Region ambient" docs/feature-matrix.md` → the single `✗` row.
  Cross-checked against the four `dispatch_region_ambient_music` call sites
  independently confirmed wired in the Lifecycle Invariant Matrix above —
  the feature is real, not a half-landed WIP the docs are correctly hedging
  on.
- **Impact**: Documentation only, no runtime behaviour. Same failure mode
  the skill flags docstring drift for: the next audit cycle, or a
  contributor scoping "REGN ambient" work from `docs/feature-matrix.md`
  alone, would read a `✗` and either re-derive `dispatch_region_ambient_music`
  from scratch or report its absence as a gap — exactly the "~5 of 30 bad
  findings in past sweeps" class the skill's methodology section exists to
  prevent, and a bigger miss than the water-surface drift #3181 already
  fixed once this window (a whole shipped consumer marked unbuilt, not just
  an under-detailed phase note). The test-count sentence is lower-stakes
  but is now demonstrably unable to stay in sync with a subsystem shipping
  at this cadence — four drifts in nine days on a single prose sentence is
  a process signal, not a one-off typo.
- **Related**: Same class as #3181 (just-fixed water drift), #1859/
  AUD-2026-07-02-01 (stale `SoundCache` path), and #3088 (the test-count
  sentence this finding's third component re-drifts). Consider filing this
  as a direct follow-up to #3088 rather than a fresh doc-rot issue, since
  it is arguably that issue re-opening one day after closure — the
  dedup call is left to `/audit-publish`'s discretion given the closed
  issue's specific wording ("re-measure... rather than trusting this line").
- **Suggested Fix**: Split the crate docstring's REGN bullet into two:
  "REGN ambient background music (`music` field) — shipped" moved into a
  new phase block alongside a one-line pointer to
  `byroredux/src/asset_provider/audio.rs::dispatch_region_ambient_music`,
  and "REGN `incidental`/`sounds` ambient-loop selection — pending (blocked
  on `chance_raw` fixed-point scale)" staying in Future work. Flip
  `feature-matrix.md:146`'s `✗` to `~ Partial` with the same one-line
  split. Update `ROADMAP.md:705`'s trailing pending-clause the same way,
  and refresh the test-count sentence's `11`/`46` to `12`/`60` (or better,
  drop the specific numbers from ROADMAP prose entirely in favor of the
  sentence's own advice to run the two `cargo test` commands — the closing
  line already argues for this; the M44 row should take its own advice).

---

## Disproved candidates (investigated, not reported)

Recorded so the next cycle doesn't re-derive them.

- **Exterior REGN ambient dispatch racing the interior dispatch across a
  door transition.** Hypothesis: `apply_cell_region_ambient` runs
  unconditionally every `step_streaming` tick (outside the grid-changed
  guard), so a door walk from an exterior worldspace into an interior cell
  might leave the exterior tick still firing on a stale/meaningless
  `player_grid` derived from the interior camera pose, fighting the
  interior's own `dispatch_region_ambient_music` call for the last write to
  `RegionAmbientRes` / the `AudioWorld` music slot. **Disproved**: traced
  `step_cell_transition`'s `Interior` destination arm
  (`app_step.rs:722-770`) — it calls `drain_streaming_state` (which does
  `streaming_slot.take()`, `streaming_helpers.rs:372`) **before** the
  interior load, setting `self.streaming` to `None`. `step_streaming`'s
  first line is `if self.streaming.is_none() { return; }`
  (`app_step.rs:43-45`), so the exterior tick — and therefore
  `apply_cell_region_ambient` — cannot run again until a subsequent
  `begin_exterior_streaming` call re-populates `self.streaming`. The two
  dispatchers are structurally mutually exclusive, not racing.
- **REGN ambient music restarting every frame on a stationary player near a
  cell/tile boundary.** Both call sites (`cell_loader/load.rs`,
  `scene/world_setup.rs::apply_cell_region_ambient`) compare against
  `RegionAmbientRes`'s value *before* the write, and `apply_cell_region_ambient`
  additionally short-circuits entirely (`return`) on `previous == Some(ambient)`
  before even reaching the dispatch call. **Disproved** — a stationary
  player, or one crossing repeatedly between two tiles sharing the same
  winning REGN `Sound` entry, produces zero redundant `play_music`/`stop_music`
  calls per tick.
- **`dispatch_region_ambient_music` panicking or leaking a stale track when
  `--sounds-bsa` is supplied but the resolved SOUN path isn't in the
  archive.** Traced the failure branch: `provider_present = true`,
  `bytes = None` → logs a WARN (correctly distinguishing "no archive" from
  "archive present, file missing," per the function's own doc) → calls
  `stop_region_ambient_music`, which `stop_music`s the *previous* cell's
  track rather than leaving it playing into a cell whose directive can't be
  honored. **Disproved as a defect** — this is the documented, tested
  behaviour (`dispatch_with_unresolvable_form_id_stops_playback`), and the
  audit agrees it's the right contract (silence over a wrong cell's stale
  ambient bed).
- **`RegionAmbientRes::resolve`'s per-frame REGN `Sound`-entry sort as a
  perf regression** (`apply_cell_region_ambient` calls `resolve` — which
  sorts a `Vec<&RegionDataEntry>` — unconditionally every `step_streaming`
  tick, not gated behind the grid-changed check). **Not escalated**: the
  function's own doc explicitly makes this tradeoff ("cheap enough that
  gating the *resolve* isn't worth the extra state"), the candidate list is
  bounded by how many `REGN` polygons tag one cell (single digits in every
  observed corpus cell), and a `HashMap::get` plus a small stable sort is
  far cheaper than the LOD/streaming work already running unconditionally
  in the same function on the same tick. Below the audit's reporting bar —
  noted here so a future perf pass doesn't have to re-derive the
  "acceptable, by design" conclusion.
- **`kira::Mix` semantics for `apply_underwater_filter`'s dry bypass** —
  re-verified against the crate's own regression test rather than
  independently re-deriving kira internals a second time (last cycle
  already worked the `tan(π·clamp(...))` degeneracy math for the
  Nyquist-adjacent alternative and rejected it): `above_water_filter_state_is_a_dry_bypass`
  and `above_water_cutoff_never_reaches_the_kira_clamp_ceiling` both pass
  (`cargo test -p byroredux-audio`), and the fix commit's own message
  documents the exact `a1` collapse math that ruled out the alternative.
  No new evidence surfaced to reopen this.

---

## Future-Phase Readiness (invariants pinned for the next phase)

- **FOOT / 3.5b (per-material footstep sound)**: `FootstepConfig.default_sound`
  decoupling, `FootstepScratch` Vec reuse, the `{min, max}` attenuation
  shape, and — as of this cycle — the BU→metre unit seam all survive and
  are no longer blocked (#3178 fixed the unit bug that would have made NPC
  footsteps silent the moment they stopped being co-located with the
  listener).
- **REGN `incidental`/`sounds`**: the `music` half is done; `incidental`
  (RDSI) and the chance-based `sounds: Vec<RegionSound>` list remain
  explicitly deferred pending a verified `chance_raw` fixed-point scale
  (no-guessing policy). `RegionAmbientRes` already carries `incidental_form`
  as a field with no consumer — the next session can wire it without a
  resource-shape change. Whether `incidental` wants the spatial
  `AudioEmitter` path (a real point-source, not the non-spatial main track
  `music` uses) is an open design question the REGN commit message flags
  explicitly, not attempted here.
- **MUSC (`ZNAM`/`XCMO` cell-level music)**: still zero consumers — REGN's
  `play_music` caller is a structurally different mechanism (region-keyed,
  not cell-`ZNAM`-keyed) and does not retire this gap. The eventual MUSC
  caller inherits the same single-slot/main-track/streaming/change-guard
  invariants REGN just proved out, plus the two constraints the 2026-08-20
  report already recorded (gate on FormID equality; decide explicitly
  whether music should be low-passed underwater — REGN doesn't touch this
  either, since `update_underwater_filters` walks `active_sounds` only and
  music lives on its own field on the main track).
- **Occlusion attenuation**: `apply_reverb_send` and `apply_underwater_filter`
  remain the per-track effect-chain seam a future raycast occlusion low-pass
  can join without touching either dispatch path — unchanged this cycle.
- **`SoundCache` producer**: still zero consumers; REGN doesn't change this
  count (it's a streaming load, never a `SoundCache` candidate in the first
  place — `SoundCache` only ever held `StaticSoundData`). #3189's scope
  grew (a third `--sounds-bsa` opener landed) without closing; still open.

---

## Delta vs prior report

This report supersedes `AUDIT_AUDIO_2026-08-20.md`. That cycle closed with
one HIGH, one MEDIUM, and four LOW (six new findings); every one of them —
plus the un-issue-numbered ripple-mismatch defect found in the same cycle
— is fixed and verified fixed at HEAD `048a8bd8`, not merely claimed fixed
by commit message. This is the first `/audit-audio` cycle in the project's
history to open with zero carried-over defects from its immediately
preceding report.

This cycle:

- Independently re-verified all four fixes in `5ce2b1c5` (#3178/#3179/#3180/
  #3181) and the ripple-cadence fix in `f8cfd185` by reading the current
  source and, where a regression test exists, running it — not by trusting
  the fix commit's own message.
- Audited the first genuinely new gameplay-audio producer since the water
  work: REGN ambient background music (`ede48ffb`/`3ef05d1b`). Traced the
  full FormID → path → archive → decode → `play_music` chain across all
  four call sites, the change-guard logic at each, the exterior/interior
  transition boundary for a dueling-writer race (disproved), and the
  `NOT_SAVED_BY_DESIGN` save-registry classification for the two new
  resources (`RegionAmbientRes`, `SoundArchiveProvider`). Found no defect
  in the feature itself.
- Surfaced one LOW: the REGN ship reopened the exact documentation-drift
  class #3181 had just closed for water, one layer over — all three
  authoritative status sources (crate docstring, `feature-matrix.md`,
  `ROADMAP.md`) still describe REGN ambient as entirely unbuilt, and the
  ROADMAP's audio test-count sentence has now drifted a fourth time in nine
  days (measured, then immediately invalidated by the same day's ripple-test
  addition, with the REGN test suite never counted at all).
- Ran the audio test suites directly rather than trusting prose: crate 29
  passed / 6 ignored / 0 failed; `systems::audio` 12 passed / 0 failed;
  `asset_provider::audio` 13 passed / 0 failed. True total across the three
  files: 60 tests, not the 46 the ROADMAP currently states.
- Re-verified every #842–#3180 regression guard structurally; none have
  drifted. Headless boot remains PASS.

---

## Severity Counts

- **CRITICAL**: 0
- **HIGH**: 0
- **MEDIUM**: 0
- **LOW**: 1 (NEW: AUD-2026-08-24-D6-01) · carried Existing: #3086, #3087,
  #3189

TALLY: CRITICAL=0 HIGH=0 MEDIUM=0 LOW=1
