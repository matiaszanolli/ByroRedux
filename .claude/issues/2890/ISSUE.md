# PHYS-D7-06

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2890

---

Found by `/audit-physics` Dimension 7 (Queries & Diagnostics). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: LOW · **Status**: NEW
**Location**: `crates/physics/src/world.rs:352-368` (fast-path rationale), `:398-406`, `:452-457`

## Trigger Conditions
Documentation-only; fires whenever someone reasons about whether the fast path still earns its complexity, or budgets physics cost from these numbers.

## Description
The comment that justifies the static-scene fast path's existence reads, **in the present tense**:

> *"A `pipeline.step()` pays full broad-phase + **query-pipeline-rebuild** cost over every collider regardless of motion — on a radius-12 exterior that's ~8-10 ms/step x up to 5 substeps, ~40 ms/frame."*

Forty lines below, the same function explicitly passes `None` for the query pipeline with its own comment explaining that the in-substep rebuild was removed because it *"was the bulk of the per-step cost"*. So the stated per-step figure **includes a cost the step no longer pays**, and is quoted as current.

`git log` confirms both the figure and its invalidation landed in the *same* commit, `6e55b492` ("perf(physics): sleep the simulation step for static scenes"). Its message gives the measured before/after: *"physics step ~45 ms -> ~0.02 ms once settled"*, with three compounding causes — `length_unit` left at 1.0 (so nothing ever slept), no fast path, and the per-substep query-pipeline rebuild. The `~8-10 ms/step` number describes the world **before all three fixes**.

**Secondary**: `:400-402` and `:452-457` both call `QueryPipeline::update` a *"BVH refit"*. `rapier3d-0.22.0/src/pipeline/query_pipeline/mod.rs:348-358` implements `update` as `self.qbvh.clear_and_rebuild(...)` — a full **rebuild**, not a refit. The distinction is material: "refit" implies an incremental cost that is not there.

## Evidence
A real radius-12 exterior could **not** be measured (no engine launch permitted; needs game data; no `cargo bench` target exists for the physics crate). A **synthetic in-crate proxy** was run instead — 30 000 `Fixed` cuboid colliders + 1 awake dynamic body, release build, `substep_time_budget` disabled, 20 iterations after warmup:

```
DIM7 MEASURE: colliders=30000
  PhysicsWorld::step (1 substep + the post-loop QP update) = 2.820 ms
  bare QueryPipeline::update(&colliders)                   = 2.219 ms
  => pipeline.step() itself ~ 0.6 ms
```

(Test added, run, then reverted — working tree clean.) Caveat stated plainly: cuboids are cheaper than the real mix, which is heavy in TriMesh colliders whose QBVH rebuild is materially more expensive, so real-cell numbers will be higher. What the proxy *does* establish directionally is that the once-per-frame `QueryPipeline::update` **dominates `pipeline.step()` roughly 4:1** — the inverse of what the comment attributes the cost to.

## Impact
Anyone re-deriving the physics budget, or asking whether the fast path (with its `pending_wake` protocol and the PHYS-D2-01 / PHYS-D2-02 correctness bugs it has already caused) is still worth its complexity, will do so from a number that is three fixes out of date and attributed to the wrong mechanism.

## Suggested Fix
Rewrite the rationale as history (*"before `6e55b492` a radius-12 FNV exterior spent ~45 ms/frame in `step` because ..."*), state the current cost centre (one `QueryPipeline::update` per stepping frame, O(all colliders), full rebuild), and replace "refit" with "rebuild" at both sites.

## Related
- **PHYS-D2-03** (the code half of the same cost story — double `clear_and_rebuild` on streaming frames; fix alongside, do not merge)
- PHYS-D2-01, PHYS-D2-02 (fast-path correctness); commit `6e55b492`
