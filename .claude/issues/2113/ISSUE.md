# #2113: D7-01: Pending stream requests never cancelled when their cell leaves the load ring

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2113
**Labels**: bug, import-pipeline, low, performance

---

**Severity**: low
**Dimension**: Streaming & Cells
**Location**: `byroredux/src/app_step.rs:87-161`, `byroredux/src/streaming.rs:716-758` (`compute_streaming_deltas`)
**Status**: NEW

## Description
`compute_streaming_deltas` derives `to_unload` from `state.loaded` only (`streaming.rs:717` — the function signature takes `loaded: &HashMap<(i32, i32), LoadedCell>`, not `state.pending`). A cell dispatched to the background worker but not yet spawned lives in `state.pending: HashMap<(i32, i32), u64>` (streaming.rs:211) and is invisible to this diff. If the player leaves the area before the in-flight parse finishes, the payload still classifies as `Apply` when it arrives and pays a full main-thread spawn (terrain + BLAS + upload) — then unloads again at the next boundary crossing.

## Evidence
```rust
// streaming.rs:716-721
pub fn compute_streaming_deltas(
    loaded: &HashMap<(i32, i32), LoadedCell>,
    player_grid: (i32, i32),
    radius_load: i32,
    radius_unload: i32,
) -> StreamingDeltas {
```
No `pending` parameter. `state.pending` (streaming.rs:211) is only consulted later, in the payload-arrival decision (`streaming.rs:778-817`, `PayloadDecision`), not in the unload diff.

## Impact
Bounded and self-correcting (world state stays consistent; `MAX_CELLS_SPAWNED_PER_FRAME` caps the damage) — this is a wasted-work finding, not a correctness bug. Wastes a full main-thread spawn (terrain + BLAS build + texture upload + fence) for a cell the player has already left, immediately followed by an unload of the same cell.

## Suggested Fix
In `compute_streaming_deltas` (or its caller in `app_step.rs`), drop `state.pending` entries whose coord is now `> radius_unload` from `player_grid` so the in-flight payload classifies as `PayloadDecision::StaleNoPending` (per the existing decision enum at `streaming.rs:792-798`) and is discarded before spawn, rather than after.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix (e.g. simulate a pending request whose cell exits the unload radius before arrival)

