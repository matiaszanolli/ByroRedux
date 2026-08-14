# PHYS-D1-04

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2877

---

Found by `/audit-physics` Dimension 1 (Shape Translation). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: LOW · **Status**: NEW
**Location**: `crates/physics/src/convert.rs:349-412`

## Trigger Conditions
None today — the production code is correct. The gap fires only on a future edit that inverts the compose order.

## Description
`flatten_to_parts` composes correctly as `parent_iso * iso_from_trs(*t, *r)` (`convert.rs:120`) — parent-then-child, matching how `BhkTransformShape` authors it and how `spawn.rs:1064` composes the placement.

The two tests that claim to pin this — `nested_compound_flattens_to_part_list` and `deeply_nested_compound_composes_transforms` — build **every level with `Quat::IDENTITY`**. Composition of pure translations commutes, so `child * parent` produces byte-identical results and both tests pass under the inverted implementation. There is no test anywhere in the crate that exercises a non-identity rotation through a nested compound.

## Evidence
`convert.rs:356-372` and `:395-407` — every `children` tuple is `(Vec3::new(...), Quat::IDENTITY, Box::new(...))`. Verified by inspection that no other test in `crates/physics` feeds a rotated compound child.

## Impact
A transposed/reversed compose is the exact failure mode this area is most exposed to — *"collider in the wrong place, visually invisible, physically fatal"* — and the existing guard cannot catch it. Code-quality / test-coverage only; no live defect.

## Suggested Fix
Add one test with a 90-degree parent rotation and an off-axis child offset, asserting the child's composed translation lands where `parent_rot * child_t + parent_t` puts it (and would land elsewhere under the reversed product).

## Related
- PHYS-D1-01 (the same flatten path also drops scale); #373 (the flattening design)
