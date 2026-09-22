# Audio Subsystem Audit (M44) — 2026-09-22

- **HEAD**: `ee6d3fb39` · **Baseline**: [`docs/audits/AUDIT_AUDIO_2026-09-11.md`](AUDIT_AUDIO_2026-09-11.md) (HEAD `b3db49fa` at that run's task start) · **Audited**: Dim 1–5 (all) · **Unchanged since baseline (skimmed)**: none — all 5 dimensions had commits since D and were checked in full
- **Restart note**: this run resumes a same-preset `/audit-audio` pass killed mid-way by a
  server-side 529 error on 2026-09-21. That prior pass had already produced complete
  dimension analyses for all 5 dimensions in `/tmp/audit/audio/dim_1.md`–`dim_5.md`, each with
  either explicit "no findings" (Dim 1, Dim 2) or full findings (Dim 3, Dim 4, Dim 5), all traced
  to file:line evidence at its own HEAD `73aaed7b9`. This run re-verified every claim against the
  current HEAD (`ee6d3fb39`, 39 commits later) rather than redoing the analysis: diffed every cited
  file between `73aaed7b9` and `ee6d3fb39`, re-ran the guard suites, and corrected two citations
  that drifted from an unrelated line-count shift (see Dim 4). No dimension required a redo.
- **Command**: `/audit-audio` (default scope, all 5 dimensions), run as one leg of
  `/audit-suite --preset comprehensive`. No sub-agents were spawned; each dimension was analyzed
  serially in this session per the orchestrator's explicit instruction (a past suite run silently
  dropped a HIGH finding when a fanned-out sub-agent's result never reached the orchestrator).
- **kira**: pinned `0.10` (workspace `Cargo.toml`), resolved `kira-0.10.8` (unchanged).
- **Tests run**: `cargo test -j 4 -p byroredux-audio` → **33 passed, 0 failed, 7 ignored**
  (identical to the 2026-09-21 pre-restart run and to the 2026-09-11 baseline's post-fix count).
  `cargo test -j 4 -p byroredux --bin byroredux -- audio combat_anim scheduler_access` →
  **63 passed, 0 failed** (engine-binary harness `byroredux-ceb1508ef8849429`, not the engine
  executable — 20 `asset_provider::audio`, 14 `systems::audio`, 9 `combat_anim`, the
  `scheduler_access_tests` pins). **Headless-mode boot: PASS**
  (`audio_world_constructs_without_panic_on_any_environment`,
  `explicit_headless_world_discards_playback_without_retaining_sound`).
  Seven crate guards stayed `#[ignore]`d for lack of a free audio device (one was in use by a
  running game on this host): `looping_emitter_survives_natural_duration_and_stops_on_emitter_remove`,
  `non_looping_emitter_stops_on_emitter_remove_regression_858`, `play_oneshot_queue_drives_real_playback`,
  `audio_system_full_lifecycle_on_real_fnv_sound`, `play_music_drives_streaming_playback_on_real_ogg`,
  `play_music_looping_survives_track_end`, `real_fnv_sounds_decode_through_kira`. The despawn
  truncation and music-looping guards were traced in code instead (Dim 4); the decode-only
  `real_fnv_sounds_decode_through_kira -- --ignored` **was** run and passed (WAV 27,836 frames @
  48 kHz, OGG 52,544 frames @ 32 kHz).

## Executive Summary

Delta since 2026-09-11 is almost entirely one new producer: `combat_feedback_system`
(`ec3a18d2f`, the P2 Draugr combat tail's sound half, landed 2026-09-21), plus a lock-discipline
fix to it (`fc825a6cd`/#4605) and an `AudioWorld::new()`/`headless()` split (`2e2f40b23`) that
touched only construction, not behavior. The crate's core mechanisms (unit seam, reverb send,
underwater filter, music slot, SoundCache, schedule/lifecycle) are unchanged and every guard for
them still passes — **Dim 1 and Dim 2: no findings**.

The new consumer is where the real news is. `docs/audits/AUDIT_GAMEPLAY_2026-09-21.md`
(GAME-D4-2026-09-21-02) already reports that the whole P2 combat tail — Draugr attack/hit/death
takes and their sounds — never fires in production because `DraugrCombatAnim` is inserted only on
the runtime-FaceGen spawn path (Oblivion/FO3/FNV), never on the prebaked path every Skyrim actor
(Draugr included) spawns through, and states "only the player swing sound … can fire" as the one
surviving piece. This audit's own trace of the audio side (**AUD-2026-09-21-D5-01**) corrects that:
the swing push happens before the same `DraugrCombatAnim` query, and that query's early return
also exits before the swing-drain loop further down the function — so **the swing sound cannot
fire either**. On Skyrim today, none of the P2 combat sound family plays, not even the one piece
the gameplay report credited as surviving. A second finding (D5-02) shows that even once the
marker-insertion bug is fixed, the take-state-keyed sound gating drops most impacts (an active take
blocks the next `HitEvent`), drops the killing blow's impact entirely (`Dead` wins the same-frame
race against the hit take), and fires the two-handed-weapon swing sound at the player's own
unarmed punches while the Draugr's real attacks stay silent.

The remaining findings are lower-severity hygiene: a new hand-rolled cache bypasses `SoundCache`
(D3-01), a partial `headless()` migration left three engine tests and one crate test opening a
real audio device under comments that call them headless (D4-01), a scheduler-comment line
pointer drifted stale for a second time — now with an unrelated +3-line shift on top of the
original drift (D4-02), the combat-sound path constants are pinned by a tautological self-match
test (D5-03), no smoke fixture supplies `--sounds-bsa` so no end-to-end route can observe any
audio dispatch, combat included (D5-04), a doc comment says ripples are silent while the code
plays them (D5-05, doc-only), and two status docs (`feature-matrix.md`, `ROADMAP.md`) describe REGN
ambient music and the reverb-ordering mechanism without the qualifiers `#3816` and `#4146`
already established (D5-06, doc-only).

**Severity counts (NEW + regression findings this run)**:

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 1 |
| LOW | 8 |
| **Total** | **9** |

All 9 are **NEW** (dedup: refreshed `gh issue list --repo matiaszanolli/ByroRedux --limit 6000
--state all` → 4,506 issues; keyword search against titles found no match for any of the 9 —
0 matched to existing open issues; every related closed issue cited below was independently
confirmed CLOSED and its fix confirmed still in place). No regression of a prior closed audio
finding was found: #3522/#4146 (reverb-zone comment), #4147 (water-audio magic numbers), #3775
(REGN loop region), #3776 (single-archive scan), #3183 (ripple mixing) — all re-checked in Dim 5
and hold.

## Lifecycle Invariant Matrix (HEAD `ee6d3fb39`)

| Invariant | State | Anchor |
|---|---|---|
| Field-drop order `active_sounds → pending_oneshots → music → reverb_send → reverb_send_db → listener → manager → multi_listener_warned → underwater` | HOLDS (struct untouched) | `crates/audio/src/lib.rs:398,405,410,417,421,425,428,434,437` |
| `new()` keeps drop order and handles through the `..Self::headless()` split (`2e2f40b23`) | HOLDS | `lib.rs:502-506` (explicit `reverb_send`, `manager`; no `Drop` impl on `AudioWorld`) |
| `headless()` never contacts a device | HOLDS | `lib.rs:513-525` (pure field literal) |
| Capacities 512 / 32 (`SUB_TRACK_CAPACITY`/`SEND_TRACK_CAPACITY`) applied in `new()` | HOLDS | `lib.rs:327-328,452-459` |
| Zero `unwrap`/`expect` on the manager `Option` | HOLDS | grep: 0 hits in `lib.rs` |
| `AudioWorld::new()` boot-only, never on cell transition or resize | HOLDS | `byroredux/src/boot/world.rs:185` (single production constructor) |
| Sticky listener (never cleared on despawn; kira capacity 8) | HOLDS | `lib.rs:980-1001` |
| Despawn truncation (tweened `stop()` over `ActiveSound.unload_fade_ms`, `stop_issued` debounce, retain on `Stopped`, queue sounds exempt) | HOLDS (code unchanged; device-gated guards not run this cycle, traced instead) | `lib.rs:1284-1348` |
| `audio_system` body order: listener → underwater → drain → dispatch → prune | HOLDS | `lib.rs:895-899` |
| `footstep_system` Late exclusive before `audio_system` | HOLDS, pinned by `footstep_runs_after_camera_follow_in_late` (#4185, closed) | `late.rs:98-107` vs `:243` |
| `submersion_system` → `water_audio_system` → `audio_system` (exclusive registration order) | HOLDS, pinned by `submersion_runs_after_camera_follow_and_before_water_audio` | `late.rs:144-158,214-224,243` |
| `reverb_zone_system` Late **parallel**, `audio_system` Late **exclusive** | HOLDS, pinned by `reverb_zone_is_parallel_and_audio_is_exclusive_in_late` (#4146, closed) | `late.rs:165-171,243` |
| New PostUpdate producer (`combat_feedback_system`) → Late drain, same frame | HOLDS (stage order is structural) | `boot/schedule/post_update.rs:137-139` → `late.rs:243` |
| REGN music change-gated at every call site (no unconditional redispatch) | HOLDS, 3 sites | `cell_loader/load.rs:661-665,1027-1031`; `scene/world_setup.rs:559-563` |
| `event_cleanup_system` last Late exclusive (`HitEvent`/Splash/Ripple live until after audio) | HOLDS | `late.rs:472` (was `:469`, shifted +3 by `0f0287519`) |

Every row was re-diffed against `73aaed7b9` this run; the only structural change since baseline is
the new PostUpdate row (`combat_feedback_system`) and a cosmetic +3 line shift from an unrelated
scheduler-access fix (#4574) that does not change any ordering guarantee.

## Findings

### AUD-2026-09-21-D3-01: The first runtime lazy-decode SFX consumer hand-rolls its own cache instead of `SoundCache`, so decoded combat PCM is invisible to the telemetry that samples the cache
- **Severity**: LOW
- **Dimension**: Music & SoundCache
- **Location**: `byroredux/src/systems/combat_anim.rs:89-97` (`FeedbackScratch.sounds`), `:373-417` (`play_oneshot_cached`); `crates/audio/src/lib.rs:1460-1471` (the #859 "Dormant API" contract), `:1513-1532` (`SoundCache::get_or_load`); `byroredux/src/ownership_sample.rs:67-72`
- **Status**: NEW
- **Description**: `combat_feedback_system` decodes its three pinned WAVs on first use and caches
  them in a closure-scratch `FxHashMap<&'static str, Option<Arc<byroredux_audio::Sound>>>`. That
  re-implements `SoundCache::get_or_load` (a cache hit returns the `Arc`, a miss runs the loader
  and inserts), the crate API written for exactly this case ("callers can pay the BSA-extract cost
  lazily"). It is the third sound producer to route around `SoundCache`, after
  `try_load_default_footstep` and `try_load_default_water_splash`. It is the first **runtime**
  consumer: those two decode once at boot into a config resource. The crate's #859 contract says
  "anyone wiring the first real consumer should also wire eviction at the same time". This consumer
  sidesteps that by keeping its own map.
  The private map does one thing `SoundCache` cannot: it caches a miss (`None`), so a missing
  archive or a failed decode is not re-probed on every swing; `get_or_load` returns `None` without
  caching and would call the loader again next time. That is a real gap in `SoundCache`, and a
  reason to extend it rather than fork it.
- **Evidence**: `scratch.sounds.insert(path, decoded.clone())` (`combat_anim.rs:396`) after a
  `SoundArchiveProvider::extract` → `load_sound_from_bytes` chain (`:385-395`).
  `git grep -n 'SoundCache' byroredux/src` hits only `ownership_sample.rs` (a `try_resource` of a
  resource nothing inserts) and `save_io/registry_completeness_tests.rs`. #3189's close note defers
  "ideally through SoundCache" to #859/#850. Both are CLOSED, so nothing tracks it.
- **Impact**: bounded today: three `&'static str` keys, about 318 KB of 16-bit mono PCM (38 KB +
  84 KB + 195 KB), so no growth hazard. The costs are structural:
  - the decoded audio is missing from `sound_cache_entries` / `bytes_estimate`, the only audio
    memory telemetry;
  - a second cache contract (no case folding, negative caching) sits beside the crate's;
  - the next producer (a stagger voice, 3.5b FOOT) has two templates to copy.
  Minor: the decode runs synchronously on the main thread inside a PostUpdate exclusive on the
  first swing/hit/kill, instead of at boot like the footstep/splash loaders. At these sizes it is
  sub-millisecond.
- **Related**: #859, #850, #3189 (all closed); AUD-2026-09-21-D5-01 (the same system's swing
  path); #4605 (CLOSED 2026-09-21 by `fc825a6cd`, filed as CONC-D3-2026-09-21-02 — fixed the
  function's shadowed `DraugrCombatClips` guard; unrelated to this finding's cache-shape gap).
- **Suggested Fix**: add a negative-cache arm to `SoundCache::get_or_load` (remember a failed key).
  Install one engine `SoundCache` resource at boot and route `play_oneshot_cached` through it. Sample
  `bytes_estimate` next to `sound_cache_entries` in `ownership_sample.rs`, so the first real
  consumer brings its telemetry with it.

### AUD-2026-09-21-D4-01: `2e2f40b23`'s move to `AudioWorld::headless()` stopped at `systems/audio.rs` — three engine tests and one crate test still open the host audio device through `default()`/`new()`, two under comments that call it headless
- **Severity**: LOW
- **Dimension**: Manager & ECS Lifecycle
- **Location**: `byroredux/src/asset_provider/audio.rs:499`, `:515`, `:537` (comments `:492-495`, `:507-510`); `crates/audio/src/tests.rs:517` (`underwater_listener_state_persists`)
- **Status**: NEW
- **Description**: `2e2f40b23` added `AudioWorld::headless()`, which never contacts a device, and
  moved the `systems/audio.rs` tests to it ("ensuring no audio device interaction"). The sibling
  REGN-music tests in `asset_provider/audio.rs`, `dispatch_with_no_music_form_stops_playback_without_panic`,
  `dispatch_with_folder_form_soun_stops_playback_without_archive_lookup` and
  `dispatch_with_unresolvable_form_id_stops_playback`, still insert
  `byroredux_audio::AudioWorld::default()`. That is `new()`, which runs
  `AudioManager::<DefaultBackend>::new` and on a desktop host opens a cpal output stream plus kira's
  backend thread for each test. Their comments say "a real (headless-fallback) `AudioWorld`" and
  "a real (headless) `AudioWorld`". That is true only on a host without a device. In the crate,
  `underwater_listener_state_persists` constructs `new()` to test a plain setter. The four
  hand-written inactive `AudioWorld { … manager: None … }` literals (`tests.rs:409,474,535,894`)
  now duplicate `headless()`. Each must be edited whenever a field is added.
- **Evidence**: `git grep -n 'AudioWorld::default()' byroredux/src` → the three
  `asset_provider/audio.rs` lines. `lib.rs:440-444` (`Default` → `new()`) and `:460`
  (`AudioManager::new`).
- **Impact**: host-dependent unit tests. The assertions (`!is_music_active()`, the setter
  round-trip) pass on both arms today, so nothing is red. But the lane opens the real audio device on
  every developer machine. It runs a different branch on a device host than in CI (`manager: Some`
  vs `None`), so it pins less than it reads as pinning. The comments state the opposite of what
  happens on the host the tests are usually run on.
- **Related**: `2e2f40b23` (the partial sweep); the skill's Dim 4 invariant ("tests use it, not the
  device-dependent `default()`").
- **Suggested Fix**: switch the three `asset_provider/audio.rs` sites and
  `underwater_listener_state_persists` to `AudioWorld::headless()`, and correct the two comments.
  Replace the four inactive literals with `AudioWorld::headless()` (with `reverb_send_db` set where a
  test needs a non-default level). Keep `new()` only in the tests that exist to probe the device
  (`audio_world_constructs_without_panic_on_any_environment`, `reverb_send_defaults_to_silent`, the
  `#[ignore]`d playback set).

### AUD-2026-09-21-D4-02: `audio_system`'s registration comment points at "line 650-656 above" and quotes a phrase that no longer exists — stale since the M27 comment was written in `main.rs`, through two file splits (and drifted a further +3 lines under this same run, unrelated cause)
- **Severity**: LOW
- **Dimension**: Manager & ECS Lifecycle
- **Location**: `byroredux/src/boot/schedule/late.rs:233-244` (was `:230-239` as of `73aaed7b9`;
  `0f0287519`/#4574 inserted an unrelated 3-line access-declaration row upstream at ~line 200 for
  `reconcile_pending_dead_actors_system`, shifting this block — the pointer was already wrong by
  ~400 lines before that commit and stays wrong now)
- **Status**: NEW
- **Description**: The comment on `scheduler.add_exclusive(Stage::Late, byroredux_audio::audio_system)`
  reads: "The ordering comment at line 650-656 above ("MUST run BEFORE audio_system" / "Must run
  BEFORE audio_system") encodes a real dependency…". `late.rs` is 473 lines, so there is no line
  650. The uppercase quote "MUST run BEFORE audio_system" appears nowhere in the tree. The
  dependency it means is `camera_follow_system`'s "Must run BEFORE `audio_system` /
  `submersion_system`" at `late.rs:17-18`. The text was written in `main.rs` by `05fe2bac2` (M27
  Phase 3, 2026-05-23), carried verbatim into `boot.rs` by `40d533a85` (#1858), then into
  `boot/schedule/late.rs` by `8c5e02aab` (#3855). The line numbers were never updated. The
  2026-09-11 report quoted the surrounding block (then `late.rs:225-234`) as the correct statement
  of the mechanism without noting the dead pointer. The mechanism is correct: exclusive after the
  parallel batch. Only the pointer and the quote are stale — and the pointer just moved again, for
  a third, entirely unrelated reason (an access-declaration fix four commits removed from audio).
- **Evidence**: `wc -l byroredux/src/boot/schedule/late.rs` → 473 (470 as of `73aaed7b9`, +3 from
  the unrelated `0f0287519`/#4574 access-row insert);
  `grep -rn 'MUST run BEFORE audio_system' byroredux/src` → only this comment;
  `git log -S'line 650-656'` → `05fe2bac2`, `40d533a85`, `8c5e02aab`.
- **Impact**: documentation only. This is the same registration-comment drift class that #3522 and
  #4146 each closed once on the neighbouring `reverb_zone_system` comment. A reader chasing the
  cited dependency finds nothing. The +3 shift this cycle demonstrates the fragility directly: any
  unrelated edit above this block re-breaks the citation without touching audio code at all.
- **Related**: #3522, #4146 (closed; same class, adjacent comment); #1858, #3855 (the splits that
  carried it); #4574 (this cycle's unrelated shift).
- **Suggested Fix**: name the dependency instead of a line: "`camera_follow_system` (Late parallel
  batch) authors the camera pose this system reads; exclusive sequencing runs this after that
  batch". Drop the phantom uppercase quote so no future unrelated edit can re-break a line pointer.

### AUD-2026-09-21-D5-01: The player's swing sound is queued before a `DraugrCombatAnim` early return that skips the only drain — on Skyrim every swing is stranded in a session-long `VecDeque`, so no P2 combat sound can fire (not even the swing)
- **Severity**: MEDIUM
- **Dimension**: Engine Consumers
- **Location**: `byroredux/src/systems/combat_anim.rs:126-148` (push), `:150-152` (early return), `:329-332` (drain); `byroredux/src/npc_spawn/resumable.rs:1079-1084` (the only `DraugrCombatAnim` insert); `crates/core/src/ecs/world.rs:474-476` (`query` → `None` for an unregistered storage)
- **Status**: NEW
- **Description**: `combat_feedback_system_inner` pushes the player's swing position into
  `scratch.swings` during its read pass (`:146`). Four lines later it runs
  `let Some(anim_q) = world.query::<DraugrCombatAnim>() else { return; };` (`:150-152`). That
  `return` exits the whole function and skips the only drain (`:330-332`). `World::query` returns
  `None` when no storage exists for the type (`self.storages.get(&type_id)?`, `world.rs:476`).
  Nothing registers `DraugrCombatAnim`. Its storage appears lazily on the first `world.insert`,
  which is only the runtime-FaceGen spawn Finalize (`resumable.rs:1082`). `has_runtime_facegen_recipe()`
  is `Oblivion | Fallout3NV` (`crates/plugin/src/esm/reader.rs:268-269`). `DraugrCombatClips`
  installs only on Skyrim (`populate_draugr_combat_clips` returns early for any other `GameKind`),
  so there is no configuration in which the resource exists and the storage exists too:
  - **Today (Skyrim, `--game skyrimse` / LE)**: the profile lists `Skyrim - Animations.bsa`, so the
    clips install. `docs/audits/AUDIT_GAMEPLAY_2026-09-21.md`'s GAME-D4-2026-09-21-02 means the
    marker is never inserted on Skyrim, so the storage never exists. Every character-mode swing is
    pushed and never played. **This corrects that report's "only the player swing sound … can
    fire": none of the P2 combat sound family can fire in production, on any game.** (Audio-side
    fact GAME-D4-2026-09-21-02 does not have — cited per the orchestrator's brief.)
  - The closure scratch lives for the process, so `scratch.swings` gains one `Vec3` per swing for
    the whole session and is never drained. The growth is unbounded but per user action.
  - **After GAME-D4-2026-09-21-02 is fixed**: a Skyrim session that starts in a Draugr-free cell
    (MQ101 Helgen, Whiterun, the P0/P1 Bannered Mare fixtures) is still silent until the first
    Draugr spawns. On that frame every stranded swing drains at once, a burst of N simultaneous
    swing one-shots at stale positions, possibly in a previous cell's coordinate frame. Past 256 it
    trips `play_oneshot`'s per-push cap `warn!`, and the >32 drain warn fires regardless.
  - Structural cause: the sound half sits behind two animation-side preconditions (clip resource
    present, marker storage registered). The player's own swing needs neither.
- **Evidence**: `git grep -n DraugrCombatAnim -- byroredux/src crates` → the definition
  (`components.rs:2045`), the one insert (`resumable.rs:1082`), comments in `cell_loader/load.rs`.
  No test reaches the swing drain. `headless_swing_delta_is_a_safe_no_op` (`:700-718`) registers
  the storage through `spawn_actor` and installs no `PlayerEntity`/`PlayerMode`, so nothing is ever
  pushed. The `#[cfg(test)]` twin builds a fresh `FeedbackScratch` per call (`:436-438`), so a
  cross-tick `last_attacks_started` delta cannot be observed through it. Re-confirmed at HEAD
  `ee6d3fb39`: `fc825a6cd` (#4605, closed 2026-09-21) touched this function but only rescoped the
  `DraugrCombatClips` guard's lifetime — it left the query/return/drain ordering untouched.
- **Impact**: Skyrim player melee is silent on the profile route today, contrary to the P2 "spatial
  sound family" goal and to today's gameplay report. Slow unbounded scratch growth. A latent audible
  burst once the marker fix lands.
- **Related**: GAME-D4-2026-09-21-02 in `docs/audits/AUDIT_GAMEPLAY_2026-09-21.md` (root cause of
  the missing storage; its swing claim is corrected here); #4551 (closed; clip install on `--cell`);
  #4605 (CLOSED 2026-09-21 by `fc825a6cd`, filed as CONC-D3-2026-09-21-02 — fixed the same
  function's shadowed `DraugrCombatClips` guard, a lock-discipline defect unrelated to this
  early-return-before-drain defect, which the fix left untouched); AUD-2026-09-21-D5-02, -D5-03,
  -D5-04.
- **Suggested Fix**: drain `scratch.swings` (or dispatch the swing directly) before the marker
  query, and let a missing `DraugrCombatAnim` storage skip only the per-actor loop
  (`if let Some(anim_q) = …`). Add a test on the inner function with a persistent scratch:
  `PlayerEntity` + `PlayerMode::Character` + a `CombatState` bump, no `DraugrCombatAnim` storage,
  and assert `scratch.swings` is empty after the tick.

### AUD-2026-09-21-D5-02: (latent) Combat sounds are keyed to take-state transitions, not combat events — impacts are dropped during any active take and on the killing blow, and the pinned two-handed swing plays for the player while the Draugr's own attack is silent
- **Severity**: LOW. Latent until GAME-D4-2026-09-21-02 (and D5-01) are fixed; fix together.
- **Dimension**: Engine Consumers
- **Location**: `byroredux/src/systems/combat_anim.rs:153-241` (per-actor decision ladder), `:18-21` (module doc), `:202-204` (dedup rationale), `:224-239` (attack take, `sound: None`), `:123-148` (swing keyed on the player's `attacks_started`)
- **Status**: NEW
- **Description**: the ladder runs, per actor: (1) `Dead` → death take + `DeathVoice`, `continue`;
  (2) an active take → tick/restore, `continue`; (3) a `HitEvent` on the actor → hit take +
  `Impact`; (4) the actor was an aggressor → attack take, `sound: None`. Sounds only come out of the
  install branches:
  - (a) **Impacts during an active take are dropped.** Step 2 runs before step 3, so a hit on a
    Draugr that is mid-stagger (`mtstaggermedium`, 2.03 s) or mid-attack (`2hmattackforwardb`,
    2.50 s, per `docs/engine/p2-combat-anim-sound-fixture.md`) plays nothing. The player's melee
    cooldown is `MELEE_COOLDOWN_SECONDS / weapon.speed`, which is 0.45 s unarmed (`combat.rs:31,
    477-483`), and the P2 gate's hits are the 8-damage `UNARMED_DAMAGE` fallback. At that cadence
    only about one hit in five sounds. For the frozen target (50 Health, 8/hit, 7 hits) only hits 1
    and 6 would play an impact. The code calls this gate dedup ("the transient event can be visible
    for more than one read"), but each `HitEvent` is visible to exactly one PostUpdate read
    (inserted in Update, drained by the last Late exclusive). There is nothing to dedup, and the
    gate discards real hits.
  - (b) **The killing blow has no impact.** `combat_damage_system` (Update) inserts `Dead` in the
    same frame as the lethal `HitEvent` (`combat.rs:334-335`), so step 1 always wins. The module doc
    promises "`HitEvent` → impact one-shot at the target" (`:18-21`).
  - (c) **The swing is attributed to the wrong combatant.** The fixture doc pins
    `fx_swing_blade2hand_03.wav` as "2-handed blade family, matches the fixture weapons". Those are
    the Draugr's Battleaxe/Greatsword (`docs/engine/p2-combat-fixture.md:31-32`), and the doc says
    the "swing one-shot fires at take start … at the actor's position". The code fires it on the
    player's `attacks_started` delta at the player's position, even for an unarmed punch. The
    Draugr's own attack take (step 4) carries `sound: None`, so the enemy's two-handed swings make
    no sound at all. When a Draugr hits the player, no impact plays either: the player has no
    marker, and `fleshdraugr` would be the wrong material anyway.
- **Evidence**: see the line references above. Real-data clip durations are from the fixture doc
  table; `clips.hit_secs`/`attack_secs` are `clip.duration` (`asset_provider/animation.rs:297-316`).
- **Impact**: once reachable, most player hits on a Draugr are silent, the kill is silent apart from
  the voice, and the enemy's attacks are soundless. The planned P2 "spatial sound family" evidence
  would not track the fight.
- **Related**: AUD-2026-09-21-D5-01; GAME-D4-2026-09-21-02 and GAME-D4-2026-09-21-05
  (`AUDIT_GAMEPLAY_2026-09-21.md`, the same ladder's death-replay defect); #4564 (fixture doc vs
  pinned attack clip).
- **Suggested Fix**: decouple sound from the animation ladder. Emit the impact for every `HitEvent`
  whose target carries the marker, including a target that died this frame, whether or not a take
  is installed, and keep the take gating for animation only. Fire the two-handed swing at the
  Draugr's attack-take start, at its position. Give the player's swing a weapon-appropriate sound or
  none, and drop the dedup premise from the comment.

### AUD-2026-09-21-D5-03: The combat sound-path pin is tautological, and a wrong path would fail silently — no test resolves the three WAVs, and an archive miss is cached as `None` with no log
- **Severity**: LOW
- **Dimension**: Engine Consumers
- **Location**: `byroredux/src/systems/combat_anim.rs:734-741` (sound loop of `fixture_paths_and_resource_contract_stay_aligned`), `:382-399` (`play_oneshot_cached` miss path); `byroredux/src/asset_provider/animation.rs:905-958` (real-data test, clips only)
- **Status**: NEW
- **Description**: the second loop asserts
  `include_str!("combat_anim.rs").contains(SWING_SOUND_PATH)` (and the impact/death constants).
  This file defines those constants (`:51-54`), so the check passes for any value, including a typo
  no archive contains. The first loop (clip paths checked against `animation.rs`) is a real
  cross-file pin; the sound half is not. `draugr_combat_clips_install_real_assets_when_available`
  (`#[ignore]`) installs only the three HKX clips. Nothing extracts or decodes the WAVs. At runtime
  a non-empty provider that lacks the path returns `None` from `extract`. `play_oneshot_cached`
  caches that `None` without a log (only a decode failure warns, `:392`). REGN dispatch, by
  contrast, warns "not found in any --sounds-bsa archive" (`asset_provider/audio.rs:281`).
- **Evidence**: the three paths are correct at HEAD. A read-only BSA probe
  (`/tmp/audit/audio/bsa_probe.py`, from the pre-restart pass) found all three in SE and LE
  `Skyrim - Sounds.bsa` as 16-bit PCM. The gap is that a regression would go unnoticed.
- **Impact**: a renamed or mistyped path silently disables that combat sound forever in the session,
  with no test or log signal.
- **Related**: #4604 (closed; the same vacuous-needle shape in another test); AUD-2026-09-21-D5-04.
- **Suggested Fix**: replace the self-referential loop with an `#[ignore]`d real-data check beside
  the clip test (extract each path through `SoundArchiveProvider` from `Skyrim - Sounds.bsa` and
  decode with `load_sound_from_bytes`). Warn once on an extract miss when the provider is non-empty,
  mirroring the REGN dispatcher.

### AUD-2026-09-21-D5-04: No smoke fixture supplies `--sounds-bsa`, and the Skyrim one also lacks `Skyrim - Animations.bsa`, so the P2 combat-sound gate the fixture doc plans cannot observe a sound, and no smoke gate exercises any audio consumer
- **Severity**: LOW
- **Dimension**: Engine Consumers
- **Location**: `docs/smoke-tests/fixtures/skyrim_se.env` (`FIXTURE_ARCHIVE_ARGS`), and the same array in `fnv.env`, `fo3.env`, `oblivion.env`, `fo4.env`; `docs/smoke-tests/p2-melee-core.sh:155-170`; `docs/engine/p2-combat-anim-sound-fixture.md` (Wiring step 4, "Gate"); `byroredux/src/boot/cli.rs:303-336` (profile expansion only under `--game`)
- **Status**: NEW
- **Description**: every fixture lists only mesh, texture (and script/material) archives. None
  passes `--sounds-bsa`. Smoke launches use `--esm`, and `expand_game_profile_args` expands profile
  archives only for `--game` (or `[defaults].game` with no other load flag), so
  `SoundArchiveProvider` is empty on every smoke route. Footsteps, splash, REGN music and combat
  sounds all take their "no archive" silent branch. The fixture doc's P2 gate ("extend
  `p2-melee-core.sh` … plus non-zero `play_oneshot` queue observations, so the gate proves playback")
  targets `skyrim_se.env`. That fixture also passes no `Skyrim - Animations.bsa`, so
  `populate_draugr_combat_clips` warns and installs nothing, and `combat_feedback_system` returns on
  its first line. #4551's premise ("the --esm --cell route which every smoke test drives") therefore
  does not reach the smoke route either. On a device-less runner `play_oneshot` returns before
  queueing (`manager.is_none()`, `lib.rs:578`), so "queue observations" are unobservable there
  whatever the archives. The existing telemetry (`pending_oneshot_count`, `active_sound_count`) only
  moves with a live manager.
- **Evidence**: `sed -n '/FIXTURE_ARCHIVE_ARGS=(/,/)/p' docs/smoke-tests/fixtures/*.env`: no
  `--sounds-bsa` anywhere. The `--game skyrimse` profile supplies both archives
  (`assets/debug_profiles.toml:190,204`).
- **Impact**: no end-to-end evidence for any audio path. The planned P2 sound gate cannot pass as
  designed on its named route. Test infrastructure only.
- **Related**: #3788 (the same missing-`--sounds-bsa` gap on the FNV profile, closed); #4551
  (closed); AUD-2026-09-21-D5-01. Owner of the fixtures: `/audit-runtime`.
- **Suggested Fix**: add `--sounds-bsa` (and for Skyrim `--bsa "Skyrim - Animations.bsa"`) to the
  fixtures whose gates will assert audio. Add a monotonic `oneshots_requested` counter to
  `AudioWorld`, incremented before the manager gate, so a headless gate can observe that dispatch
  was requested.

### AUD-2026-09-21-D5-05: `water_audio_system`'s doc says ripples stay silent while its body plays them, and the module header omits water audio
- **Severity**: LOW
- **Dimension**: Engine Consumers
- **Location**: `byroredux/src/systems/audio.rs:226-229`, `:319-340`, `:1`
- **Status**: NEW
- **Description**: the doc reads "Ripple markers intentionally remain presentation/gameplay data;
  only the edge-triggered splash is audible, preventing a looping sound on every frame". The body
  plays the strongest ready ripple at ×0.45 when no splash fired, rate-limited by the per-surface
  `RIPPLE_COOLDOWN_SECS` (`:319-340`). The doc and the ripple playback landed together in
  `948f104a3`, so the doc was wrong from its first commit. `f8cfd185e` (#3183) reworked the cadence
  without touching it. The module header (`:1`) is "Audio routing systems — reverb zones, footstep
  emitters", with no water audio.
- **Evidence**: `git log -S'intentionally remain presentation'` → `948f104a3`, whose body already
  contains the `ripple_ready` playback branch.
- **Impact**: documentation only. A reader looking for the source of an audible repeating
  water sound is told ripples are silent.
- **Related**: #3183, #4147 (closed; same system).
- **Suggested Fix**: state the actual contract: splashes are edge-triggered; ripples are audible,
  quieter, at most one per frame, per-surface cooldown, only when no splash fired. Add "water
  audio" to the module header.

### AUD-2026-09-21-D5-06: The status docs present REGN background music as shipped with no #3816 qualifier, and ROADMAP's M44 row still attributes the reverb ordering to registration order in `boot.rs`
- **Severity**: LOW
- **Dimension**: Engine Consumers
- **Location**: `docs/feature-matrix.md:156` (`Region ambient (REGN) — background music | ✓`), `:148` (`BSA WAV decode + cache | ✓`); `ROADMAP.md:837` (M44 row: "REGN ambient background music shipped"; "registered in `boot.rs` ahead of `audio_system`"), `:1267`
- **Status**: NEW
- **Description**: the feature-matrix row (added `a924244ee`, 2026-08-25) predates #3787/#3811/#3816.
  Those showed that `music_form` resolves as a SOUN on **no** supported game: Oblivion RDMD is an
  enum, Skyrim RDMO targets MUSC, FNV RDSB/RDSI target MSET. The mechanism ships and is tested, but
  plays nothing in production. The ✓ and ROADMAP's "shipped" carry no qualifier. ROADMAP's M44 row
  also says `reverb_zone_system` is "registered in `boot.rs` ahead of `audio_system`". `boot.rs` was
  split by `8c5e02aab` (#3855), and "ahead of" is the registration-order premise #4146 removed from
  the code comment: the guarantee is parallel-batch-before-exclusives. The "cache" in
  `BSA WAV decode + cache ✓` is `SoundCache`, which no engine code installs (see
  AUD-2026-09-21-D3-01).
- **Evidence**: see the lines above; #3816 is OPEN; `git blame -L 156,156 docs/feature-matrix.md`
  → `a924244ee`. Re-confirmed at HEAD `ee6d3fb39`: neither file changed since `73aaed7b9`.
- **Impact**: documentation only. The authoritative status table tells readers region music plays.
  The ROADMAP row teaches the mechanism #4146 corrected.
- **Related**: #3816 (open), #3523 / #3088 (closed; earlier status-site drift on the same rows),
  #4146 (closed).
- **Suggested Fix**: mark the REGN row "mechanism ✓, plays nothing until #3816 (MUSC/MUST/MSET/RDMD
  decode)" in both sources. Replace the ROADMAP reverb sentence with the parallel-vs-exclusive
  mechanism and the `boot/schedule/late.rs` path. Qualify "cache" or drop it until `SoundCache` has
  an engine installer.

## Future-Phase Readiness

- **FOOT records → per-material footstep sound (3.5b)**: still not shipped.
  `try_load_default_footstep` picks one default sound; the FOOT-record → material-keyed dispatch
  described in the skill's "Not shipped" list has no code yet. No regression this cycle.
- **MUSC/MUST/MSET/RDMD decode (#3816, open)**: the REGN dispatch mechanism (`dispatch_region_ambient_music`,
  change-gating, loop threading, failure-layer stop-on-every-arm) is live, tested and unchanged
  since baseline, but `music_form` cannot resolve as a `SOUN` on any supported game until #3816
  lands. D5-06 flags that the status docs don't say so.
- **Occlusion**: no line-of-sight or geometry-based attenuation exists; `Attenuation` is pure
  distance-based. Not scoped for this cycle; no new code touches it.
- **P2 combat sound family (new this cycle)**: the mechanism is wired (fourth `play_oneshot`
  producer, correct unit seam and routing per D5-01's "disproved" check) but is fully silent in
  production for the reasons in D5-01/D5-02. This is the dimension most likely to see near-term
  fix activity, given the sibling gameplay audit already flagged the root cause.

## Dedup Method

`gh issue list --repo matiaszanolli/ByroRedux --limit 6000 --state all --json number,title,state,labels`
refreshed to `/tmp/audit/issues.json` (4,506 issues). Every finding's keywords were searched against
issue titles and against `docs/audits/`. All 9 findings are NEW (no title match). Every closed issue
cited above (`#859`, `#850`, `#3189`, `#3522`, `#4146`, `#4147`, `#3183`, `#4551`, `#4604`, `#3788`,
`#3523`, `#3088`, `#4605`, `#1858`, `#3855`) was independently confirmed CLOSED via the refreshed
listing, and its fix re-checked still in place against current HEAD where the finding depends on it.

---
*Scratch dimension files (`/tmp/audit/audio/dim_1.md`–`dim_5.md`) retained per this run's working
notes; not part of the committed report.*
