# RT-1: Skyrim Dragonsreach bench-window FPS collapsed 321→8.7 — ECS scheduler stalls ~140 ms/frame for ~28 s

**Severity**: HIGH
**Dimension**: performance / ecs (scheduler) — surfaced via runtime telemetry
**Location**: `atw_scheduler` stage in `byroredux/src/main.rs:2272` (bench timing) + `crates/core/src/ecs/scheduler.rs` (parallel system scheduler); per-frame systems registered in `byroredux/src/main.rs` / `byroredux/src/systems.rs`. Evidence: `/tmp/audit/runtime/skyrim_se-WhiterunDragonsreach.engine.log`.
**Status**: NEW (CONFIRMED against live telemetry 2026-06-23)

## Description
On the heaviest baselined interior (WhiterunDragonsreach, 6049 entities, 294 newly-parsed meshes), the 240-frame bench window runs at a **steady** ~7 fps / dt≈147 ms for its entire ~28 s duration, then recovers instantly to 555–697 fps / dt≈1.5 ms the moment the window ends. The cost is **entirely CPU-side in the scheduler stage** — `wall_ms=114.3`, `systems_ms=113.5`, while `draw_ms=0.9` and every GPU pass reads ~0. The per-second `cpu_ms` breakdown pins it precisely: `atw_scheduler=138..147` ms during the window vs `atw_scheduler=1` ms once warm. This is a 37× regression against the contract metric (baseline 321.1 fps from AUDIT_RUNTIME_2026-06-14).

## Evidence
```
bench: frames=240 wall_fps=8.7 wall_ms=114.31 ... draw_ms=0.87 systems_ms=113.54 entities=6049 draws=2445/2b/4c
cpu_ms: ... atw_scheduler=138 atw_post=1   (during window)
cpu_ms: ... atw_scheduler=143 atw_post=1   (during window)
```
The same metric on all four other games shows `systems_ms` 0.14–1.18 ms with zero dt>100 ms frames — the pathology is unique to this cell.

## Impact
The first ~28 wall-seconds after entering Dragonsreach (or any cell of comparable scheduler load) render at ~7 fps — a multi-second hitch on cell entry. Reproducible across two runs (run 2: 8.5 fps / systems_ms=116.3 / 27 slow seconds). The recover-after-N-frames shape points to a transient backlog draining through the scheduler (candidates: first-frame query-cache population across the 294 fresh meshes, deferred BLAS/descriptor warm-up serialized onto the main scheduler, or a newly-added per-frame system doing one-time-amortized work). It is **not** the M47.2 scripting systems (`trigger_detection_system` / `recurring_update_tick_system` iterate only the sparse `TriggerVolume` / `RecurringUpdate` sets — and an O(entities) system would not self-recover after 28 s).

## Suggested Fix
Bisect bench-window `atw_scheduler` on Dragonsreach across the 06-14→06-23 range (`git log --since=2026-06-14 -- crates/core/src/ecs/ byroredux/src/systems.rs byroredux/src/main.rs`). Add a one-line per-system-cost dump for the first 60 frames (the scheduler already times each system post-#1647) to name the offending stage, then decide whether the backlog should be amortized across frames or moved off the per-frame scheduler. Pair with `/audit-performance` and `/audit-ecs`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked across other heavy interiors (any cell with comparable fresh-mesh + scheduler load)
- [ ] **LOCK_ORDER**: If a RwLock scope changes in the scheduler, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test (or the runtime baseline gate) pins this specific fix
