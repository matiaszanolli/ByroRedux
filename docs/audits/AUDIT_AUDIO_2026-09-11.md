# Audio Subsystem Audit (M44) — 2026-09-11

- **Command**: `/audit-audio` → all 7 dimensions, `--depth deep`
  (7 dimension agents dispatched in parallel batches of 3/3/1, each
  independently re-deriving its checklist against live source — not a
  single-agent pass)
- **Branch**: main · **HEAD at task start**: `b3db49fa`
- **kira**: pinned `0.10` (workspace `Cargo.toml`), resolved `kira-0.10.8`
- **Method**: each dimension agent read its assigned files in full
  (`crates/audio/src/lib.rs` — 1566 lines; `crates/audio/src/tests.rs` — 1572
  lines; `byroredux/src/systems/audio.rs` — 851 lines; `byroredux/src/asset_provider/audio.rs`
  — 547 lines; `byroredux/src/asset_provider/texture.rs`; `byroredux/src/components.rs`
  audio sections; `byroredux/src/boot/schedule/late.rs` — full file, 449 lines;
  `crates/core/src/ecs/scheduler.rs` — full file; relevant sections of
  `byroredux/src/scene.rs`, `byroredux/src/scene/world_setup.rs`,
  `byroredux/src/cell_loader/load.rs`, `byroredux/src/boot/world.rs`), plus
  the vendored `kira-0.10.8` sources and `crates/core/src/ecs/sparse_set.rs`.
  Every claim in this report was traced to exact file:line evidence by the
  dimension that owns it — no row was inherited from the prior report without
  re-verification against live HEAD.
- **Tests run** (not trusted from prose): `CARGO_BUILD_JOBS=4 cargo test -q -p
  byroredux-audio` → **32 passed, 0 failed, 7 ignored** (grown from the
  2026-08-30 report's 29+6 — new tests landed alongside #3521's and #3775's
  fixes). **Headless-mode boot: PASS.** The engine-binary test modules
  (`byroredux/src/systems/audio.rs`, `byroredux/src/asset_provider/audio.rs`)
  were verified **statically** rather than compiled — the box was under
  memory pressure during this cycle (2.6 GB free, ~20 GB already in swap) and
  `cargo test -p byroredux --bin byroredux` links the whole engine binary,
  the same OOM hazard prior cycles hit. Static counts: 14 `#[test]` in
  `systems/audio.rs`, 20 in `asset_provider/audio.rs`, 39 in the crate
  (matching the 32-pass/7-ignore split above).
- **Dedup baseline**: fresh `gh issue list --repo matiaszanolli/ByroRedux
  --limit 200 --json number,title,state,labels` (56 open issues at pull
  time), the full prior `docs/audits/AUDIT_AUDIO_2026-08-30.md` report, and a
  shared briefing document distributed to every dimension agent listing which
  of the prior cycle's findings had since been closed (verified against the
  closing commit's diff, not the tracker's `state` field alone, for every
  citation below).

---

## Delta Analysis (since `AUDIT_AUDIO_2026-08-30.md`, HEAD `64f64480`)

~90 commits landed on `main` in the window, a much larger delta than the
three previous audit-audio cycles combined (each of which found the
subsystem's executable surface byte-identical to its predecessor). Commits
actually touching this subsystem's scope:

| Commit | What it did | This audit's disposition |
|---|---|---|
| `d7f7d93a` (#3520) | Author `FootstepEmitter.stride_threshold` in Bethesda units (52.5 BU, was 1.5 "≈1.5 m") | **FIXED, re-verified** |
| `1489d252` (#3521) | `drain_pending_oneshots`: `VecDeque::drain(..)` in place, not `mem::take` | **FIXED, re-verified** |
| `cb6196ea` (#3775) | Decode SOUN's Loop flag (`sound_loops`), wire it into `play_music`'s new 4th param `looping: bool` | **FIXED, re-verified in full — the whole chain, not just the signature** |
| `57fdcc57` | (subject: doc rot) also silently corrected `is_music_active`'s docstring (closes #3778 / AUD-2026-08-30-D4-02) | **FIXED, re-verified** |
| `47f9f068` (#3787), `74e21026` (#3811) | Correct the REGN RDSB/RDSI SOUN premise; confirm Oblivion RDMD / Skyrim RDMO target types | **Confirmed doc/diagnosis fixes; did not unblock playback** |
| `cba6825e` (#3914) | Carry SOUN.FNAM's file-vs-folder distinction (`sound_is_folder`) instead of documenting it as always-a-file | **FIXED, re-verified** |
| `6a571390` (#3915) | Track FNV MSET region music under #3816; correct a false "FNV inherits RDMD" claim | **Documentation/tracking consolidation only — no code path added** (re-verified via `git show --stat`) |
| `a6295284` (#3913), `fc0aa830` (#3788) | Verify per-game archive keys for the footstep/splash loaders; give the fnv profile a `--sounds-bsa` | **FIXED — closes AUD-2026-08-30-D7-01, and more thoroughly than that report's own suggested remediation** (see Findings) |
| `1382efb0` (#3652 sibling) | Move `footstep_system` from `Stage::PostUpdate` to `Stage::Late`, giving it a declared `Access` for the first time | **Confirmed landed; same-frame producer→consumer edge into `audio_system` re-verified under the new mechanism** |
| `8c5e02aa` | `refactor(boot): split boot.rs into boot/ — one file per concern` | **Caused a new regression** — see Findings |
| `012dfa93` | `fix(assets,shaders): … last-wins sound archives …` | Confirmed `SoundArchiveProvider::extract` stayed `.rev()` (last-wins, #3917) — not reverted |

### Verification of last cycle's findings

All four findings from `AUDIT_AUDIO_2026-08-30.md` are now **CLOSED and
verified fixed at HEAD** (not merely trusted from the issue tracker — every
row below was re-derived from the closing commit's actual diff):

| Finding | Was | Now |
|---|---|---|
| AUD-2026-08-30-D4-01 (REGN music has no loop region, plays once then silence) | MEDIUM | **FIXED** — #3775 (`cb6196ea`): real Loop-bit decode, real `.loop_region(0.0..)` application, threaded end-to-end through `dispatch_region_ambient_music` |
| AUD-2026-08-30-D7-01 (`--sounds-bsa` arity split: REGN provider repeatable, footstep/splash loaders first-match-only) | MEDIUM | **FIXED** — the two one-off loaders no longer parse `args` themselves; both now take the same `&SoundArchiveProvider` instance the REGN path reads, built once in `boot/world.rs` |
| AUD-2026-08-30-D4-02 (`is_music_active` docstring promises "playing or fading out", reports `false` through the whole fade tail) | LOW | **FIXED (doc-only)** — `57fdcc57` corrected the docstring to match the actual immediate-clear behavior; no `stopping: bool` was added, and none was needed |
| AUD-2026-08-30-D5-01 (unsourced `ReverbBuilder` literals) | LOW | **FIXED** — `REVERB_FEEDBACK`/`REVERB_DAMPING`/`REVERB_STEREO_WIDTH` are now named consts with a provenance comment and a dedicated regression test |

Also carried and correctly **not** re-flagged: **#3086** (entity-path spatial
sub-track position frozen at dispatch — still open, still latent, zero
engine producers reach it); **#3301**/**#2372** (REGN `incidental` / non-
`Sound` RDAT kinds — future-phase); **#3816** (Skyrim MUSC/MUST + FNV MSET +
Oblivion RDMD decode — still open, still the correct tracking issue for "REGN
`music_form` never resolves on any supported game's live corpus," re-verified
this cycle against `6a571390`'s diff rather than assumed).

**New this cycle**: a doc-comment regression (of the already-closed #3522)
was independently re-discovered by three separate dimension agents (5, 6, 7)
working from different entry points, and one new LOW documentation-parity
gap. Both are detailed in Findings below.

---

## Executive Summary

**7 dimensions run. 2 NEW findings (0 CRITICAL / 0 HIGH / 0 MEDIUM / 2 LOW).**

| # | Dimension | NEW findings |
|---|---|---|
| 1 | Spatial Sub-Track Lifecycle & Leaks | **0** |
| 2 | Listener Pose & Attenuation | **0** |
| 3 | SoundCache Growth & Eviction | **0** |
| 4 | Streaming Music Lifecycle | **0** (one LOW test-gap observation recorded, deliberately not filed — see Future-Phase Readiness) |
| 5 | Reverb Send & Routing | **1** (LOW — independently corroborated by Dims 6 and 7; reported once below) |
| 6 | Manager Lifecycle, ECS & Cell Streaming | **1** (the same LOW as Dim 5, independently derived) |
| 7 | Gameplay Audio Wiring | **2** (the same LOW as Dims 5/6, plus one new LOW) |

Deduplicated across dimensions: **2 distinct NEW findings**, both LOW.

**Headless-mode boot**: **PASS** — 32 default tests green in
`byroredux-audio`, 7 device/data-gated `#[ignore]`d, 0 failing.

**Headline**: this was the largest-delta cycle in the audio audit's history
— ~90 commits since the last report, versus three consecutive prior cycles
that found the subsystem byte-identical to its predecessor. All four of the
prior cycle's findings are now closed and genuinely fixed (verified against
each closing commit's diff, not the tracker alone), including the flagship
one: REGN ambient music now actually loops. The only new defect this cycle
is a **documentation regression**, not a runtime bug — a comment explaining
why `reverb_zone_system` is guaranteed to run before `audio_system` was
fixed once (closing #3522), then broken again by an unrelated `boot.rs`
file-split refactor (`8c5e02aa`) nine days later, and its *replacement* text
also never actually stated the correct mechanism. Three independent
dimension agents, entering from different angles (reverb routing, field-drop
ordering, gameplay wiring), each traced the actual `Scheduler`
implementation from scratch and reached the same conclusion: **the ordering
this comment exists to explain genuinely holds, is structurally guaranteed,
and was never at risk** — only the comment's explanation of *why* is wrong,
for the second time.

- **Shipped surface, re-confirmed at HEAD**: `AudioWorld` graceful
  degradation with zero `unwrap()`/`expect()` on the manager `Option`
  (`SUB_TRACK_CAPACITY = 512` / `SEND_TRACK_CAPACITY = 32`, both applied in
  `new()`); `AudioListener`/`AudioEmitter`/`OneShotSound`; `audio_system` =
  `sync_listener_pose` → `update_underwater_filters` → `drain_pending_oneshots`
  → `dispatch_new_oneshots` → `prune_stopped_sounds`; both one-shot dispatch
  paths (queue `VecDeque` cap 256 `pop_front`, drained in place; entity path
  with `loop_region(..)`); tweened-`stop()` despawn truncation with
  `stop_issued` debounce; single-slot main-track streaming music, now with a
  working loop region and a real (`world_info.rs`) diagnostic caller; global
  reverb send (`NEG_INFINITY` dry default, named+documented `ReverbBuilder`
  constants); the `bu_to_audio_space` unit seam, applied at all four position
  call sites; the underwater-filter `Mix::DRY`/`Mix::WET` bypass.
- **Live engine consumers, corrected count**: **three**, not the two the
  audit-audio skill's own text names — `footstep_system` (`play_oneshot`),
  `water_audio_system` (`play_oneshot` + `set_underwater`; audited fully
  this cycle after the skill's own scope section was found to omit it
  entirely), and `dispatch_region_ambient_music` (`play_music`/`stop_music`).
  `reverb_zone_system` remains the only `set_reverb_send_db` caller. The
  entity dispatch path (`spawn_oneshot_at`/`AudioEmitter`/`OneShotSound`)
  still has zero engine callers (#3086, still open, still latent).
- **Scheduler ordering, resolved definitively this cycle**: read from
  `crates/core/src/ecs/scheduler.rs` directly rather than inferred from
  comments. `StageData` holds two disjoint vectors, `parallel` and
  `exclusive`; `Scheduler::run` always drains a stage's entire `parallel`
  batch (blocking rayon `par_iter_mut`) before running its `exclusive` list.
  This means: (a) `reverb_zone_system` (parallel) is *always* complete
  before `audio_system`/`footstep_system`/`water_audio_system` (all
  exclusive) run, regardless of registration order — no same-frame race
  exists; (b) exclusive-vs-exclusive ordering *is* plain registration order
  (sequential `Vec` iteration), which is what makes `footstep_system`
  (registered before `audio_system` in `late.rs`) reliably precede it now
  that both moved into `Stage::Late`.
- **`footstep_system` relocated**: moved from `Stage::PostUpdate` to
  `Stage::Late` (commit `1382efb0`), fixing a one-frame pose-staleness issue
  in player/third-person mode (the same class of bug #3652 already fixed for
  `make_billboard_system`), and gained a declared `Access` for the first
  time. The same-frame producer→consumer edge into `audio_system` (both now
  `Stage::Late` exclusives) still holds under the new mechanism.
- **Pending phases (correctly unbuilt, not flagged)**: 3.5b FOOT records →
  per-material footstep sound; REGN `incidental`/`sounds`; per-cell acoustics
  beyond binary interior/exterior; raycast occlusion attenuation; CELL-level
  MUSC (ZNAM/XCMO) — still zero consumers, unrelated to and independent of
  the REGN `music_form` field.
- **REGN ambient music, precisely characterized**: the dispatch *mechanism*
  (crossfade, change-gating at three call sites, loop-region application,
  fail-closed on every failure layer including the folder-vs-file
  distinction, last-wins archive resolution) is fully built, correct, and
  tested. The FormID *resolution* it depends on is still structurally dead on
  every supported game — Oblivion `RDMD` is a music-category enum, not a
  FormID; Skyrim `RDMO` targets `MUSC`; FNV `RDSB`/`RDSI` target `MSET` — none
  of which this engine decodes yet. #3816 (still open) is the correct single
  tracking issue for that gap; #3915 (`6a571390`) was purely a documentation
  consolidation onto that issue, not a partial fix.

---

## Lifecycle Invariant Matrix

Owned by Dimension 6 per the audit's dedup protocol (Dimensions 1/4/5 point
here rather than duplicating it). Every row re-derived from live source this
cycle at HEAD `b3db49fa`.

| Invariant | State | Anchor |
|---|---|---|
| `AudioWorld` field declaration = drop order: `active_sounds` → `pending_oneshots` → `music` → `reverb_send` → `reverb_send_db` → `listener` → `manager` → `multi_listener_warned` → `underwater` | **HOLDS** | `crates/audio/src/lib.rs:398, 405, 410, 417, 421, 425, 428, 434, 437` |
| Handles that must outlive `manager` (`active_sounds`, `music`, `reverb_send`, `listener`) are all declared before it | **HOLDS** | same anchors — all four precede `manager` (428) |
| `ActiveSound` field order: `entity` → `handle` → `_track` → `underwater_filter` → `underwater` → `unload_fade_ms` → `stop_issued` | **HOLDS** | `lib.rs:345, 346, 347, 350, 351, 358, 367` |
| `ActiveSound._track` underscore name intact, Drop-side-effect only; lands in `active_sounds` before the helper returns | **HOLDS** | field `lib.rs:347`; pushed at `1076` (queue path) / `1231` (entity path) — moved into the literal in the same statement that constructs it |
| Graceful degradation — manager init failure logs WARN, leaves `None`; zero `unwrap()`/`expect()` on the manager `Option` | **HOLDS** | `lib.rs:460-477`; grep confirms 0 hits across the crate |
| Manager capacities exceed kira defaults (512/32), applied in `new()` (#842) | **HOLDS** | consts `lib.rs:327-328`, applied `452-459` |
| `AudioWorld::new()` called exactly once, at boot — never on cell transition/resize | **HOLDS** (anchor moved) | `boot/world.rs:178` inside `build_world`, called once from `main.rs:730`; `boot.rs` itself no longer exists (#3855 split) |
| Lazy listener creation; no frame-1 cold-start panic; `add_listener` failure is transient-retry | **HOLDS** | `lib.rs:968-985` |
| Sticky listener (#849) — only write sites are `new()` (to `None`) and the lazy create; never cleared on entity churn | **HOLDS** | `lib.rs:508, 980`; no clear site anywhere in the crate |
| Multi-listener diagnostic debounced (#843); "first wins" is deterministic (insertion-ordered `SparseSetStorage`, not hash order) | **HOLDS**, with a recorded nuance | debounce `lib.rs:955`; `SparseSetStorage::iter` zips two plain `Vec`s (`crates/core/src/ecs/sparse_set.rs:160-161`). **Nuance** (not a bug): the dense array is *swap-remove*, not append-only, so "first wins = oldest wins" only holds because the sole production insert site (`scene.rs:1336`) runs once and nothing ever removes an `AudioListener` — a future dynamic-listener-reassignment feature that removes the marker would not automatically preserve "oldest wins" |
| `AudioWorld`/`SoundCache` are `Resource` (interior mutability via `&self`); no `&mut World` requirement on either | **HOLDS** | `impl Resource` at `lib.rs:721, 1563`; the crate's one `&mut World` parameter (`spawn_oneshot_at`) is a free helper for callers who already hold it, not a Resource method |
| `audio_system` registered `add_exclusive(Stage::Late, ...)`; body order matches its 5-pass docstring, including the underwater pass | **HOLDS** | registration `byroredux/src/boot/schedule/late.rs:235`; body `lib.rs:883-887` (`sync_listener_pose` → `update_underwater_filters` → `drain_pending_oneshots` → `dispatch_new_oneshots` → `prune_stopped_sounds`) vs doc `850-874` |
| **`reverb_zone_system` (parallel, `add_to_with_access`) completes before `audio_system` (exclusive, `add_exclusive`) every tick** — structural via `Scheduler::run`'s per-stage two-phase loop, independent of registration order | **HOLDS** (mechanism fully resolved from scheduler source this cycle) | `late.rs:165-171` vs `235`; `crates/core/src/ecs/scheduler.rs:93-99 (StageData), 497-514 (Scheduler::run)` |
| `footstep_system` → `audio_system` same-tick ordering | **HOLDS**, mechanism changed since 2026-08-30 | both are now `Stage::Late` **exclusives** (`late.rs:98, 235`); exclusive-vs-exclusive order is plain registration order, and footstep's registration precedes audio_system's. (Was: cross-stage `PostUpdate`→`Late`, pre-`1382efb0`.) |
| `FootstepScratch` capacity restored on the success path **and** the `AudioWorld`-absent bail (#932); `footstep_system` now has a declared `Access` (previously bare `add_exclusive`) | **HOLDS**, and the access-declaration gap is itself now fixed | `late.rs:98-107` |
| `OneShotSound` marker consumed on success **and** both failure arms (#2394) | **HOLDS** | `lib.rs:1202, 1227, 1240` (push), removed `1247-1253` |
| Despawn truncation (#844/#845/#858/SAFE-23): emitter-presence test, stop-then-mark, `retain` only on `Stopped`, `AudioEmitter` removed on completion, applies to looping AND non-looping | **HOLDS** | `lib.rs:1287-1290, 1296-1313, 1316-1329, 1330-1336` |
| Queue-driven sounds (`entity == None`) exempt from despawn truncation | **HOLDS** | `lib.rs:1284-1286` |
| Listener-entity despawn: `sync_listener_pose` early-returns; handle stays sticky; drops with `AudioWorld` at shutdown | **HOLDS** | `lib.rs:940-942` |
| Cross-cell music continuity — `dispatch_region_ambient_music` gated on the resource's *prior* `music_form` at every call site | **HOLDS**, **three** call sites now (one more than the prior report enumerated — a deferred-apply interior-load job grew alongside the synchronous path) | `cell_loader/load.rs:657-666`, `cell_loader/load.rs:1022-1029`, `scene/world_setup.rs:557-562` |
| Single global reverb send track; per-new-track opt-in gate (`is_finite() && > SILENCE_DB`) identical at both dispatch paths via `apply_reverb_send` | **HOLDS** | `lib.rs:288-299`, called at `1048-1052` (queue) and `1182-1186` (entity) |
| Underwater filter construction identical at both dispatch sites, routed through one `apply_underwater_filter` call site | **HOLDS** | queue `lib.rs:1053-1055`; entity `1187-1189`; regression-pinned by a test asserting `FilterBuilder::new()` appears exactly once in the file |
| `SoundCache`: lowercase-once at all 3 key sites; dormant in the engine binary (`len() == 0` steady-state, `bytes_estimate` correctly documented as unwired) | **HOLDS**, no new consumer landed this cycle | `lib.rs:1481, 1489, 1505` (keys); `lib.rs:1554-1560` (`bytes_estimate`); `grep` confirms zero new call sites in `byroredux/src/` beyond the pre-existing telemetry read |

---

## Findings

### AUD-2026-09-11-D6-01: `reverb_zone_system`'s ordering-guarantee comment has drifted stale for the second time — wrong file (again) and, newly established this cycle, a mechanism that was never actually correct

- **Severity**: LOW
- **Dimension**: Reverb Send & Routing / Manager Lifecycle & ECS/Cell Streaming / Gameplay Audio Wiring (independently re-derived by all three dimension agents; reported once here to avoid triple-filing the same finding)
- **Location**: `byroredux/src/systems/audio.rs:55-58`
- **Status**: Regression of #3522 (closed 2026-08-31 by `4dabfbaf`, which
  fixed the file-attribution half of the original finding but restated,
  rather than corrected, the mechanism half — and has since drifted stale
  on the path too, following the unrelated `8c5e02aa` `boot.rs` → `boot/`
  split on 2026-09-09)
- **Description**: The live comment on `reverb_zone_system` reads:
  ```rust
  /// Runs in `Stage::Late` alongside `audio_system` — registered earlier
  /// in `boot.rs::build_scheduler` (systems within a stage run in
  /// registration order) so the send level is in place before any new
  /// spatial track gets constructed this frame.
  ```
  Two independent problems, both disproved against live source by three
  separate dimension agents working from different entry points (reverb
  routing, field-drop-order, gameplay wiring) and all reaching the same
  conclusion:

  1. **Stale file reference.** `boot.rs` no longer exists — it was split
     into `byroredux/src/boot/` under commit `8c5e02aa` ("refactor(boot):
     split boot.rs into boot/ — one file per concern", 2026-09-09).
     `reverb_zone_system`'s actual registration is at
     `byroredux/src/boot/schedule/late.rs:165-171`, inside
     `register_late_systems`.
  2. **The stated mechanism was never correct for this pairing, even
     before the file split.** "Systems within a stage run in registration
     order" is true for *exclusive-vs-exclusive* ordering (a plain
     sequential `Vec` iteration — `crates/core/src/ecs/scheduler.rs:511-514`)
     but false for the *parallel-vs-exclusive* relationship this comment is
     actually describing. `reverb_zone_system` is registered via
     `add_to_with_access` (`late.rs:165`), landing in `Stage::Late`'s
     **parallel** vector; `audio_system` is a bare `add_exclusive`
     (`late.rs:235`), landing in the **exclusive** vector. `Scheduler::run`
     (`scheduler.rs:497-514`) always drains a stage's entire parallel batch
     (via a blocking `rayon::par_iter_mut`) before running a single entry
     from its exclusive list — this is what actually guarantees the
     ordering, and it is completely independent of which call textually
     appears first in `late.rs`. In fact `reverb_zone_system` is registered
     *after* three other `Stage::Late` **exclusives**
     (`make_billboard_system`, `footstep_system`, `ragdoll_writeback_system`
     — `late.rs:71-121`) and still runs before every one of them, precisely
     because it isn't one.

  #3522's original filing already correctly identified mechanism (b) as
  the deeper problem (citing `scheduler.rs`'s parallel/exclusive split
  directly), but the closing commit `4dabfbaf` only fixed clause (a) — it
  swapped `main.rs` for `boot.rs::build_scheduler` — while *restating*
  clause (b) in slightly different words ("registered earlier... systems
  within a stage run in registration order") rather than adopting the
  language #3522 itself already proposed. The mechanism claim was never
  actually corrected; it was reworded, and has now drifted stale on the
  path too as an unrelated refactor moved the registration site nine days
  later.
- **Evidence**: `late.rs:165-171` (`add_to_with_access`, parallel) vs.
  `late.rs:235` (`add_exclusive`, exclusive); `scheduler.rs:93-99`
  (`StageData`'s two-vector layout) and `scheduler.rs:497-514`
  (`Scheduler::run`'s two-phase per-stage loop); contrast with the
  *correct* statement of this exact mechanism already in-tree four lines
  from `audio_system`'s own registration, `late.rs:225-234` ("registered as
  **exclusive** so it sequences after the Late parallel batch... exclusive
  sequencing makes the dependency structural"), and in
  `byroredux/src/boot/schedule/mod.rs:27-34`'s own top-of-file explanation.
- **Impact**: Documentation-only — three independent re-derivations from
  the actual `Scheduler` source this cycle confirm the runtime behavior is
  correct and the ordering is structurally guaranteed, not incidental. The
  blast radius is a future refactor: this is exactly the comment a
  maintainer reads while touching `reverb_zone_system`, and it teaches a
  false mental model ("registration order" as a blanket rule) that a
  plausible future change — converting `reverb_zone_system` to an exclusive
  registered after `audio_system`, the same kind of parallel→exclusive
  conversion `make_billboard_system` and `footstep_system` already
  underwent (#3652) — would silently violate without any warning that the
  real invariant (staying in the `.parallel` bucket while `audio_system`
  stays `.exclusive`) had broken.
- **Related**: #3522 (closed; this is the unfixed remainder of its own
  original two-part finding, now further stale on path); #3855/`8c5e02aa`
  (the boot split that broke the file reference); #3652 (the
  parallel→exclusive conversions that make the mechanism risk concrete
  rather than hypothetical).
- **Suggested Fix**: Replace the parenthetical with the mechanism, not a
  file path that can drift again: state that `reverb_zone_system` is a
  `Stage::Late` **parallel** registration and `audio_system` a `Stage::Late`
  **exclusive**, and that `Scheduler::run` completes a stage's entire
  parallel batch before starting its exclusive list
  (`crates/core/src/ecs/scheduler.rs`) — independent of where either call
  appears in `boot/schedule/late.rs`. Consider a lightweight test that greps
  `late.rs` for both registrations and asserts `reverb_zone_system` uses
  `add_to*` while `audio_system` uses `add_exclusive*`, so a future
  accidental swap trips a test instead of relying on a comment surviving
  the next refactor.

### AUD-2026-09-11-D7-01: `water_audio_system`'s splash/ripple attenuation and the ripple intensity-damping factor are uncommented magic numbers, unlike the sibling footstep attenuation in the same file

- **Severity**: LOW
- **Dimension**: Gameplay Audio Wiring
- **Location**: `byroredux/src/systems/audio.rs:303-306, 314-318, 319`
- **Status**: NEW
- **Description**: `footstep_system`'s attenuation literal carries an inline
  rationale (`Attenuation { min_distance: 0.5, max_distance: 12.0 }`,
  commented "Tighter attenuation than the default — footsteps drop off fast
  in real environments. 0.5m → full volume, 12m → inaudible",
  `systems/audio.rs:203-208`). `water_audio_system`'s two `Attenuation {
  min_distance: 1.0, max_distance: 24.0 }` literals (splash and ripple) and
  the ripple-only `* 0.45` intensity-damping factor carry no such comment.
  The values themselves are plausible (tighter than
  `Attenuation::default()`'s `{2.0, 30.0}`, consistent with a near-surface
  sound) and are not being second-guessed here — this is a
  documentation-parity gap between two sibling systems in the same file,
  the same class of gap AUD-2026-08-30-D5-01 already flagged (and closed)
  for the crate's `ReverbBuilder` literals.
- **Impact**: Low — a future tuning pass touching water audio has no
  recorded rationale to preserve or deliberately deviate from.
- **Suggested Fix**: A one-line comment on each `Attenuation` literal and on
  the `0.45` factor (e.g. "ripples read quieter than a direct splash — chosen
  by ear, no cited source") closes the parity gap cheaply.

---

## Disproved candidates (investigated, not reported)

Recorded so the next cycle doesn't re-derive them.

- **The briefing's open concurrency question** — whether `reverb_zone_system`
  (a parallel `Stage::Late` registration, textually interleaved among
  several exclusives) could race `audio_system`'s same-tick dispatch of new
  spatial tracks. **Disproved, definitively**, by three independent reads of
  `crates/core/src/ecs/scheduler.rs`: the parallel/exclusive split is a
  structural property of `StageData`'s two separate vectors and
  `Scheduler::run`'s two-phase per-stage loop, not an artifact of
  registration order. No race exists; see AUD-2026-09-11-D6-01 for the
  (documentation-only) residual defect this investigation surfaced.
- **`SparseSetStorage`'s swap-remove dense array undermining "first wins =
  oldest wins" for multi-listener resolution.** Investigated as a genuine
  data-structure property (swap-remove, not append-only), but disproved as
  *currently reachable*: the sole production `AudioListener` insert site
  (`scene.rs:1336`) runs exactly once and nothing in the tree ever removes
  the marker, so the dense array never exceeds one entry. Recorded as a
  nuance in the invariant matrix, not filed as a finding, for whoever first
  wires dynamic listener reassignment.
- **`RIPPLE_COOLDOWN_SECS` decay starving when `splash_sound` is `None`.**
  Re-checked against current code: `water_audio_system` returns before the
  cooldown block is ever reached in that case, and `splash_sound` has
  exactly one writer in the whole tree (boot-time only, never cleared). No
  path leaves a stale cooldown.
- **`WaterAudioState` absence silently disabling splash SFX too** (a
  cross-coupling that looked suspicious on first read — the
  cooldown-resource-absent branch returns before *any* `play_oneshot` call,
  including splashes, which don't conceptually need the cooldown map). Not
  escalated: `WaterAudioState::default()` is inserted unconditionally at
  boot and never removed, so the resource is never actually absent in a
  running engine.
- **Three REGN-dispatch call sites instead of two** (one more than the
  2026-08-30 report enumerated). Confirmed legitimate growth — a
  background/deferred-apply interior-cell-load job sits alongside the
  original synchronous path, applying the identical change-gated dispatch
  pattern — not a missed or duplicated guard.
- **Effect-handle leak on a failed `add_spatial_sub_track`** (queue/entity
  dispatch paths construct an `underwater_filter`/reverb-send opt-in before
  the track-construction call that might fail). Disproved by tracing kira's
  builder/handle construction ordering: the effect handle is a
  command-channel endpoint whose corresponding audio-thread state is only
  registered once the *track* construction succeeds; the `Err` arm (both
  paths `continue` past a never-attached handle) leaves nothing registered
  to leak against.
- **`is_music_active()` gaining a real caller changing the D4-02 doc fix's
  risk profile.** `byroredux/src/commands/world_info.rs` now calls it from a
  console diagnostic (not present at the 2026-08-30 audit). Not escalated:
  the doc is now accurate, so the console output correctly (if
  conservatively) reads "no active channel" during the ~3s crossfade
  fade-out tail — a cosmetic limitation of a debug command, fully explained
  by the now-correct contract.

---

## Future-Phase Readiness (invariants pinned for the next phase)

- **FOOT / 3.5b (per-material footstep sound)**: `FootstepConfig.
  default_sound` decoupling, `FootstepScratch` capacity reuse on both live
  paths, the metre-authored `{0.5, 12.0}` attenuation shape, the
  now-BU-correct `stride_threshold`, and the BU→metre position seam all
  survive and are fixed relative to the 2026-08-30 report's two open carry-
  overs (#3520, AUD-2026-08-30-D7-01) — both are now closed, so 3.5b no
  longer inherits either defect.
- **REGN ambient music**: the dispatch mechanism (crossfade, change-gating,
  loop-region application, fail-closed on every failure layer, last-wins
  archive resolution) is fully built, correct, and — per Dimension 4 — has
  one recorded but deliberately unfiled test-coverage gap: no single test
  exercises the full chain from a resolvable, `looping: true` REGN directive
  through to a kira handle that actually survives past the source track's
  natural end. This is intentionally deferred rather than filed, since
  #3816's still-missing FormID resolution means no supported game can reach
  that success branch with real data today; Dimension 4's recommendation is
  to add the join-point test in the same PR that lands #3816's MUSC/MUST/
  MSET decode, not before.
- **#3816 (MUSC/MUST/MSET/RDMD decode)**: still the correct single tracking
  issue for "REGN ambient music never resolves in production on any
  supported game." Re-verified this cycle that #3915 was a pure
  documentation consolidation (corrected a false "FNV inherits RDMD" claim,
  re-pointed citations) with zero code-path changes — it did not partially
  unblock any game.
- **CELL-level MUSC (ZNAM/XCMO)**: still zero consumers, confirmed via a
  fresh grep this cycle. Independent of and unrelated to REGN's `music_form`
  field — whichever caller lands first will need to arbitrate against
  `dispatch_region_ambient_music` for the single music slot, a design
  decision this report does not make for them.
- **`SoundCache` first consumer**: still dormant, `len() == 0` steady state,
  no new call site landed this cycle. Whoever wires the first producer
  should wire eviction and `bytes_estimate` telemetry in the same commit,
  per the standing recommendation from prior cycles.
- **Scheduler-ordering documentation debt**: AUD-2026-09-11-D6-01 is the
  second time this exact comment has drifted stale after a partial fix.
  The suggested lightweight regression test (grep `late.rs` for both
  registration calls, assert the parallel/exclusive split) would make a
  third recurrence a test failure instead of a documentation nit, and is
  cheap enough to land alongside the comment fix itself.

---

## Suggested next step

```
/audit-publish docs/audits/AUDIT_AUDIO_2026-09-11.md
```

Domain label: `audio`. AUD-2026-09-11-D6-01 also warrants `tech-debt` /
`doc-rot` (it is a documentation-only regression, not a runtime bug).
AUD-2026-09-11-D7-01 likewise warrants `tech-debt` / `doc-rot`.
