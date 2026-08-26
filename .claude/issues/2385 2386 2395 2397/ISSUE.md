# Issues 2385, 2386, 2395, 2397 — ECS audit findings (AUDIT_ECS_2026-08-07)

All four are LOW severity, `ecs` domain, filed from `docs/audits/AUDIT_ECS_2026-08-07.md` via `/audit-publish`.

## #2385 — ECS-D1-02: ABBA detector's slow path poisons the global graph
- Location: `crates/core/src/ecs/lock_tracker.rs:253`, `:293-305`, `:324`
- `record_and_check`'s slow path takes `GRAPH.write()` then may `panic!` while holding the write guard, poisoning `GRAPH` permanently. Every later `GRAPH.read()/.write()` uses `.expect("GRAPH poisoned")` — no `PoisonError` resolution, unlike `storage_lock_poisoned::<T>()`/`resource_lock_poisoned::<R>()` in world.rs.
- Fix: compute the cycle verdict under the guard, `drop(graph)`, then panic; resolve both `GRAPH` acquisitions with `.unwrap_or_else(PoisonError::into_inner)`.

## #2386 — ECS-D1-05: Recursive same-type read locking whitelisted, invisible to ABBA graph
- Location: `crates/core/src/ecs/lock_tracker.rs:74, 419-428, 83-84`
- `track_read` counts recursive reads of the same type instead of rejecting; `multiple_reads_same_type_ok` test pins this as intended. No production path currently does this (per audit's scan). Reported as latent gap, not a live bug.
- Suggested Fix: document the hazard on `World::query`, add a debug-only `log::warn!` when `read_count` transitions 1→2.

## #2395 — ECS-D2-NEW-03: `PackedStorage::clear_erased` doesn't release capacity; docs assert it does
- Location: `crates/core/src/ecs/packed.rs:290-298` vs `crates/core/src/ecs/sparse_set.rs:179-191`; doc claims at `storage.rs:110-113`, `world.rs:279-281`
- `SparseSetStorage::clear_erased` calls `shrink_to_fit()` on all 3 vecs (#2148); `PackedStorage::clear_erased` only `clear()`s. Self-heals on next batch removal, not a leak, but asymmetric + docs wrong.
- Fix: add `shrink_to_fit()` to `PackedStorage::clear_erased` + mirror test, or correct the docs.

## #2397 — ECS-D2-NEW-01: `SparseSetStorage::remove_entities_erased` is byte-identical to trait default
- Location: `crates/core/src/ecs/sparse_set.rs:172-177` vs default at `storage.rs:87-92`
- Override adds no early-out (`if self.dense.is_empty() { return; }`) despite trait doc inviting one. No correctness impact; missed optimization + drift risk.
- Fix: either delete the override, or add the early-out (per #2397) — coordinate with #2395/PackedStorage sibling override.

## Domain classification
All four → **ecs** → `byroredux-core` crate for Phase 6 testing.

## Plan
Fix all four together since they're all small, localized changes in the same crate:
1. #2385: lock_tracker.rs poison handling — drop-before-panic + PoisonError resolution
2. #2386: lock_tracker.rs — debug warn on recursive same-type read escalation
3. #2395: packed.rs — add shrink_to_fit to clear_erased + doc correction
4. #2397: sparse_set.rs — add early-out to remove_entities_erased
