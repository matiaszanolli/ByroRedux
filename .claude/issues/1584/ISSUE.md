**Severity**: MEDIUM · **Dimension**: CPU Hot Paths · **Status**: NEW
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-06-14.md` (F5)

## Description
The per-frame `about_to_wait` pre-scheduler phase builds two brand-new `HashSet<u32>` and does two full O(entity_count) component walks (`MeshHandle`, `TextureHandle`) to compute counts surfaced only via the `stats` command / window title / debug-UI panel. The block runs **unconditionally** — outside the `config_debug` gate (which starts at `byroredux/src/main.rs:2186`) and with no throttle.

## Evidence
Verified live (`byroredux/src/main.rs:2093-2111`): two fresh `std::collections::HashSet::new()` populated by `for (_, h) in q.iter()` walks over `MeshHandle` and `TextureHandle`, returning only `.len()`. The `CpuFrameTimings::atw_pre_ms` doc-comment (`crates/core/src/ecs/resources.rs:566-572`) already flags this walk as a growth risk.

## Impact
Two heap allocations + two rehash-growth sequences + two full component walks per frame, scaling with cell entity count; pure waste when nothing reads the result. Not frame-dominating on a 7950X but defeats the "zero steady-state allocations" posture of the rest of the hot path. No quantitative guard exists (#1381).

## Suggested Fix
(a) Gate the block on a live consumer (`config_debug` OR `debug_ui.visible` OR the existing 16-frame window-title throttle), and/or (b) hoist the two `HashSet`s to persistent `App` fields and `clear()`+reuse. Option (a) is strictly better — the walk itself is wasted when nothing reads it.

## Related
#1381 (dhat / alloc-counter coverage unwired — the missing quantitative guard).

## Completeness Checks
- [ ] **SIBLING**: Check other unconditional per-frame ECS walks in `about_to_wait` for the same ungated-cost pattern
- [ ] **TESTS**: Wire the dhat alloc bound for this path (cross-link #1381) so the gate is regression-locked
