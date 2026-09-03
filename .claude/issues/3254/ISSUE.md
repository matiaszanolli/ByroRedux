# #3254 — ECS-2026-08-24-06: cinematic unload-retention permanently orphans entities out of all cell ownership

**Severity**: MEDIUM · **Dimension**: ECS
**Location**: `byroredux/src/cell_loader/unload.rs`

## Split (established session pattern for a two-part finding)

The issue's suggested fix has two independent halves:
1. **"At minimum"** — restrict the `CellRoot` strip to `retained ∩ this
   unload's victims` so an unrelated unload elsewhere stops de-parenting
   live cinematics. Mechanical, low-risk, fully verifiable — **fixed
   here**.
2. **Make retention actually reversible** — add a `HorseTetherState`
   termination path (currently nonexistent anywhere in production code)
   and a re-adoption/despawn path for entities that already lost
   `CellRoot`. Requires researching *when* a vanilla tether should end —
   a game-logic design question this project's no-guessing policy says
   needs a source, not an assumption — **split out to #3817**.

Re-verified the issue's premise against current code before implementing:
all of it still holds post-#3690 (the perf fix earlier this session hoisted
the whole-world retention *scan* out of the per-cell loop, but did not
touch the world-wide *strip* this issue is about — confirmed by reading
the current `release_cinematic_retention` before touching anything).

## Fix

Renamed `release_cinematic_retention` to `strip_retained_cell_root` and
gave it a `victims: &[EntityId]` parameter: it now strips `CellRoot` only
from the intersection of `victims` (this specific cell's own drained
victim list) and `retained` (the world-wide cinematic set), instead of
every retained entity in the world on every unload batch anywhere.

This required moving the strip back from the once-per-batch call site
(`unload_cell`/`unload_cells`, where #3690 had hoisted it for the
*scan*'s sake) into `unload_cell_inner`, which already has each cell's
own `victims` list in scope. The whole-world `cinematic_retained_
entities` scan itself stays hoisted (computed once per batch, per #3690's
own fix, unaffected by this change) — only the *application* of that set
moved back to being per-cell, scoped by that cell's own victims.

Semantically: a retained entity now keeps its `CellRoot` — and stays
fully reversible if its tether/cinematic state happens to end first,
since nothing was ever touched — right up until its *own* home cell
actually tries to reclaim it. That is a real behavior change from "any
unload anywhere triggers permanent orphaning" to "only this entity's own
cell unloading can trigger it," which is the correct, minimal trigger for
the strip to exist at all.

## SIBLING / TESTS (issue's own checklist item)

The issue's own suggested regression test, implemented as
`strip_leaves_a_retained_entity_untouched_when_it_is_not_a_victim_of_this_cell`
— a retained entity, an unrelated cell's own (empty) victim list, asserts
the retained entity's `CellRoot` is untouched. This is the exact scenario
described: "a tethered cart, an unrelated cell unload elsewhere,"
asserting the cart keeps its `CellRoot`.

Renamed and kept the two pre-existing behavior tests
(`strip_removes_cell_root_only_from_retained_victims_of_this_cell`,
`strip_is_a_no_op_for_an_empty_retained_set`), adapted to the new
`victims` parameter. The two #3690 source-scan guard tests
(`unload_cell_inner_does_not_recompute_the_retention_set`,
`unload_cells_computes_the_retention_set_once_before_the_per_root_loop`)
needed no changes and pass unmodified — this fix only touches how the
already-hoisted `retained` set is *applied*, not the hoisting itself.

**Reintroduce-and-revert verification**: temporarily restored the
world-wide strip (ignoring the `victims` parameter) — confirmed
`strip_leaves_a_retained_entity_untouched_when_it_is_not_a_victim_of_this_cell`
failed with the expected message. Restored the fix and reran — all 8
tests in `cell_loader::unload::cinematic_retention_tests` pass again.

## Follow-up

Filed **#3817** for the deferred half: a `HorseTetherState` termination
path and re-adoption/despawn for already-stripped entities, gated on
researching the vanilla cart-script's actual termination trigger before
implementing.

## Verification

- `cargo check -p byroredux --tests`: clean, zero warnings.
- `cargo test -p byroredux cell_loader::unload::`: 8 tests passing, 0
  failing (+1 new).
- `cargo test -q -p byroredux`: passing.
- `cargo test -q --no-fail-fast` (full workspace): **7149 passing, 0
  failing**.
