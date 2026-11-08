# CONC-2026-11-08-3.1: resource_2_mut_both_writable closes a lock-order cycle with itself; core lib lane and CI lock-order-check are red

_Immutable snapshot of the issue as filed (GitHub is authoritative for state). Source: docs/audits/AUDIT_CONCURRENCY_2026-11-08.md. Labels: high,concurrency,ecs,bug_

Found by `/audit-concurrency` 2026-11-08 (`docs/audits/AUDIT_CONCURRENCY_2026-11-08.md`, HEAD 0419437ab).

- **Severity**: HIGH under the red-lane rule used for the 10-27 cycles. It is a test body on one thread; no shipped thread can reach it, but it reddens CI `lock-order-check` and masks any real cycle behind it.
- **Dimension**: ECS Lock Ordering (core test lane)
- **Location**: `crates/core/src/ecs/world_tests.rs` (`resource_2_mut_both_writable`)

## Description
`resource_2_mut::<ResA, ResB>()` takes the pair in TypeId order and records that edge. The same test then holds `world.resource::<ResA>()` and `world.resource::<ResB>()` together in plain order. When `TypeId(ResB) < TypeId(ResA)` the sorted edge is B→A and the plain-order hold closes A→B inside one test. Same family as #6101 / #6234 (deterministic per build configuration).

## Evidence
- Fails in isolation, single-threaded: `ecs::world::tests::resource_2_mut_both_writable` panics in `lock_tracker.rs` with "attempted acquisition of `ResB` while holding `ResA` … `ResB` → `ResA` → `ResB`". Passes with `BYRO_LOCK_ORDER_CHECK` unset.
- Passed at the 2026-10-31 baseline (core 888/0). Red at HEAD in every form tried: multi-crate `--lib` lane (7/7), `-p byroredux-core --lib` (12/12), `--features inspect`, and the CI form `cargo test --workspace --no-fail-fast --exclude byroredux-ui`. The TypeId order flipped in the window (likely `Cargo.lock`/`Cargo.toml` churn; cause not pinned).

## Impact
CI `lock-order-check` is red. The audit skill's "core alone stays green" no longer holds.

## Suggested Fix
Read the two values in separate statements (`let a = world.resource::<ResA>().0;` then `...ResB...`) or use `try_resource_2::<ResA, ResB>()`, which sorts. Better: give every test that deliberately holds an unsorted pair its own private fixture types, so a plain-order hold cannot collide with a sorted-pair test in the process-wide graph. Related: CONC-2026-11-08-3.2 (same fix).

## Completeness Checks
- [ ] **SIBLING**: other tests in `world_tests.rs` that hold two shared-fixture types in plain order (see also the Health/Position pair)
- [ ] **LOCK_ORDER**: TypeId-sorted acquisition preserved in the fixed test
- [ ] **TESTS**: the core lane is green under `BYRO_LOCK_ORDER_CHECK=1` in the multi-crate, core-alone and CI forms
