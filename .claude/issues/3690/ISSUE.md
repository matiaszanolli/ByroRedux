# #3690 — PERF-D7-2026-08-30-05: `unload_cells` recomputes the whole-world cinematic-retention set once per victim cell, then hash-probes every victim against it even when it is empty

**Severity**: LOW · **Dimension**: Streaming & Cells
**Location**: `byroredux/src/cell_loader/unload.rs`

## Fix

`cinematic_retained_entities` is a whole-world property (queries
`HorseTetherState`, `ActorCinematicState`, walks `Children` hierarchies) —
its inputs can't change across the roots of one unload batch, but
`unload_cell_inner` recomputed it and re-ran its `CellRoot`-removal loop
once per victim cell, inside `unload_cells`'s per-root loop.

Per the issue's own suggested fix, hoisted both halves out to run once per
batch:

- Extracted the `CellRoot`-removal half into its own function,
  `release_cinematic_retention(world, retained: &HashSet<EntityId>)`,
  early-returning (no `CellRoot` query at all) when `retained` is empty —
  the universal case outside an active cinematic.
- `unload_cell_inner` now takes `retained: &HashSet<EntityId>` as a
  parameter instead of computing it itself, and only runs
  `victims.retain(...)` when the set is non-empty (skipping the per-victim
  SipHash probe entirely in the common case).
- `unload_cells` computes `cinematic_retained_entities` and calls
  `release_cinematic_retention` once, before its per-root loop, and passes
  the shared `&retained` into every `unload_cell_inner` call. Added an
  early return for `cell_roots.is_empty()` so an empty batch does neither
  the scan nor the (already-unconditional) `finish_unload_batch` call.
- `unload_cell` (the single-root convenience wrapper) does the same
  once — it was already only ever going to do this once, so its behavior
  is unchanged, just restructured to share the new helper.
- The one-time scan + release is still charged to
  `UnloadPhaseTimings::ownership_index`, per the issue's own note that
  this is how the fix is directly verifiable against
  `StreamingTelemetry::unload_ownership_index` — it now happens once per
  batch rather than N times.

## SIBLING (issue's own checklist item)

`unload_cell_inner` is only called from `unload_cell` and `unload_cells`,
both in this file — no other callers exist to update.
`cinematic_retained_entities` had one other reference, an existing unit
test (`cinematic_retention_tests::active_tether_retains_horse_cart_rider_
and_hierarchy`), unaffected by this refactor since it calls the function
directly.

## TESTS (issue's own checklist item)

Two behavioral tests on the newly-extracted `release_cinematic_retention`
(no `VulkanContext` needed — pure `World` + `HashSet` logic):
- `release_removes_cell_root_only_from_retained_entities`
- `release_is_a_no_op_for_an_empty_retained_set`

`unload_cell`/`unload_cells`/`unload_cell_inner` need a live
`VulkanContext` (no fixture exists in this crate) — matching this
session's established convention for that situation (e.g. #3675's
`batches_scratch_reserve_tests`), added a static source-scan test module,
`retention_hoisting_tests`, pinning:
- `unload_cell_inner_does_not_recompute_the_retention_set` — the function
  body contains no `cinematic_retained_entities(` call and does take
  `retained: &HashSet<EntityId>` as a parameter.
- `unload_cells_computes_the_retention_set_once_before_the_per_root_loop`
  — exactly one `cinematic_retained_entities(` call in the function body,
  and it appears before the `for &cell_root in cell_roots` loop.

Scoped the search to everything before the file's first `#[cfg(test)]`
module (`production_source()`), per the self-matching trap #3674/#3675
surfaced earlier this session — this file's own tests reference both
`cinematic_retained_entities(` and `unload_cell_inner(` by name, so an
unscoped `include_str!` search would self-match regardless of the
production code's state.

**Reintroduce-and-revert verification**, two separate probes:
1. Added a redundant `cinematic_retained_entities(world)` call inside
   `unload_cell_inner`'s body — confirmed
   `unload_cell_inner_does_not_recompute_the_retention_set` failed with
   the expected message, while the other new test still passed.
2. Restored, then added a redundant `cinematic_retained_entities(world)`
   call inside `unload_cells`'s per-root loop (simulating the old
   once-per-cell recompute) — confirmed
   `unload_cells_computes_the_retention_set_once_before_the_per_root_loop`
   failed (`left: 2, right: 1`), while the other new test still passed.

Restored the clean fix after each probe and reran — all 7 tests in
`cell_loader::unload::` pass again.

## Verification

- `cargo check -p byroredux --tests`: clean.
- `cargo test -p byroredux cell_loader::unload::`: 7 tests passing, 0
  failing (+4 new).
- `cargo test -q -p byroredux`: 1878 passing (renderer-independent crate
  suite; +6 counting the two behavioral + two source-scan + two prior
  siblings in this module's own scope), 0 failing.
- `cargo test -q --no-fail-fast` (full workspace): **7106 passing, 0
  failing**.
