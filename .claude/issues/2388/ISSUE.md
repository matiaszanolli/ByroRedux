# #2388 — ECS-D1-06: Six new inverted lock-order pairs among exclusive systems and the debug evaluator, including two inside one file

- **Severity**: LOW
- **Domain**: ecs, sync
- **Audit**: `docs/audits/AUDIT_ECS_2026-08-07.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2388


- **Severity**: LOW
- **Dimension**: 1 — Lock Ordering & Deadlock (latent ordering divergence)
- **Location**: `byroredux/src/ragdoll.rs:432-448`; `byroredux/src/systems/bounds.rs:68-71`, `:102`; `crates/debug-server/src/evaluator.rs:254-286`, `:329-338`; `byroredux/src/render/skinned.rs:80-81`
- **Status**: NEW (same finding class as #2269 / #2153, different code sites and different type pairs — neither of those names these)

**Description**

Six type pairs are acquired in opposite orders by different call sites, none of them routed through a paired helper: (1) `ragdoll_writeback_system` (Stage::Late, exclusive) takes `Transform → Parent → Children → GlobalTransform(w) → PhysicsWorld → LocalBound → WorldBound(w)`, while `make_world_bound_propagation_system` (Stage::PostUpdate, exclusive) takes `LocalBound → Parent → Children → GlobalTransform → WorldBound(w)` — inverting three pairs (`Parent↔LocalBound`, `Children↔LocalBound`, `GlobalTransform↔LocalBound`); (2) `eval_walk_entity` (`evaluator.rs`) inverts `Transform↔Parent/Children/GlobalTransform`, `Name↔Transform`, and `SkinnedMesh↔GlobalTransform` against their respective production orders; (3) sharpest instance: `eval_inspect_skinned_mesh` and `eval_walk_entity`, two functions in the same file both driven by `DebugDrainSystem`, invert `SkinnedMesh↔GlobalTransform` and `StringPool↔GlobalTransform` between each other.

**Evidence** (re-confirmed at publish time — `evaluator.rs:254` `world.query::<SkinnedMesh>()` … `:286` `world.query::<GlobalTransform>()` vs `:331` `world.query::<GlobalTransform>()` … `:336` `world.query::<SkinnedMesh>()`, same commit `79bfc76e`):

```rust
evaluator.rs:254   let Some(skin_q) = world.query::<SkinnedMesh>()
evaluator.rs:286   let gt_q = world.query::<GlobalTransform>();
// vs
evaluator.rs:331   let gt_q = world.query::<GlobalTransform>();
evaluator.rs:336   let skin_q = world.query::<SkinnedMesh>();
```

**Impact**

No live deadlock today: `ragdoll_writeback_system` and `make_world_bound_propagation_system` are both `add_exclusive`; the evaluator runs inside `DebugDrainSystem::run`, also exclusive, on the scheduler thread (client threads only enqueue commands); `build_skinned_palettes` runs after `scheduler.run` returns. So no two of these ever hold overlapping guards. The safety is entirely circumstantial: promoting any one of them to `add_to_with_access` is a one-line change with no compile-time or test-time signal. Concrete near-term consequence: in a `BYRO_LOCK_ORDER_CHECK=1` debug build, issuing `skin <id>` then `walk <id>` through `byro-dbg` aborts the engine on a spurious ABBA report — the detector cannot tell "sequential temporaries on one thread" from a real overlap, and this pair is reachable from two ordinary console commands.

**Related**: #2269 (`CinematicPresentationState↔QuestStageState`, same class), #2153, #313, #1410.

**Suggested Fix**: Pick and document one process-wide order for the hierarchy/bounds cluster (`Transform → Parent → Children → GlobalTransform → LocalBound → WorldBound` matches `transform_propagation_system`), reorder `eval_inspect_skinned_mesh`/`eval_walk_entity` to agree with each other and with `render/skinned.rs`, and add the order to `docs/engine/ecs.md`'s lock-ordering policy section.

## Completeness Checks
- [ ] **LOCK_ORDER**: Reorder all six sites to the documented process-wide order; re-verify with `BYRO_LOCK_ORDER_CHECK=1` that the `skin <id>` → `walk <id>` byro-dbg sequence no longer aborts
- [ ] **SIBLING**: Sweep other `add_exclusive` systems and debug-evaluator functions for the same pattern beyond the six named sites
- [ ] **TESTS**: A regression test (or documented manual byro-dbg repro) pinning the `skin`/`walk` sequence no longer spuriously panics under the opt-in detector

---
Filed from `docs/audits/AUDIT_ECS_2026-08-07.md` via `/audit-publish`.
