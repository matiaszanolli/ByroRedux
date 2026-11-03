# RT-2026-10-31-08: A recursive `QuestStageState` read is live on FNV and FO4 cell load, and now locatable

Labels: medium,ecs,concurrency,scripting,bug,game:fnv,game:fo4
Source: docs/audits/AUDIT_RUNTIME_2026-10-31.md

**Source**: `docs/audits/AUDIT_RUNTIME_2026-10-31.md` RT-2026-10-31-08 (report HEAD `670f81917`).
**Re-verified at HEAD `7f2d98944`**: `crates/scripting/src/condition/eval/global.rs:81` is unchanged; the outer holder is still unidentified. Related closed #6008 (the scene-trigger router held the same guard across `evaluate_condition_list`) and #6502 (the locator fix).

- **Severity**: MEDIUM (the hazard is HIGH-class: the std `RwLock` is writer-preferring on Linux, so a recursive read deadlocks behind a parked writer)
- **Dimension**: ECS runtime hazards (owner `/audit-ecs`, `/audit-scripting`)
- **Location**: second read at `crates/scripting/src/condition/eval/global.rs:81` (`get_stage_done`, `world.try_resource::<QuestStageState>()`); the first guard's holder is not named by the tracker
- **Status**: Existing hazard, open (10-27 RT-6 filed only the locator defect, now fixed by #6502; the hazard itself has no open issue). Related closed #6008.
- **Game / Cell**: fnv / GSProspectorSaloonInterior (seen in the `m47-3` gate and by hand), fo4 / InstituteBioScience (seen in the telemetry capture log at 20:11:39Z)
- **Description**: `lock_tracker` reports "a second `QuestStageState` read guard is live on this thread, acquired at `condition/eval/global.rs:81:29`". The call site is correct now, but the outer holder is still unidentified: it is some caller that keeps a `QuestStageState` read guard while evaluating a CTDA / ObScript condition (the 10-27 report listed SCEN phase transitions on FO4 and the first game-loop frame on FNV). The tracker warns once per type per thread, so only the first instance is ever visible.
- **Evidence**: `/tmp/audit/runtime/fo4-InstituteBioScience.engine.log`, `/tmp/audit/runtime/nuka/err.log`.
- **Suggested Fix**: Find the enclosing guard with `RUST_BACKTRACE`-style capture in the tracker's warn path (print `Backtrace::capture()` for the first instance), then hand the evaluator a snapshot or drop the outer guard before evaluating.

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SIBLING**: Same pattern checked in related call sites
- [ ] **TESTS**: A regression test pins this specific fix
