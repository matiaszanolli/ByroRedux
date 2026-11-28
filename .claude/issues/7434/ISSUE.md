# #7434: DLOD gate: object_resident counts empty placeholder quads, so an undrawn object family can look resident

- State at fetch: OPEN
- Labels: bug, low, terrain-exterior, test-gap

Related: #6637 (M35 Phase 1), #7431. PRD `docs/prd/distant-lod-completion.md`, DLOD-GATE-006. Found by reading the code at `5e313e4e5` on 2026-11-27. **Not observed in a run.**

## Problem

`object_resident` on the `lod-coverage:` line counts empty placeholder quads as resident.

- `update_lod_coverage` (`byroredux/src/streaming_helpers.rs`) builds `object_keys` from every key in `state.object_lod_blocks` and sets `coverage.object_resident = object_keys.len()`.
- The object ring (`cell_loader/object_lod.rs`) puts `ObjectLodBlock::empty` into that same map whenever an admitted quad produces nothing: no readable file, a parse failure, nothing to import, or no entity left to spawn. A pool refusal leaves no placeholder.
- The other families leave placeholders out of their counts: `tree_resident` (documented on the field, and filtered by both tree rings) and `placement_resident`.

So `object_resident` can be nonzero with no object quad drawn. DLOD-GATE-006's absent-family check in `m-exteriors.sh` fails a profile only when the count reads 0, and PRD §3 reports a Drawn family by a nonzero resident count.

The same unfiltered keys also feed the object overlap checks (`find_overlaps`, and the two full-detail overlap counts). The moat count uses them too. There, treating a resolved quad with nothing to draw as covered matches the tree rings, and should stay.

How often placeholders occur was not measured. The "quads loaded / tracked" log line counts loads for one reconcile pass only, so it cannot show this.

## Acceptance Criteria

- [ ] DLOD-GATE-006: `object_resident` counts only quads that spawned at least one entity
- [ ] A test shows that an object ring holding only placeholders reads `object_resident=0`
- [ ] The object overlap counts ignore placeholders, and `object_moat_cells` still treats a resolved placeholder as covered
- [ ] `m-exteriors.sh all boundary` is re-run, and any change in `object_resident` figures is recorded

