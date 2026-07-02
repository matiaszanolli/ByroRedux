# FNV-D7-02: bhkBreakableConstraint is invisible to the ragdoll graph — silent edge drop, no warning

**Source audit**: `docs/audits/AUDIT_FNV_2026-07-02.md` (finding FNV-D7-02)
**GitHub issue**: https://github.com/matiaszanolli/ByroRedux/issues/1850
**Labels**: medium, nif-parser, legacy-compat, bug

**Severity**: MEDIUM
**Dimension**: PHYSAL Ragdoll
**Location**: `crates/nif/src/import/collision.rs:357-360` (constraint loop downcasts only `BhkConstraint`); separate block dispatch at `crates/nif/src/blocks/collision/constraints.rs:486` / `crates/nif/src/blocks/mod.rs:28`
**Status**: NEW

## Description

`extract_ragdoll` builds the constraint graph by downcasting each block to `BhkConstraint` only. `bhkBreakableConstraint` parses into a **separate struct** (`BhkBreakableConstraint`) which the loop never downcasts. Its `wrapped_type` can be a Ragdoll (7) or LimitedHinge (2) — a genuine articulation joint — but the block is skipped entirely. Unlike the `BhkConstraintData::Other` arm (#1539, `collision.rs:390`), which `log::warn!`s before dropping, this drop is **completely silent**: no warning, no telemetry. The downstream forest-detection in `build_ragdoll` only warns after the fact about the resulting disconnected component, without naming the culprit block.

## Evidence

```rust
// crates/nif/src/import/collision.rs:357-360
for block in scene.blocks.iter() {
    let Some(c) = block.as_any().downcast_ref::<BhkConstraint>() else {
        continue;   // BhkBreakableConstraint falls through here — no warn
    };
```

`BhkBreakableConstraint` already decodes `entity_a`/`entity_b`/`wrapped_type` (`constraints.rs:503+`), so the data to surface a wrapped Ragdoll/LimitedHinge exists but is unreachable from `extract_ragdoll`.

## Impact

A breakable-wrapped ragdoll joint (rare on vanilla FNV skeletons — malleable-Ragdoll dominates per `docs/engine/physal.md`, but present on some creature/modded skeletons) silently disconnects a limb: `orient_tree` yields a forest and the detached limb builds as an independent free-floating multibody that free-falls. This is the exact failure class #1539 raised to make *loud*, but it bypasses that warning because the block type is never examined. Distinct from OPEN #1718 (bone-name miss) and from the #1539 `Other` drop.

## Related

#1718 (bone-name-miss silent drop), #1539 (`Other` constraint drop — warns)

## Suggested Fix

In the `extract_ragdoll` constraint loop, also downcast `BhkBreakableConstraint`; when `wrapped_type` is 7/2 surface the inner joint, or — as a minimal one-line safety net — emit the same `log::warn!` as the #1539 arm naming the two bones it would have linked.

## Completeness Checks
- [ ] **SIBLING**: Check whether any other constraint-type dispatch site (e.g. block-dispatch tests, `dispatch_tests/havok.rs`) assumes `BhkBreakableConstraint` is reachable from `extract_ragdoll` and would mask this gap
- [ ] **TESTS**: A regression test pins a `bhkBreakableConstraint`-wrapped Ragdoll/LimitedHinge surfacing as a joint (or at minimum, emitting the warning) instead of silently vanishing
