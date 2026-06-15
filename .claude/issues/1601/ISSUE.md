# CONC-2026-06-14-01: ragdoll_writeback_system reintroduces a declared GlobalTransform WriteWrite conflict in the Stage::Late parallel batch

- **Issue**: #1601
- **Severity**: MEDIUM
- **Labels**: medium, sync, bug
- **Dimension**: Scheduler Access Declarations (primary) / RwLock Patterns (Resource↔Storage, Physics)
- **Location**: `byroredux/src/main.rs:864-871` (ragdoll registration), `byroredux/src/main.rs:846-858` (camera_follow registration), `byroredux/src/main.rs:933-938` (build-time guard), `byroredux/src/ragdoll.rs` (writeback body)
- **Source**: `docs/audits/AUDIT_CONCURRENCY_2026-06-14.md` (CONC-2026-06-14-01)
- **Status when filed**: NEW, CONFIRMED — introduced by PR #1529 (PHYSAL ragdoll).

## Description
PR #1529 added a second `Stage::Late` parallel system writing `GlobalTransform`. Both `camera_follow_system` and `ragdoll_writeback_system` declare `writes::<GlobalTransform>()`; `analyze_pair` classifies this as `ConflictKind::WriteWrite`, pushing `known_conflict_count()` from 0 to 1. The build-time guard asserts only `undeclared_parallel_count() == 0`, so the conflict slips through.

## Impact
1. Diagnostic regression — `sys.accesses` prints `1 known conflicts`, breaking the post-M27 "0 conflicts" invariant.
2. Not a data race today — entity-disjoint writes serialised by the per-storage RwLock; loses intended parallelism on GlobalTransform when a ragdoll is active.
3. Latent hazard — exactly the pattern M27 exclusive-staging eliminated.
4. Secondary (cross-ref #1375) — LocalBound/WorldBound lag check for ragdoll bones.

## Suggested Fix
Demote `ragdoll_writeback_system` to `add_exclusive(Stage::Late, …)`, mirroring M27 Phase 3 treatment of `audio_system`. Pair with #1602.

## Related
#1394 (guard scope, CLOSED); #1375 (Late GT writers / stale bounds, CLOSED); #1602 (the guard gap that let this land).
