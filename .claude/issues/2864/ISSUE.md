# PHYS-D2-03

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2864

---

Found by `/audit-physics` Dimension 2 (Step Determinism & Budget). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: MEDIUM · **Status**: NEW
**Location**: `crates/physics/src/sync.rs:657-659` + `crates/physics/src/world.rs:455-457`

## Trigger Conditions
Any frame that both registers >=1 newcomer and runs >=1 substep — i.e. every frame of a cell stream / cell-entry settle storm.

## Description
`register_newcomers` refreshes the query pipeline before the step (`sync.rs:657-659`), and `PhysicsWorld::step` refreshes it again after the substep loop (`world.rs:455-457`). Nothing between those two points reads the query pipeline:
- `pipeline.step` is deliberately passed `None` (`world.rs:398-406`)
- `apply_buoyancy` uses `collider.compute_aabb()`, not the pipeline (`water.rs:371-375`)
- every engine-side cast consumer runs in `Stage::Early`/`Update` (`byroredux/src/systems/character.rs`, `byroredux/src/systems/locomotion.rs:64`, `byroredux/src/interaction.rs:327`) — i.e. before `Stage::Physics`, against the previous frame's pipeline

So the pre-step refresh is redundant on exactly the frames that also step. It is *not* dead code — it is the only refresh on a frame where the fast path skips the step — but it can be deferred.

## Evidence
`QueryPipeline::update` is a **full clear-and-rebuild**, not a refit: `rapier3d-0.22.0/src/pipeline/query_pipeline/mod.rs:348-358` -> `self.qbvh.clear_and_rebuild(mode, self.dilation_factor)`. The comment at `world.rs:399-401` describing it as a *"BVH refit over the whole set"* therefore understates it, and the same comment records that this rebuild was *"the bulk of the per-step cost"* over ~30 k static colliders — the reason it was hoisted out of the substep loop in the first place.

Synthetic measurement from Dimension 7 (30 000 fixed cuboid colliders, release): `PhysicsWorld::step` = 2.82 ms, of which the single `QueryPipeline::update` = **2.22 ms** — the rebuild dominates `pipeline.step()` roughly 4:1.

rapier 0.22 also ships `QueryPipeline::update_incremental(colliders, modified, removed, refit_and_rebalance)` (`mod.rs:315-346`), which is the intended API for the newcomer case.

## Impact
Doubles the dominant per-frame physics cost on precisely the frames the `#1698` anti-spiral budget exists to protect (cell entry). Because the pre-step rebuild happens *before* `loop_start` is sampled (`world.rs:383`), it is also **invisible to the substep budget** — it inflates the frame without ever being counted against `substep_time_budget`.

## Suggested Fix
Drop the refresh in `register_newcomers` and change the post-step condition in `step()` to "steps > 0 **or** colliders were inserted this frame" (a `colliders_dirty` flag armed by registration), so exactly one rebuild happens per frame. Alternatively switch the registration path to `update_incremental` with the freshly-inserted handles. Note `byroredux/src/scene.rs:814` already calls `update_query_pipeline()` explicitly on the spawn path, so that path is unaffected either way.

## Related
- #1698 (the anti-spiral budget); the `SUBSTEP_TIME_BUDGET` doc block (`world.rs:16-39`)
- PHYS-D7-06 — the doc half of the same cost story
