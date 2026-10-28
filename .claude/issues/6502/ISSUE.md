# RT-6: Regression of #3249: recursive-read hazard warning always reports `lock_tracker.rs:172:30` (Location::caller() inside a non-track_caller closure)

Labels: medium, ecs, concurrency, bug
Source: docs/audits/AUDIT_RUNTIME_2026-10-27.md

**Report**: `docs/audits/AUDIT_RUNTIME_2026-10-27.md` · **Finding**: RT-6 · **Severity**: MEDIUM (the hazard it exposes is HIGH-class; the filed defect is the diagnostic that cannot locate it) · **Dimension**: ECS runtime hazards / diagnostics
**Status**: Regression of #3249 (closed)
**Location**: `crates/core/src/ecs/lock_tracker.rs` - `track_read` (`#[track_caller]`, line ~96) calls `std::panic::Location::caller()` at line ~172, **inside** the `LOCKS.with(|locks| { ... })` closure.

## Description
`track_read` is `#[track_caller]`, but `Location::caller()` is evaluated inside the `LOCKS.with(|locks| ...)` closure. The closure is not `#[track_caller]`, so `caller()` returns the closure's own location (`lock_tracker.rs:172:30`), not the user call site. #3249's fix (`a0b1598dc`) put the call inside the closure from the start, so the call site was never reported. Every live recursive-read warning therefore reads `acquired at crates/core/src/ecs/lock_tracker.rs:172:30`, which defeats the purpose of #3249 (the doc comment on the function even claims it names `render/static_meshes.rs:142`-style sites).

Live hazards observed (headless captures) that cannot be located because of this:
- `QuestStageState` on FNV GSProspectorSaloonInterior at the first game-loop frame.
- `QuestStageState` on FO4 InstituteBioScience right after SCEN phase transitions (`DN054DragRaceScene`, `Companion_PrestonHero_Scene`).
- `AnimationPlayer` on Oblivion Tamriel at bootstrap quest start (attributed to ECS-2026-10-27-01's `apply_pending_group_swaps` -> `sequence_player` re-query).

A nested `QuestStageState` read was fixed once before (`35f1cf051`); another nesting remains, which the static ECS and concurrency audits did not find.

## Evidence
`ECS recursive-read hazard: a second byroredux_scripting::quest_stages::QuestStageState read guard is live on this thread, acquired at crates/core/src/ecs/lock_tracker.rs:172:30` in the FO4 InstituteBioScience engine log and the FNV quicksave session log; the same text for `AnimationPlayer` in the Oblivion exterior runs. Code confirmed at HEAD: `let caller = std::panic::Location::caller();` sits inside the closure.

## Impact
A live deadlock-class hazard in quest-stage code under the parallel scheduler (std `RwLock` is writer-preferring on Linux, so a recursive read deadlocks behind a parked writer), and the one diagnostic built to locate such hazards cannot locate it.

## Related
ECS-2026-10-27-01 (live-confirmed by the Oblivion warning); #2386; #3249 (closed); #3696.

## Suggested Fix
Capture `let caller = std::panic::Location::caller();` at the top of `track_read`, outside the closure, and use it inside. Add a test that the warning names the caller's file (not `lock_tracker.rs`). Then re-run the FNV / FO4 captures to locate the `QuestStageState` nesting (separate fix).

## Completeness Checks
- [ ] **LOCK_ORDER**: No change to TypeId-sorted acquisition or the global-order check
- [ ] **SIBLING**: `track_write` / other `Location::caller()` uses in the module checked for the same closure pattern
- [ ] **TESTS**: A regression test pins that the recursive-read warning names the user call site

