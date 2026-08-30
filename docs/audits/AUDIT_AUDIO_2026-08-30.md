# Audio Subsystem Audit (M44) — 2026-08-30

- **Command**: `/audit-audio` → all 7 dimensions, `--depth deep`
  (run as part of an `/audit-suite --preset comprehensive` sweep;
  **single-agent by explicit task constraint** — every dimension re-derived
  in-process, no sub-agent fan-out, per the relay hazard recorded in
  `feedback_audit_suite_nested_agent_relay`)
- **Branch**: main · **HEAD**: `64f64480`
- **kira**: pinned `0.10` (workspace `Cargo.toml:176`, unchanged) → resolved
  `kira-0.10.8`
- **Method**: files read in full or in paginated ranges —
  `crates/audio/src/lib.rs` (1510 lines), `crates/audio/src/tests.rs` (1420),
  `byroredux/src/systems/audio.rs` (768),
  `byroredux/src/asset_provider/audio.rs` (351),
  `byroredux/src/asset_provider/texture.rs:95-188`,
  `byroredux/src/components.rs` (audio + `RegionAmbientRes` sections),
  `byroredux/src/boot.rs` (resource wiring + the `PostUpdate`/`Late`
  registrations), `byroredux/src/scene.rs:1195-1235`,
  `byroredux/src/scene/world_setup.rs:515-542`,
  `byroredux/src/cell_loader/load.rs:536-561`,
  `byroredux/src/ownership_sample.rs`,
  `crates/core/src/ecs/sparse_set.rs`, and the vendored `kira-0.10.8` sources
  (`sound/streaming/{data,settings,handle}.rs`,
  `sound/static_sound/data.rs`, `tween/tweenable.rs`).
- **Tests actually run** (not trusted from prose):
  `CARGO_BUILD_JOBS=4 cargo test -q -p byroredux-audio` →
  **29 passed, 0 failed, 6 ignored**. **Headless-mode boot: PASS.**
  The two engine-side audio test modules were **not** executed this cycle —
  the suite ran under a hard memory constraint (29 GB box, ~8 GB available,
  9 GB into swap, an earlier audit in this sweep OOM-killed), and
  `cargo test -p byroredux --bin byroredux` links the whole engine binary.
  Their counts were verified **statically** instead (12 `#[test]` in
  `systems/audio.rs`, 13 in `asset_provider/audio.rs`, 35 in the crate =
  60 total, matching `ROADMAP.md:718`).
- **Dedup baseline**: fresh `gh issue list --repo matiaszanolli/ByroRedux
  --limit 300 --json number,title,state,labels` (pulled this cycle — the
  scratch dir carried an Aug 28 `issues.json` from a previous, OOM-aborted
  run, which was discarded along with its four stale `d*.md` fragments),
  plus targeted `--state all --search` pulls for the AUD-2026-08-27 finding
  IDs, plus the full prior `docs/audits/AUDIT_AUDIO_*.md` chain (14 reports,
  2026-05-05 → 2026-08-27).

---

## Delta Analysis (since `AUDIT_AUDIO_2026-08-27.md`, HEAD `969d81c8`)

45 commits landed on `main` in the window. Restricting to the files this
audit owns:

| Path | Commits in window | Audio-relevant? |
|---|---|---|
| `crates/audio/` | **0** | — |
| `byroredux/src/systems/audio.rs` | **0** | — |
| `byroredux/src/asset_provider/audio.rs` | **0** | — |
| `byroredux/src/components.rs` | 3 (`265f0c9b` Disable() consumer, `b0a30fd5` VWD-cull doc, `83a81da8` doc/dead-code) | No — none touch the audio, footstep, or REGN-ambient sections |
| `byroredux/src/boot.rs` | 1 (`9812285c` FxHash skinning scratches) | No — 4 lines, outside the audio registrations |
| `byroredux/src/asset_provider/texture.rs` | 1 (`6b939189` #3334 one texture cache key) | No — diff does not reach `try_load_default_footstep` / `try_load_default_water_splash` |

**The audio subsystem's executable surface is byte-identical to the
2026-08-27 audit**, which was itself byte-identical to 2026-08-24. This is
the third consecutive cycle with no code change in scope. Accordingly this
cycle deliberately spent its budget on ground that prior cycles had covered
thinnest — the REGN music **consumer** (shipped 2026-08-23, audited twice,
never traced past the `play_music` call), and the CLI-arity contract shared
between the three `--sounds-bsa` parsers — rather than re-deriving the
invariant matrix a fourth time. Both new MEDIUMs came from that ground.

### Verification of last cycle's findings

All four AUD-2026-08-27 findings were published and remain **OPEN**; all
four were re-verified as still present at HEAD by direct inspection, not
inherited:

| Issue | Severity | State at HEAD `64f64480` |
|---|---|---|
| **#3520** `stride_threshold` metres-vs-BU (footsteps fire once per frame) | HIGH | **Present.** `components.rs` still defaults `1.5` with the "~1.5m at FNV scale" docstring; `systems/audio.rs:161-171` still compares it against the raw BU delta. |
| **#3521** `mem::take` strands `pending_oneshots` heap capacity | LOW | **Present.** `lib.rs:979`, no capacity write-back. |
| **#3522** `reverb_zone_system` doc still says *main.rs* + wrong ordering mechanism | LOW | **Present.** `systems/audio.rs:56-58`. |
| **#3523** two secondary status sites still call REGN ambient music unbuilt | LOW | **Present.** `ROADMAP.md:1121` still lists "REGN region-keyed ambient layers" as pending while `ROADMAP.md:718` carries the shipped clause. |

Also carried and skipped per the dedup protocol: **#3086** (entity-path
sub-track position frozen at dispatch — still latent, entity path still has
zero engine producers), **#3189** (duplicated `--sounds-bsa` scan / triple
`Archive::open` — still three openers), **#3301** / **#2372** (REGN
`incidental` + non-`Sound` RDAT kinds — future-phase).

**Stale candidates dropped: 0 filed, 4 pre-checked.** Every finding below
had its premise re-derived from live source before writing; the four
carried-open issues above were each re-read against HEAD rather than
assumed, and no candidate was discarded as stale this cycle because the
code did not move. The three doc/status sites that *had* drifted in prior
cycles and are now correct (the `try_load_default_footstep` path in the
crate docstring — #1859; the three primary REGN status sites — #3274; the
`ROADMAP.md:718` test counts) were re-checked and confirmed **clean**, and
are explicitly **not** re-flagged.

---

## Executive Summary

**7 dimensions run. 4 NEW findings (0 CRITICAL / 0 HIGH / 2 MEDIUM /
2 LOW).**

| # | Dimension | NEW findings |
|---|---|---|
| 1 | Spatial Sub-Track Lifecycle & Leaks | **0** |
| 2 | Listener Pose & Attenuation | **0** |
| 3 | SoundCache Growth & Eviction | **0** |
| 4 | Streaming Music Lifecycle | **2** (1 MEDIUM, 1 LOW) |
| 5 | Reverb Send & Routing | **1** (LOW) |
| 6 | Manager Lifecycle, ECS & Cell Streaming | **0** |
| 7 | Gameplay Audio Wiring | **1** (MEDIUM) |

**Dimensions 1, 2, 3 and 6 produced no findings.** All of their checklist
invariants were re-derived from live source and hold; the per-dimension
verification tables are reproduced in the matrix below.

**The headline finding is that the one shipped REGN audio feature stops
working after one track length.** `dispatch_region_ambient_music` resolves a
region's background-music FormID, extracts it, decodes it as a streaming
sound and hands it to `AudioWorld::play_music` — which never sets a loop
region (kira's default is `loop_region: None`). Nothing polls the handle and
nothing re-dispatches while the player stays inside the region, because the
only trigger is a *change* in `RegionAmbientRes.music_form`. A region's
ambient bed is therefore audible exactly once per region entry and then
silent for the rest of the visit. Details in AUD-2026-08-30-D4-01, which
deliberately stops short of prescribing the fix: the `SNDD`/`SNDX` flag word
that would say whether a given SOUN is authored as a loop is **not parsed**,
so choosing a continuation policy without it would be guesswork.

The second MEDIUM is a CLI-arity split: `--sounds-bsa` is repeatable for the
REGN provider (and documented as such) but first-match-only for the footstep
and splash loaders, so the documented mod-override ordering silently
disables both (AUD-2026-08-30-D7-01).

- **Headless-mode boot**: **PASS** — 29 default tests green in
  `byroredux-audio`, 6 device/data-gated `#[ignore]`d, 0 failing;
  `audio_world_constructs_without_panic_on_any_environment` and
  `audio_system_no_op_when_audio_world_inactive` both green.
- **Shipped surface, re-confirmed at HEAD**: `AudioWorld` graceful
  degradation with no `unwrap()` on the manager `Option`
  (`SUB_TRACK_CAPACITY = 512` / `SEND_TRACK_CAPACITY = 32`, both applied in
  `new()`); `AudioListener` / `AudioEmitter` / `OneShotSound`;
  `audio_system` = `sync_listener_pose` → `update_underwater_filters` →
  `drain_pending_oneshots` → `dispatch_new_oneshots` →
  `prune_stopped_sounds`; both dispatch paths (queue `VecDeque` cap 256
  `pop_front`; entity path with `loop_region(..)`); tweened-`stop()` despawn
  truncation with `stop_issued` debounce; single-slot main-track streaming
  music with one live caller; global reverb send (`NEG_INFINITY` dry
  default); the `bu_to_audio_space` unit seam; the `Mix::DRY` underwater
  bypass.
- **Live engine consumers remain three**: `footstep_system`
  (`play_oneshot`), `water_audio_system` (`play_oneshot` +
  `set_underwater`), `dispatch_region_ambient_music` (`play_music` /
  `stop_music`). `reverb_zone_system` remains the only
  `set_reverb_send_db` caller. The **entity dispatch path
  (`spawn_oneshot_at` / `AudioEmitter` / `OneShotSound`) still has zero
  engine callers** — re-confirmed by grep across `byroredux/src`; its only
  non-test mentions are two `save_io/registry_completeness_tests.rs`
  exclusion rationales.
- **Pending phases (correctly unbuilt, not flagged)**: 3.5b FOOT records →
  per-material sound; REGN `incidental` / `sounds`; MUSC + hardcoded music
  routing; per-cell acoustics beyond binary interior/exterior; raycast
  occlusion attenuation.
- **MUSC parse→play gap, explicitly**: cell-music FormIDs **are** parsed
  (`default_music`/ZNAM and `music_type_form`/XCMO on `CellData`) and read by
  nothing. `dispatch_region_ambient_music` is the tree's only `play_music`
  caller. The single-slot / main-track / streaming-type invariants are pinned
  below for the eventual caller, which will have to arbitrate against REGN
  music for the one slot.

---

## Lifecycle Invariant Matrix

Owned by Dimension 6 per the skill's dedup instruction (Dims 1/4/5 point
here). Every row re-derived from live source this cycle.

| Invariant | State | Anchor |
|---|---|---|
| `AudioWorld` field-declaration = drop order: `active_sounds` → `pending_oneshots` → `music` → `reverb_send` → `reverb_send_db` → `listener` → `manager` → `multi_listener_warned` → `underwater` | **HOLDS** | `lib.rs:377, 384, 389, 396, 400, 404, 407, 413, 416` |
| `ActiveSound` field order (`entity` → `handle` → `_track` → `underwater_filter` → `underwater` → `unload_fade_ms` → `stop_issued`) | HOLDS | `lib.rs:324, 325, 326, 329, 330, 337, 346` |
| `ActiveSound._track` underscore name intact, Drop-side-effect only; track lands in `active_sounds` before the helper returns | HOLDS | `lib.rs:326`; pushes at `1020` (queue) / `1175` (entity) |
| Graceful degradation — manager init failure logs WARN and leaves `None`; **zero `unwrap()`/`expect()` on the manager `Option`** | HOLDS | `lib.rs:438-455` |
| Manager capacities exceed kira defaults (512 / 32) and `new()` applies them (#842) | HOLDS | consts `lib.rs:306-307`, applied `432-433` |
| `AudioWorld::new()` called exactly once, at boot — never on cell transition or resize | HOLDS | single call site `boot.rs:518` |
| Lazy listener creation; no frame-1 cold-start panic; `add_listener` failure is transient-retry (leaves `None`, next frame re-enters lazy branch) | HOLDS | `lib.rs:920-946` |
| Sticky listener, never cleared on entity churn (#849) — only write site is the lazy create | HOLDS | `lib.rs:942`; no clear site exists |
| Multi-listener diagnostic debounced (#843) | HOLDS | `multi_listener_warned` set once at `lib.rs:917` |
| **Multi-listener "first wins" is deterministic, not hash-ordered** — `SparseSetStorage::iter` zips two insertion-ordered `Vec`s | HOLDS (newly verified this cycle) | `crates/core/src/ecs/sparse_set.rs:158-160` |
| **Listener orientation tween is a slerp, not a component lerp** — a fast camera whip cannot drive kira through a denormalised quat | HOLDS (newly verified this cycle) | `kira-0.10.8/src/tween/tweenable.rs:34-36` |
| Listener orientation is already in kira's frame (ears at ±X, forward −Z) — no residual Z-up→Y-up inversion | HOLDS (verified 2026-08-27, unchanged) | `kira-0.10.8/src/track/sub.rs:347-366`; `systems/camera.rs:79-86` |
| **Listener entity and submersion source are the same entity** — `AudioListener` and `ActiveCamera(cam)` are both set from `cam` | HOLDS (newly verified this cycle) | `scene.rs:1217`, `1230`; consumed `systems/audio.rs:228-237` |
| Spatial positions cross the BU→metre seam at every kira site (#3178) | HOLDS | `bu_to_audio_space` `lib.rs:200-202`; calls at `934`/`949`/`1002`/`1136`; pinned by `every_kira_position_site_goes_through_the_unit_seam` |
| **Gameplay *producer* constants compared against a BU delta are in BU** | **DRIFTED — carried #3520** | `components.rs` `stride_threshold: 1.5` vs `systems/audio.rs:161-171` |
| Attenuation `RangeInclusive`, `min<=max` normalized (#1612), NaN-safe | HOLDS | `Attenuation::distance_range()` `lib.rs:726-730` |
| Both dispatch paths gate on `listener_id` before any work | HOLDS | `lib.rs:960` / `1048` |
| Both dispatch paths apply the reverb gate through one shared helper (#2405) | HOLDS | `lib.rs:992` / `1126` → `apply_reverb_send` `267-282` |
| Both dispatch paths build the underwater filter through one call site (#3179); above-water state is a genuine `Mix::DRY` bypass | HOLDS | `apply_underwater_filter` `lib.rs:217-226`, called `999`/`1133`; `underwater_mix` `241-247` |
| `looping` / `loop_region(..)` on the entity path only; `PendingOneShot` has no `looping` field | HOLDS | `lib.rs:1156-1162`; struct `350-355` |
| Volume→dB centralized (`linear_volume_to_db`), three call sites, no inlined copy | HOLDS | `lib.rs:253-259`, called from `598`, `1011`, `1154` |
| No PCM deep-clone — `StaticSoundData.frames` is `Arc<[Frame]>`, so `(*sound).clone().volume(db)` copies a 4-field header | HOLDS | `kira-0.10.8/src/sound/static_sound/data.rs:33` |
| Drain cap (>32/tick warns) · producer cap (256 `pop_front`) · drain-gate-before-`mem::take` (#851/#852/#853) | HOLDS | `lib.rs:538-559` (producer cap), `966-978` (drain gate + cap warn) |
| **`pending_oneshots` heap capacity survives a drain** | **DRIFTED — carried #3521** | `lib.rs:979` |
| `OneShotSound` marker consumed on success **and** both failure arms (#2394) | HOLDS | `lib.rs:1146`, `1171`, `1184` (`consumed.push` on success + both failure arms) |
| Despawn truncation with `stop_issued` debounce (#844/#845/#858/SAFE-23): emitter-presence test, stop-then-mark, `retain` only on `Stopped`, `AudioEmitter` removed on completion | HOLDS on all four | `lib.rs:1231-1235`, `1256`, `1260-1272`, `1274-1280` |
| Queue-driven sounds (`entity == None`) exempt from truncation | HOLDS | `lib.rs:1225-1227` |
| `SoundCache` lowercase-once at all three key sites; dormant in engine (#859/#850) | HOLDS — `len()` wired to telemetry (`ownership_sample.rs:70-73`), `bytes_estimate` correctly documented unwired | `lib.rs:1425`, `1433`, `1449` |
| `SoundCache::clear()` cannot invalidate an `Arc` a live `ActiveSound` depends on | HOLDS | `lib.rs:1484-1486` vs `1011`/`1154` |
| Single-slot music · main track · streaming types · fade-then-drop (handle drop does not cut the fade — no `impl Drop` on `StreamingSoundHandle`) | HOLDS | `lib.rs:579-637` |
| **Music playback continues past one track length** | **DRIFTED — AUD-2026-08-30-D4-01** | `lib.rs:598-610`; `kira-0.10.8/src/sound/streaming/settings.rs:37` |
| **`is_music_active()` matches its documented "playing or fading out" contract** | **DRIFTED — AUD-2026-08-30-D4-02** | `lib.rs:613-637` |
| Reverb `None`-safe · `NEG_INFINITY` default · `is_finite() && > SILENCE_DB` gate | HOLDS | `lib.rs:267-286`, `486` |
| **Reverb effect parameters carry a rationale or a source** | **DRIFTED — AUD-2026-08-30-D5-01** | `lib.rs:463-469` |
| `audio_system` registered `add_exclusive(Stage::Late, ...)`; body order matches its 5-pass docstring | HOLDS | `boot.rs:1481`; `lib.rs:845-849` vs doc `800-836` |
| `reverb_zone_system` runs before `audio_system` in `Stage::Late` (parallel batch drains before the exclusive list) | HOLDS (but the in-file comment still states the wrong mechanism — carried #3522) | `boot.rs:1411` vs `1481` |
| Late-stage exclusive order: water_damage → reconcile_dead → water_interaction → water_audio → audio_system | HOLDS | `boot.rs:1421-1481` |
| `footstep_system` (`PostUpdate` exclusive) enqueues; `audio_system` (`Late`) drains — same frame | HOLDS | `boot.rs:1115`, `1481` |
| `FootstepScratch` capacity restored on the success path **and** the `AudioWorld`-absent bail (#932); the third exit returns with the guard still live | HOLDS | `systems/audio.rs:177-179`, `192-194`, `215-217` |
| No component-query lock held across `play_oneshot`; `FootstepScratch` mut-guard dropped before `AudioWorld` acquired | HOLDS | `systems/audio.rs:145-185` |
| `reverb_zone_system` bit-equality transition gate + both safe no-ops | HOLDS | `systems/audio.rs:59-88` |
| REGN ambient dispatch is change-guarded against the resource's *prior* `music_form` at both call sites | HOLDS | `cell_loader/load.rs:552-561`; `scene/world_setup.rs:534-541` |
| **`--sounds-bsa` arity is consistent across its three parsers** | **DRIFTED — AUD-2026-08-30-D7-01** | `texture.rs:100-110`, `149-155` vs `asset_provider/audio.rs:104-124` |
| Crate docstring's `try_load_default_footstep` path matches the live file location (#1859) | HOLDS — `lib.rs:1394` cites `asset_provider/texture.rs`; function is at `texture.rs:100` | — |
| `ROADMAP.md:718` audio test counts (29+6 / 12 / 13 / 60) | HOLDS — 29+6 re-measured, 12/13/35 counted statically | — |

---

## Findings

### AUD-2026-08-30-D4-01: REGN ambient background music is dispatched without a loop region and has no re-trigger, so a region's ambient bed plays exactly once and then goes permanently silent

- **Severity**: MEDIUM
- **Dimension**: Streaming Music Lifecycle
- **Location**: `crates/audio/src/lib.rs:579-611` (`AudioWorld::play_music`),
  `byroredux/src/asset_provider/audio.rs:192-205`
  (`dispatch_region_ambient_music`), guards at
  `byroredux/src/cell_loader/load.rs:552-561` and
  `byroredux/src/scene/world_setup.rs:534-541`
- **Status**: NEW
- **Description**: `play_music` hands `mgr.play(...)` a `StreamingSoundData`
  on which only `.volume(db)` and `.fade_in_tween(..)` have been set. kira's
  `StreamingSoundSettings::default()` is `loop_region: None`, so the track
  plays through once and stops. Nothing restarts it:
  `dispatch_region_ambient_music` is invoked **only** when `music_form`
  differs from the previously-installed `RegionAmbientRes.music_form`, and a
  player who stays inside one region never changes that value. There is no
  polling of `is_music_active()` anywhere in the tree, and no
  `set_loop_region` call anywhere in the workspace.

  Observable behaviour on the reference title: walk into a REGN-tagged
  exterior, hear the ambient bed once, then silence for the remainder of the
  visit; the bed returns only after crossing into a differently-scored
  region and back. This is the *entire* shipped REGN audio feature (the
  2026-08-23 `ede48ffb`/`3ef05d1b` work, marked `✓` at
  `docs/feature-matrix.md:148`), so the observable failure is that a feature
  the matrix reports as complete works for one track length per region entry.
- **Evidence**: the whole configuration applied before play
  (`crates/audio/src/lib.rs:598-610`):
  ```rust
  let db = linear_volume_to_db(volume);
  let configured = streaming_sound.volume(db).fade_in_tween(Some(fade));
  match mgr.play(configured) {
      Ok(handle) => { self.music = Some(handle); }
      Err(e) => { log::warn!("M44 Phase 5: play_music failed: {e}"); self.music = None; }
  }
  ```
  kira's default, from the vendored crate
  (`kira-0.10.8/src/sound/streaming/settings.rs:37`): `loop_region: None`.
  The builder that is never called:
  `kira-0.10.8/src/sound/streaming/data.rs:106`
  `pub fn loop_region(mut self, loop_region: impl IntoOptionalRegion) -> Self`.
  A workspace-wide grep confirms the project never touches it on the music
  path — `grep -rn "loop_region\|set_loop_region" crates/ byroredux/` returns
  six hits, all on the **entity/static** path
  (`crates/audio/src/lib.rs:1161` plus its docstrings at `68`, `320`, `1157`,
  and one test assertion at `tests.rs:632/645`).

  The change guard, verbatim (`byroredux/src/cell_loader/load.rs:552-561`;
  `scene/world_setup.rs:534-541` is the same shape and the only other call
  site):
  ```rust
  let previous_music_form = world
      .try_resource::<crate::components::RegionAmbientRes>()
      .and_then(|r| r.music_form);
  if previous_music_form != region_ambient.music_form {
      crate::asset_provider::dispatch_region_ambient_music(
          world, &index.sounds, region_ambient.music_form);
  }
  ```
  `music_form` is the REGN `RDMD` (Oblivion) / `RDMO` (Skyrim) / `RDSB`
  (FNV) background-music FormID (`byroredux/src/components.rs:540-542`) —
  the field whose purpose is a continuous bed, not a stinger.
- **What this finding does NOT claim**: it does **not** prescribe
  `loop_region(..)` as the fix, and no replacement value or policy is
  proposed here. `SounRecord` carries only `{form_id, editor_id,
  sound_path}` — the `SNDD`/`SNDX` flag word that would tell the engine
  whether a given SOUN is authored as a looping bed is **not parsed**, so
  "loop it", "restart on a timer", or "the vanilla asset is already a long
  pre-looped file and the real bug is elsewhere" cannot be distinguished
  from the data the engine currently has. Per the project's no-guessing
  rule, the continuation policy must be settled against the SOUN flag layout
  (or a corpus census of REGN-referenced SOUN durations) before a value is
  chosen. **The defect being reported is the absence of *any* continuation
  mechanism**, which is verifiable from the code alone.
- **Not covered by an open issue**: #3301 ("EX-16 items 1+5 remainder")
  scopes itself explicitly to `incidental`, the non-`Sound` RDAT kinds, and
  the `sounds` chance list, and opens by asserting that "REGN-driven ambient
  audio is wired end-to-end for exactly one field: `music`". #2372 is the
  parent umbrella. Neither mentions playback continuation.
- **Recommendation**: first parse the SOUN flag word (or census the
  referenced assets) to establish the intended semantics; only then wire the
  continuation. Whichever policy wins, add a regression test alongside
  `dispatch_with_no_music_form_stops_playback_without_panic` that asserts the
  *post-track-end* state, since none of the 13 existing
  `asset_provider::audio` tests observe playback past the dispatch call.

---

### AUD-2026-08-30-D7-01: the footstep and splash loaders consult only the **first** `--sounds-bsa`, while the same flag is repeatable for the REGN provider — the documented mod-override ordering silently disables both

- **Severity**: MEDIUM
- **Dimension**: Gameplay Audio Wiring
- **Location**: `byroredux/src/asset_provider/texture.rs:100-110`
  (`try_load_default_footstep`) and `:149-155`
  (`try_load_default_water_splash`), against
  `byroredux/src/asset_provider/audio.rs:104-124`
  (`build_sound_archive_provider`)
- **Status**: NEW
- **Description**: Three consumers parse the same `--sounds-bsa` flag out of
  `args`, and they disagree about its arity. `build_sound_archive_provider`
  walks the whole arg list and pushes **every** match into
  `SoundArchiveProvider.archives`, with first-hit-wins resolution at extract
  time — its own doc calls the flag *"repeatable (list override/mod archives
  before the vanilla one — first hit wins)"*
  (`asset_provider/audio.rs:54-57`), and
  `docs/engine/exterior-readiness-plan.md:1197` records the same contract.
  The two one-off loaders take the **first** occurrence and stop.

  A user who follows that documented ordering — mod/override archive listed
  first, vanilla `Fallout - Sound.bsa` second — gets a `SoundArchiveProvider`
  that resolves REGN ambient music out of either archive, but a
  `FootstepConfig.default_sound` and a `WaterAudioConfig.splash_sound` that
  are both `None`, because the canonical
  `sound\fx\fst\dirt\walk\left\fst_dirt_walk_01.wav` and the three splash
  candidates live only in the vanilla archive that was never opened.
  Footsteps and water splashes then no-op for the whole session
  (`footstep_system` returns at `systems/audio.rs:123`; `water_audio_system`
  at `:245`), leaving only a one-line boot WARN — *"'<mod.bsa>' missing
  canonical footstep '<path>'"* — that reads as a bad archive rather than as
  a flag-arity mismatch.
- **Evidence**: `try_load_default_footstep`, verbatim
  (`texture.rs:101-110`):
  ```rust
  let mut path: Option<&str> = None;
  let mut i = 0;
  while i < args.len() {
      if args[i] == "--sounds-bsa" {
          path = args.get(i + 1).map(|s| s.as_str());
          break;
      }
      i += 1;
  }
  let Some(path) = path else { return };
  ```
  `try_load_default_water_splash` (`texture.rs:149-155`) uses a different
  spelling of the same first-match semantics:
  ```rust
  let Some(path) = args
      .windows(2)
      .find(|pair| pair[0] == "--sounds-bsa")
      .map(|pair| pair[1].as_str())
  else { return; };
  ```
  versus `build_sound_archive_provider` (`asset_provider/audio.rs:106-123`),
  which has no `break` and pushes each successfully-opened archive onto a
  `Vec`.

  The documentation is split the same way: `docs/engine/game-loop.md:55`
  lists the flag as `--sounds-bsa PATH` (singular), while
  `docs/engine/exterior-readiness-plan.md:1197` describes it as repeatable.
  Each is accurate about the consumer it was written for, which is exactly
  why the split has survived.
- **Distinct from #3189**: that issue is about the *duplication* of the scan
  and the repeated `Archive::open` of the **same** path — a
  cleanliness/boot-cost concern. This is a behavioural gap: the two loaders
  cannot see archives 2..n at all. The natural fix is the one #3189's own
  remediation note already proposes (migrate both one-off loads onto the
  persistent `SoundArchiveProvider`, which already iterates every archive),
  so the two should be worked together — but **a fix that only deduplicates
  the scan without switching to the multi-archive provider would close #3189
  and leave this defect standing.** Worth noting on #3189 when it is next
  picked up.
- **Recommendation**: settle the flag's arity in one place. Either make all
  three parsers repeatable (preferred — it is the contract the provider and
  the plan doc already state) or make the provider single-valued and correct
  `exterior-readiness-plan.md`. Then align `docs/engine/game-loop.md:55` with
  whichever wins.

---

### AUD-2026-08-30-D4-02: `is_music_active()`'s docstring promises "playing **or fading out**", but `stop_music` drops the handle, so it reports `false` for the whole fade tail

- **Severity**: LOW
- **Dimension**: Streaming Music Lifecycle
- **Location**: `crates/audio/src/lib.rs:626-637` (`is_music_active` + its
  doc), `613-630` (`stop_music`)
- **Status**: NEW
- **Description**: `stop_music` issues `handle.stop(fade)` and then sets
  `self.music = None` (`lib.rs:627-629`) precisely so a later `play_music`
  doesn't see a stale reference. But `is_music_active` is
  `self.music.as_ref().map(|h| !matches!(h.state(), Stopped)).unwrap_or(false)`
  — with the slot cleared it returns `false` immediately, for the entire
  fade-out during which kira is still rendering the tail. On the one live
  path that is `REGN_AMBIENT_CROSSFADE_SECS` = 3.0 s
  (`asset_provider/audio.rs:133`). The docstring immediately above says:

  > *True when music is currently playing or fading out. Useful for
  > menu-toggle / cell-load gameplay logic that wants to avoid stacking
  > music calls.*

  A caller that trusts that sentence as a "may I start a new track?" gate
  gets the opposite of the intended answer during the exact 3-second window
  the gate exists for.
- **Impact today is documentation-only**: `grep -rn "is_music_active"
  byroredux/ crates/` finds no non-test caller. Filed so the first
  MUSC/cell-music consumer — which will have to arbitrate against
  `dispatch_region_ambient_music` for the single slot — does not build on a
  false contract.
- **Recommendation**: either correct the doc to describe what the function
  actually reports ("true while a handle is installed and not yet
  `Stopped`; `stop_music` clears the slot immediately, so the fade tail
  reports false"), or keep a `stopping: bool` beside the slot. Both are
  cheap; the choice belongs with whoever wires the slot arbitration, and
  either way it should land with a test — neither `stop_music` nor
  `is_music_active` is currently covered by a default (non-`#[ignore]`d)
  test.

---

### AUD-2026-08-30-D5-01: the four `ReverbBuilder` parameters are unsourced bare literals — uniquely among the crate's tunables

- **Severity**: LOW
- **Dimension**: Reverb Send & Routing
- **Location**: `crates/audio/src/lib.rs:463-469`
- **Status**: NEW
- **Description**: The global reverb effect is built as
  ```rust
  ReverbBuilder::new()
      .feedback(0.85)
      .damping(0.6)
      .stereo_width(1.0)
      .mix(Mix::WET),
  ```
  Four magic numbers, inline, with no named constant, no rationale comment,
  and no cited source. They are the only tunables in the crate with none of
  the three. Every sibling constant carries at least a derivation:
  `SILENCE_DB` states the `log10` blow-up it clamps (`lib.rs:162-164`);
  `ABOVE_WATER_CUTOFF_HZ` carries a full derivation of kira's
  `g = tan(pi * clamp(f_c/f_s, 0.0001, 0.5))` and of why a "beyond-Nyquist"
  value would be numerically degenerate rather than transparent
  (`lib.rs:167-180`); `SUB_TRACK_CAPACITY` / `SEND_TRACK_CAPACITY` cite the
  ~400-emitter FO4 Diamond City Market figure and #842 (`lib.rs:296-307`);
  `DEFAULT_UNLOAD_FADE_MS` is pinned to `kira::Tween::default()` by a test
  (`tests.rs:20`). Even the consumer-side `INTERIOR_REVERB_SEND_DB = -12.0`
  carries a one-line justification (`systems/audio.rs:59-61`).
- **Why it matters**: these four values decide how every interior in every
  supported title sounds; they are invisible to `cargo test` (no test
  asserts on them — the five `reverb_tests` cover only the send **level**
  gate and its transitions); and they are not greppable as a tunable. The
  practical consequence is that the next person tuning interior acoustics —
  the per-cell acoustics work `set_reverb_send_db`'s own #847 note defers to
  — has no recorded baseline to move away from and no way to tell an
  authored choice from a copied example.
- **Deliberately NOT proposed**: replacement values. Nothing in the tree, in
  kira's docs, or in the Gamebryo 2.3 reference establishes what a Bethesda
  interior reverb should be, and inventing a plausible-sounding quadruple is
  exactly the failure the project's no-guessing rule exists to prevent.
- **Recommendation**: the remediable half is the hygiene half — promote the
  four to named `const`s beside `UNDERWATER_CUTOFF_HZ` and record whatever
  provenance the original author had, or state explicitly that they were
  chosen by ear. That converts an invisible magic number into an honest,
  greppable, revisitable one.

---

## Audio-thread / main-thread ordering note

Per the task's scoping instruction, the general lock-order and
worker-thread work is owned by the concurrency sibling audit and is not
re-derived here. Two audio-specific ordering facts, recorded for
cross-reference only, both **holding**:

- Everything in this subsystem runs on the main thread. kira owns its own
  audio render thread behind `AudioManager`, and the engine never touches it
  directly — every interaction is a handle method that writes into kira's
  command ringbuf. The `_track` / `handle` / `underwater_filter` /
  `listener` fields are command channels, not shared audio state, and the
  only cross-thread hazard the crate can create is a **Drop ordering** one:
  dropping the `AudioManager` while sub-track / send-track / listener
  handles are still live. That is exactly what the field-declaration order
  in `AudioWorld` (matrix row 1) and in `ActiveSound` (row 2) exists to
  prevent, and both hold at HEAD.
- `footstep_system` (`Stage::PostUpdate` exclusive) and
  `water_audio_system` / `reverb_zone_system` (`Stage::Late`) are all
  producers into `AudioWorld`, and `audio_system` (`Stage::Late` exclusive,
  registered last among the Late exclusives) is the sole consumer. The
  producer→consumer edge is same-frame in every case. `footstep_system`
  additionally drops its `FootstepScratch` resource-mut guard **before**
  acquiring `AudioWorld` (`systems/audio.rs:185-187`), so the
  TypeId-sorted multi-resource acquisition contract is never engaged from
  this subsystem — the one place it could have been.

One non-finding pointer, left for the concurrency audit: `audio_system` is
the only `Stage::Late` registration using bare `add_exclusive` rather than
`add_exclusive_with_access` (`boot.rs:1481` vs `1421`/`1437`/`1444`/`1454`).
This is legal — exclusive systems are not paired by the access analyzer, and
`boot.rs:1467-1480` documents the choice — but it does mean `audio_system`'s
footprint is invisible in the access report.

---

## Disproved candidates (investigated, not reported)

Recorded so the next cycle doesn't re-derive them.

- **Listener orientation tween lerping quaternion components** (a fast
  camera whip passing the listener through a denormalised quat, which would
  briefly collapse or invert the stereo image). **Disproved**: kira's
  `impl Tweenable for Quat` is `a.slerp(b, amount as f32)`
  (`kira-0.10.8/src/tween/tweenable.rs:34-36`). Never checked in any prior
  cycle; checked now.
- **Multi-listener "first wins" resolving through an arbitrary hash order**,
  so that during a two-listener camera transition the listener pose could
  flip between entities frame to frame. **Disproved**:
  `SparseSetStorage::iter` zips `dense`/`data`, two plain insertion-ordered
  `Vec`s (`crates/core/src/ecs/sparse_set.rs:158-160`), so the earliest
  inserted marker wins consistently — the policy #843's warn text documents.
- **Listener pose and underwater state tracking different entities.**
  **Disproved**: `AudioListener` is inserted only at `scene.rs:1217`, on the
  same `cam` binding that becomes `ActiveCamera(cam)` at `scene.rs:1230`;
  `water_audio_system` reads `SubmersionState` off `ActiveCamera.0`. Worth
  re-checking the moment M28.5 splits the listener onto a head joint, which
  `scene.rs:1213-1215` explicitly anticipates.
- **`FootstepScratch` capacity stranded on the third exit path.** The
  `scratch.triggers.is_empty()` early return (`systems/audio.rs:177-179`)
  has no capacity write-back, unlike the other two exits. **Disproved**: it
  returns while the resource guard is still live and before the
  `std::mem::take` at `:184`, so the `Vec` never left the resource.
- **`RIPPLE_COOLDOWN_SECS` decay starving when no splash sound is loaded.**
  `water_audio_system` returns at `:245` when `splash_sound` is `None`,
  before the `ripple_cooldowns` decay. **Disproved as reachable**: cooldown
  entries are only ever inserted after a ripple *plays*, which itself
  requires `splash_sound` to be `Some`, and `splash_sound` is written once
  at boot and never cleared. No path leaves stale cooldowns.
- **`play_oneshot`'s cap WARN spamming the log.** Undebounced, unlike the
  `multi_listener_warned` debounce #843 installed. **Not escalated**:
  reaching the 256 cap needs 256 calls inside one tick; the queue drains in
  full every tick and the two live producers emit at most one call per
  emitter per tick.
- **`dispatch_new_oneshots`' per-call `Vec` allocations** (`lib.rs:1068`,
  `1114`) as a sibling of #3521. **Not escalated**: `Vec::new()` does not allocate
  until pushed (and `Vec::with_capacity(pending.len())` is a no-op at
  length 0), and the entity dispatch path has zero engine producers, so
  neither `Vec` is ever populated. The right time to touch
  them is #3521's fix.
- **`SoundCache::get_or_load`'s `FnOnce() -> Vec<u8>` loader having no
  failure channel** (an archive miss must be expressed as an empty `Vec`,
  which then surfaces as "decode failed" rather than "not found"). **Not
  escalated**: an API wart on a dormant API with zero callers; the signature
  should change in the same commit that lands the first producer.
- **`EntityId` reuse fooling `prune_stopped_sounds`' emitter-presence
  test**, and **dropping the outgoing `StreamingSoundHandle` in
  `play_music` cutting the crossfade short**, and **`spawn_oneshot_at`
  leaking entities**. All three were disproved in the 2026-08-27 cycle
  against unchanged code (`World::despawn` never reclaims ids; kira has no
  `impl Drop` on `StreamingSoundHandle`/`StaticSoundHandle`/`FilterHandle`;
  the entity path has no producer). Re-listed rather than re-derived — the
  files are byte-identical.
- **`select_active_region_sound` picking a highest-priority `Sound` entry
  whose `music` is `None` and thereby suppressing a lower-priority entry
  that does carry music.** Still plausible on a multi-region-tagged FNV
  cell, still **not reported**: confirming it needs an ESM corpus census of
  REGN `Sound` entries whose winning priority carries `RDSI` but no `RDSB`,
  which this audit did not run, and filing it on the shape of the code alone
  would be the guessed-premise class the no-guessing policy forbids. Carried
  forward from 2026-08-27 as a cheap follow-up for whoever next has a census
  harness open.

---

## Future-Phase Readiness (invariants pinned for the next phase)

- **FOOT / 3.5b (per-material footstep sound)**: `FootstepConfig.
  default_sound` decoupling, `FootstepScratch` capacity reuse on both live
  paths, the metre-authored `{0.5, 12.0}` attenuation shape, and the
  BU→metre position seam all survive. **The two things 3.5b must not inherit
  are `stride_threshold`'s unit confusion (#3520) and the first-match-only
  `--sounds-bsa` scan (AUD-2026-08-30-D7-01)** — a FOOT-driven per-material
  lookup built on top of either will multiply the defect across every
  surface type instead of one. Fix both before 3.5b lands.
- **REGN `incidental` / `sounds`**: `incidental_form` is resolved into
  `RegionAmbientRes` and still has no consumer (#3301); the chance-based
  `sounds` list stays deferred on the unresolved `chance_raw` fixed-point
  scale. An `incidental` emitter belongs on the **spatial** path
  (`play_oneshot` / `AudioEmitter`), never on the single music slot — that
  boundary is pinned in the matrix. **The `music` half is shipped but not
  finished**: AUD-2026-08-30-D4-01 must be settled before `incidental`
  lands, or the same "plays once, then silence" shape will be copied onto a
  second field.
- **MUSC / ZNAM / XCMO cell music**: still zero consumers. `default_music`
  (ZNAM) and `music_type_form` (XCMO) are parsed into `CellData` and read by
  nothing. When a caller lands it will contend with
  `dispatch_region_ambient_music` for the **single** slot — that arbitration
  (cell music vs region music priority) is an unmade design decision, not a
  bug, and the single-slot invariant is pinned so it surfaces as a crossfade
  fight rather than stacked tracks. That caller is also the one that will
  reach for `is_music_active()` as its gate, which is why
  AUD-2026-08-30-D4-02 is filed now rather than left as a doc nit.
- **`SoundCache` first consumer**: still dormant, `len() == 0` steady state,
  `len()` wired to `ownership_sample.rs` telemetry and `bytes_estimate`
  correctly documented as unwired. Whoever wires the first producer should
  wire eviction and `bytes_estimate` in the same commit, give `get_or_load`'s
  loader a failure channel, and prefer migrating the three existing bypassing
  loaders (footstep, splash, REGN) onto it — which also closes #3189 **and**
  AUD-2026-08-30-D7-01 in one move.
- **Per-cell acoustics beyond binary interior/exterior**: unchanged.
  `set_reverb_send_db` remains a next-dispatch knob (#847), correctly
  documented as such; a real per-cell acoustic model needs the re-dispatch
  handler that limitation names, and it needs a defensible baseline for the
  reverb parameters themselves (AUD-2026-08-30-D5-01).

---

## Suggested next step

```
/audit-publish docs/audits/AUDIT_AUDIO_2026-08-30.md
```

Domain label: `audio`. AUD-2026-08-30-D4-01 additionally warrants
`game:fnv` (the REGN `RDSB` path and the only archive the engine is
routinely driven against). AUD-2026-08-30-D7-01 also warrants `game:fnv`
(the canonical footstep/splash paths are FNV assets) and should be
cross-referenced on **#3189** so that issue's fix is not scoped to
deduplication alone. AUD-2026-08-30-D5-01 is `tech-debt` / `doc-rot`.
