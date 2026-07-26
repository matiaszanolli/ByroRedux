## Issue 2147 [OPEN] ECS-2507-01: Per-cell wholesale SeatReservations clear frees seats still held by actors in other loaded cells
labels: bug ecs medium 

## ECS-2507-01: Per-cell wholesale `SeatReservations` clear frees seats still held by actors in other loaded cells

**Severity**: MEDIUM
**Dimension**: 7 — Component Lifecycles (M42 seat claims)
**Location**: `byroredux/src/cell_loader/references/mod.rs:195-197`; consumer at `byroredux/src/systems/sandbox.rs:206-217`
**Status**: NEW (from `/audit-ecs` — `docs/audits/AUDIT_ECS_2026-07-25.md`)

### Description

`load_references` clears the entire `SeatReservations` set on every invocation.
`load_references` is called **once per cell**, on both the interior path
(`cell_loader/load.rs:377`) and the exterior grid path (`cell_loader/exterior.rs:418`,
inside the per-`(gx, gy)` cell loader). On an exterior grid load with `--radius 3`
that is 49 wholesale clears; during boundary-crossing streaming it happens again
for every newly-streamed cell while previously-loaded cells (and their seated
actors) are still resident. `Seated` is a one-shot terminal marker, so an actor
that already sat never re-claims its marker after the clear — the seat is
permanently released while still physically occupied, and the next unseated
actor within `SEAT_SEARCH_RADIUS` can claim the same `(furniture entity, marker
index)`.

The in-code rationale is also factually wrong: the comment at
`references/mod.rs:189` says "clear stale seat reservations from the previous
cell (entity ids reset on unload)". Entity IDs are **never** reset or reclaimed
— `World::despawn` explicitly documents this (`crates/core/src/ecs/world.rs:114-118`,
#372) and `next_entity` only ever grows. Stale entries can therefore never
alias a new furniture entity; the clear is only preventing a slow set-growth
leak, at the cost of correctness.

### Evidence

```rust
// byroredux/src/cell_loader/references/mod.rs:195
if let Some(mut r) = world.try_resource_mut::<crate::components::SeatReservations>() {
    r.0.clear();
}
```

```rust
// byroredux/src/systems/sandbox.rs:206-217 — claims are never re-asserted
let mut reservations = world.resource_mut::<SeatReservations>();
for (npc, behavior) in sandbox_q.iter() {
    if seated_q.as_ref().is_some_and(|s| s.contains(npc)) {
        continue; // already seated (one-shot guard) — never re-inserts its claim
    }
    …
    reservations.0.insert(seat_id);
```

### Impact

Two NPCs occupying the same furniture marker in an exterior multi-cell scene.
Gated behind `BYRO_SANDBOX_SIT` (off by default), so no default-configuration
impact today, but it silently breaks the per-marker exclusivity that
`SeatReservations` exists to provide as soon as M42 seating is turned on for
exteriors.

### Related

#372 (IDs never reclaimed); the M42 seat-claim design in
`docs/engine/npc-spawn-ai-packages.md`.

### Suggested Fix

Replace the wholesale `clear()` with a targeted prune — retain only
reservations whose furniture entity still exists
(`r.0.retain(|(furn_e, _)| world.has::<Furniture>(*furn_e))`), which is both
leak-free and cross-cell-safe. Alternatively, have `sandbox_seat_system`
re-assert each `Seated` actor's claim each tick (store the claimed `seat_id`
on the `Seated` marker) so a clear is self-healing. Fix the "entity ids reset
on unload" comment either way.

## Completeness Checks
- [ ] **SIBLING**: Check for the same wholesale-clear-on-cell-load pattern against other per-cell resources (e.g. any other `SparseSetStorage`-backed reservation/claim resource keyed by cross-cell entity identity)
- [ ] **TESTS**: A regression test pins seat reservations surviving a sibling cell load while the seat is still occupied


---
## Issue 2148 [OPEN] ECS-2507-02: SparseSetStorage::sparse is sized by the monotonic EntityId high-water mark and never shrinks
labels: bug ecs medium memory 

## ECS-2507-02: `SparseSetStorage::sparse` is sized by the monotonic `EntityId` high-water mark and never shrinks

**Severity**: MEDIUM
**Dimension**: 2 / 8 — Storage Correctness + Hot-Path / Memory
**Location**: `crates/core/src/ecs/sparse_set.rs:11-13, 58-63, 77-110, 146-151`
**Status**: NEW (from `/audit-ecs` — `docs/audits/AUDIT_ECS_2026-07-25.md`)

### Description

`SparseSetStorage.sparse: Vec<Option<u32>>` is indexed directly by `EntityId`
and grown with `resize(idx + 1, None)` on insert. It is **never** truncated or
shrunk: `remove` writes `None` into the slot but leaves the `Vec` length
untouched, and `clear_erased` calls `.clear()` (which keeps capacity). Because
entity IDs are deliberately never reclaimed (`World::despawn`, #372) and
`next_entity` only grows, every sparse-set storage that ever receives an
insert near the current high-water mark permanently retains
`8 bytes × high_water_mark` of RAM regardless of how few components are
actually live. There are **122** `SparseSetStorage<Self>` component
declarations in the workspace; the ones attached to nearly every spawned
entity (`Name`, `FormIdComponent`, `MeshHandle`, `Parent`, `Children`,
`RenderLayer`, `CollisionShape`, …) all track the global high-water mark.

### Evidence

```rust
// sparse_set.rs:60-63 — only growth, no counterpart shrink
if idx >= self.sparse.len() {
    self.sparse.resize(idx + 1, None);
}
```

```rust
// sparse_set.rs:88 — remove clears the slot but not the length
self.sparse[idx] = None;
```

```rust
// sparse_set.rs:147 — clear() retains capacity
self.sparse.clear();
```

`grep -rn "shrink" crates/core/src/ecs/*.rs` returns nothing — there is no
compaction API anywhere in the ECS core. Cell unload
(`cell_loader/unload.rs:199`) goes through `World::despawn`, which never
touches `next_entity`.

### Impact

RAM growth proportional to (cumulative entities ever spawned) × (number of
sparse component types touched), *independent of live entity count*.
`Option<u32>` has no niche, so each slot is 8 bytes: a 2M-ID high-water mark
costs ~16 MB per affected storage, i.e. a few hundred MB across a dozen
commonly-attached sparse components. This is exactly the shape of a long
exterior-streaming session (repeated cell load → despawn → load at
ever-higher IDs) and it is invisible to `cargo test`. Against the
`docs/engine/memory-budget.md` "under ~4 GB total" target this is material,
though it is a slow accumulation, not a per-frame leak.

### Related

#372 (IDs never reclaimed — the documented decision this interacts with; do
**not** "fix" by reusing IDs). `PackedStorage` is unaffected (its
`entities`/`data` are sized by live count).

### Suggested Fix

Two independent, low-risk mitigations. (a) Halve the per-slot cost by
replacing `Vec<Option<u32>>` with `Vec<u32>` plus a `u32::MAX` sentinel
(4 bytes/slot, same O(1) semantics). (b) Add a `shrink_sparse_tail()` that
truncates trailing `None` slots plus a `shrink_to_fit()`, exposed on
`DynStorage` and invoked once per cell-unload from `unload_cell` — cheap (a
backwards scan) and only runs at load boundaries. Also make `clear_erased`
call `shrink_to_fit()` so a save-load actually returns the memory.

## Completeness Checks
- [ ] **SIBLING**: If a shrink hook is added, verify it composes correctly with `PackedStorage` (unaffected) and doesn't disturb dense-index invariants
- [ ] **TESTS**: A regression test pins RAM/len behavior across a despawn-heavy load/unload cycle (e.g. `sparse` len bounded relative to live entity count, or a shrink hook actually truncating trailing `None`s)


---
## Issue 2149 [OPEN] ECS-2507-03: query/query_mut/query_2_mut* defuse the tracker scope before the wrapper's fallible downcast
labels: bug ecs low sync 

## ECS-2507-03: `query`/`query_mut`/`query_2_mut*` defuse the tracker scope before the wrapper's fallible downcast, orphaning the tracker row on panic

**Severity**: LOW
**Dimension**: 1 — Lock Ordering & Deadlock (tracker hygiene)
**Location**: `crates/core/src/ecs/world.rs:396-397, 417-418, 466-471, 530-535, 544-550`; panic site `crates/core/src/ecs/query.rs:44-48, 116-120`
**Status**: NEW (from `/audit-ecs` — `docs/audits/AUDIT_ECS_2026-07-25.md`)

### Description

`World::query` calls `scope.defuse()` **before** constructing `QueryRead`, and
`QueryRead::new` performs
`downcast_ref::<T::Storage>().expect("storage type mismatch (bug in World)")`.
If that `expect` ever fired, the tracker row would already be un-owned by the
`TrackedRead` scope and not yet owned by a `QueryRead` (whose `Drop` is the
only untrack path), leaving a stale entry in the thread-local `LOCKS` map. A
later acquisition on the same thread after a `catch_unwind` would then report
a spurious "ECS deadlock detected". This is the exact failure mode #137 fixed,
and `World::get` (`world.rs:288-300`) gets the ordering right — it defuses
only inside the `Some` arm, after `ComponentRef::new` has returned. `query`,
`query_mut`, `query_2_mut`, `query_2_mut_mut` are inconsistent with it.

### Evidence

```rust
// world.rs:394-397 — defuse precedes the fallible construction
let scope = lock_tracker::TrackedRead::new(type_id, std::any::type_name::<T>());
let guard = lock.read().unwrap_or_else(|_| storage_lock_poisoned::<T>());
scope.defuse();
Some(QueryRead::new(guard, type_id))   // <- .expect() inside
```

vs. the correct shape in `World::get`:

```rust
match ComponentRef::new(guard, entity, type_id) {
    Some(cr) => { scope.defuse(); Some(cr) }
    None => None,   // scope drops → untrack
}
```

### Impact

None in practice — the downcast can only fail if `World.storages` maps a
`TypeId` to a storage that is not `T::Storage`, which is impossible by
construction (`storage_write`/`register` create `T::Storage::default()`
under `TypeId::of::<T>()`, and two distinct types cannot share a `TypeId`).
This is a defense-in-depth / consistency gap on an unreachable path, not a
live bug. The poison path *is* handled correctly (the panic happens inside
`unwrap_or_else`, before `defuse`).

### Related

#137 (`TrackedRead`/`TrackedWrite` RAII scopes), `lock_tracker::is_clean()`
test helper.

### Suggested Fix

Move the downcast + `expect` out of `QueryRead::new` / `QueryWrite::new` into
a fallible `try_new` and defuse only on success, or simply reorder so
`defuse()` runs after the wrapper is constructed
(`let q = QueryRead::new(guard, type_id); scope.defuse(); Some(q)`), matching
`World::get`.

## Completeness Checks
- [ ] **LOCK_ORDER**: Confirm the reordering doesn't change tracker-scope acquisition order relative to the ABBA graph (it shouldn't — same scope, just later defuse)
- [ ] **SIBLING**: Apply the same fix consistently across `query`, `query_mut`, `query_2_mut`, `query_2_mut_mut` (all four sites listed) — not just one
- [ ] **TESTS**: A regression test (or `lock_tracker::is_clean()` assertion) pins that the tracker scope is still armed at the point of the fallible downcast


---
## Issue 2151 [OPEN] CHAIN-D2-04: Single shared depth image is now also layout-transitioned by the FSR pass late in the frame
labels: bug low vulkan 

## Severity
LOW

## Dimension
Compute → AS → Fragment Chains — `/audit-concurrency` 2026-07-25

## Location
`crates/renderer/src/vulkan/context/mod.rs:1168` (`depth_image`, single not per-FIF); `crates/renderer/src/vulkan/frame_upscaler.rs:633-646`

## Description
`depth_image` is a single image shared by all frame-in-flight framebuffers, unlike every color attachment (explicitly per-FIF to remove cross-frame hazards). Historically the only late-frame readers were SSAO/SVGF (same-layout `SHADER_READ`); FSR now additionally performs two **layout transitions** on it per frame. With `MAX_FRAMES_IN_FLIGHT = 2`, the frame-entry fence wait is on `in_flight[frame]` (frame N-1), not frame N, so frame N+1's render pass could begin writing depth while frame N's FSR transition is still executing.

## Evidence
`draw.rs:735-738` documents the per-FIF color design explicitly; depth is the one attachment that doesn't follow it.

## Impact
A cross-frame WAW/WAR on depth would surface as flickering depth-dependent effects (SSAO shimmer, FSR disocclusion artefacts), not a crash — likely benign given in-order queue execution on current drivers, but unconfirmed.

## Trigger Conditions
Frame overlap — any frame where the GPU hasn't finished frame N by the time frame N+1's render pass starts. Normal at high frame rates.

## Verification Path
`BYRO_VALIDATION=1` with sync validation, FSR mode, 300+ frames of camera motion. Confirming signal: `SYNC-HAZARD-WRITE-AFTER-READ`/`-WRITE` naming the depth image at render-pass begin. A clean 300-frame run is meaningful evidence of non-issue.

## Related
#1583 (closed), commit `d822a783`.

## Suggested Fix
If validation fires, make depth per-FIF like every other attachment (`Vec<vk::Image>` indexed by frame). Do not add speculative barriers first.

## Completeness Checks
- [ ] **TESTS**: N/A pending validation-layer confirmation

---
