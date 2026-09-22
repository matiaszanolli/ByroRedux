# PERF-D7-2026-09-21-01: Every cell unload now runs two per-victim capture passes that probe storages one lock at a time, and neither is inside any `UnloadPhaseTimings` bucket

**Labels**: bug, low, performance, terrain-exterior

Filed via /audit-publish from docs/audits/AUDIT_PERFORMANCE_2026-09-21.md.

**Severity**: LOW (streaming hitch, unmeasured) · **Dimension**: 7 — Streaming & Parse
**Location**:
- `byroredux/src/cell_loader/unload.rs:245-246`: the two capture calls, which sit between `timings.ownership_index` (`:238`) and the next `phase_started` (`:255`)
- `byroredux/src/cell_loader/reference_state.rs:96-166`: `capture` (`d8255b2e2`, 2026-09-16)
- Sibling: `byroredux/src/cell_loader/stream_snapshot.rs:181-214`, `capture_actor_snapshots` (#3299)

**Status**: NEW
**Verified against**: HEAD `73aaed7b9`; the code is identical to the audited `f97775ca8`.

## Description

- `victims` is every entity of the unloading cell. `reference_state::capture` calls `world.get::<FormIdComponent>` on each one. Every call is a TypeId lookup plus a tracked `RwLock` read.
- For placement roots, which carry a FormID, it also does a `FormIdPool` resource lookup and resolve, then `Inventory`, `Dead` and `PickedUp` probes, before it can skip.
- `stream_snapshot::capture_actor_snapshots` walks the victims the same way. For each one it probes `global_form_id`, `Transform`, `Seated`, `AmbientPackageRuntime`, `TravelState` and `Traveled`.
- A radius-3 crossing unloads a 7-cell ring in one batch.
- Both calls sit between the end of the `ownership_index` phase and the next `phase_started`. Their cost therefore lands in none of the `UnloadPhaseTimings` phases: `ownership_index`, `handle_collection`, `gpu_release`, `owned_state_release`, `despawn`, `finalization`.

## Impact

*Est.* sub-ms to ~1 ms of main-thread time per boundary crossing. This is unmeasured, and the unload phase split cannot show it.

## Related

- #3299: the stream-snapshot pass (EX-16 item 4).
- #3690 (closed): the previous per-victim unload cost finding (cinematic-retention set).

## Suggested Fix

Acquire each pass's queries once, hoisting the `world.query::<T>()` guards out of the per-victim loop. Alternatively, merge the two passes into one walk over placement roots. Time the capture as a `snapshot_capture` phase in `UnloadPhaseTimings`.

Source: docs/audits/AUDIT_PERFORMANCE_2026-09-21.md (PERF-D7-2026-09-21-01)

## Completeness Checks
- [ ] **LOCK_ORDER**: If query guards are hoisted across the loop, multi-storage acquisition stays TypeId-sorted and no guard is held across a `resource_mut` write
- [ ] **SIBLING**: Other per-victim passes in `unload_cell_inner` are checked for the one-lock-per-probe shape
- [ ] **TESTS**: The existing streaming/unload tests still pass, and the new phase appears in `UnloadPhaseTimings` (and its `absorb`)

