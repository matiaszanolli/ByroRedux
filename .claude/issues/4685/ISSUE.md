# PHYS-D6-2026-09-21-02: The per-frame full QBVH rebuild is avoidable — rapier's incremental path costs ~1-3% of it on large worlds

**Issue**: #4685
**Filed**: 2026-09-22 (audit-publish, AUDIT_PHYSICS_2026-09-21.md)

**Severity**: MEDIUM
**Dimension**: Queries & Diagnostics
**Location**: `crates/physics/src/world.rs:702-712` (`None` passed to `pipeline.step`), `:792-795` (post-loop full update), `:603-635` (cost attribution)

## Description
The rationale claims passing the query pipeline to `pipeline.step` triggers a full `clear_and_rebuild` up to 5× per frame. In rapier 0.22, `PhysicsPipeline::step` calls `update_incremental` instead, touching only dirty leaves. The engine pays an O(all colliders) rebuild per frame to avoid a cost that does not exist.

## Evidence
rapier3d-0.22.0 `pipeline/physics_pipeline.rs:494-497,632-640` calls `update_incremental`; `query_pipeline/mod.rs:315-339` touches only dirty leaves. Standalone release proxy (1 substep/frame, 360 kinematic movers, 1 falling dynamic, 60 frames):

| World | `step(None)` + full update | Incremental `step(Some(qp))` |
|---|---|---|
| 30 k fixed | 2.78 ms/frame | 0.089 ms/frame |
| 95 k fixed | 9.59 ms/frame | 0.103 ms/frame |
| 5 k fixed | 0.33 ms/frame | 0.087 ms/frame |

A post-run ray finds the moving body in both designs.

## Impact
On a Cydonia-sized world, about 9.6 ms of a 16.7 ms frame. Together with PHYS-D2-2026-09-21-01 (#4682), about 11 ms/frame of avoidable physics CPU on the largest cells.

## Related
#2864, #2890; PHYS-D2-2026-09-21-01 (#4682).

## Suggested Fix
Pass `Some(&mut self.query_pipeline)` to `pipeline.step`; drop the post-loop full update on stepped frames. Keep a full/tracked-incremental update only for `colliders_dirty` frames with no substep. Re-measure on real content, then update the `:603-635` comment and `step_cost_rationale_is_scoped_to_history_and_names_the_real_cost_centre`.
