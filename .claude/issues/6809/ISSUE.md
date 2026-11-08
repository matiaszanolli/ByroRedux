# CONC-2026-11-08-3.2: Health/Position fixtures held in plain order by two tests; every_query_kind_hands_off_and_untracks_cleanly fails intermittently under the lock-order detector

_Immutable snapshot of the issue as filed (GitHub is authoritative for state). Source: docs/audits/AUDIT_CONCURRENCY_2026-11-08.md. Labels: medium,concurrency,ecs,bug_

Found by `/audit-concurrency` 2026-11-08 (`docs/audits/AUDIT_CONCURRENCY_2026-11-08.md`, HEAD 0419437ab).

- **Severity**: MEDIUM — same class and test-only scope as CONC-2026-11-08-3.1, but it fails ~1 in 100 parallel runs, and a flaky red lane is worse to triage than a deterministic one.
- **Dimension**: ECS Lock Ordering (core test lane)
- **Location**: plain-order holders `multiple_read_queries_coexist` and `query_read_then_write_different_types_ok`; sorted-pair victim `every_query_kind_hands_off_and_untracks_cleanly` (all in `crates/core/src/ecs/world_tests.rs`; the poison test also calls `query_2_mut::<Position, Health>`)

## Description
Two tests hold `Health` and `Position` in plain order (`query::<Health>` then `query::<Position>` / `query_mut::<Position>`). The victim takes the pair through the TypeId-sorted `query_2_mut::<Health, Position>`. In this build `TypeId(Position) < TypeId(Health)`, so whichever side inserts its edge second closes a cycle in the process-wide graph.

## Evidence
Captured failure: "attempted acquisition of `Health` while holding `Position` … `Health` → `Position` → `Health`", first test frame at the victim. Pair-running the victim single-threaded against each other test fails for exactly those two tests and no other of the 86 `ecs::world::tests`. It passes under whole-suite `--test-threads=1`. Observed in 2 of ~190 parallel runs of the multi-crate-lane core binary.

## Impact
Intermittent red of the core lib lane / CI `lock-order-check`.

## Suggested Fix
Same as CONC-2026-11-08-3.1: give these two tests private fixture types, or have them take the pair through `query_2_mut` / a sorted order. Add a one-line doc on the shared `Health`/`Position` fixtures that nothing may hold them in plain order.

## Completeness Checks
- [ ] **SIBLING**: other shared-fixture pairs held in plain order (`ResA`/`ResB`, see 3.1)
- [ ] **LOCK_ORDER**: sorted acquisition preserved
- [ ] **TESTS**: 200+ parallel runs of the core binary under `BYRO_LOCK_ORDER_CHECK=1` show no failure
