# TD2-2026-09-22-01: hierarchy-walk guard scaffolding implemented three different ways in the Fix #4572 batch

**Labels**: low, tech-debt, bug

Filed via /audit-publish from docs/audits/AUDIT_TECH_DEBT_2026-09-22.md.

**Severity**: LOW · **Dimension**: 2 — Logic Duplication
**Location**: `byroredux/src/anim_convert.rs:37-44`, `byroredux/src/cell_loader/water.rs:736-751`, `byroredux/src/ragdoll.rs:548-644` (two walks) — all from `c9843d5d1` (Fix #4572)

**Status**: NEW
**Verified against**: HEAD `c3f298a24` — re-ran the grep live; `HierarchyTraversalGuard` is constructed only in `water.rs`, the other two match only comment text. No commit in `ee6d3fb39..c3f298a24` touched these files.

## Description

`crates/core/src/ecs/hierarchy.rs`'s `HierarchyTraversalGuard` doc comment states the intended pattern: guard (step budget) plus a visited set, paired — the pattern `byroredux/src/systems/bounds.rs`'s three pre-existing guarded walks (#3700) already follow. Fix #4572, applying the same safety idiom to four more walks in one commit, paired guard+set correctly in only one of the four (`cell_loader/water.rs`); the other three (`anim_convert.rs::build_subtree_name_map`, `ragdoll.rs::ragdoll_writeback_system`'s two BFS passes) added only a bare `HashSet` and a comment citing "the HierarchyTraversalGuard rule" without importing or using the struct.

## Impact

Not a correctness bug — a visited `HashSet` alone already bounds each walk to one visit per live entity, so none of the three can spin on a cycle; #4572's actual fix is sound. This is scaffolding inconsistency: three different hand-rolled shapes of the same idiom landed together, leaving three inconsistent precedents for the next hierarchy walk to copy.

## Related

#4572 (introduced the inconsistency while landing a correct fix), #3700 (the original three guarded walks).

## Suggested Fix

Apply `HierarchyTraversalGuard` consistently at all four sites, or add a small `core::ecs` helper bundling the `HashSet` + guard pair so all seven now-guarded call sites construct it once instead of hand-assembling it per site. Effort: trivial-small.

Source: docs/audits/AUDIT_TECH_DEBT_2026-09-22.md (TD2-2026-09-22-01)
