# PHYS-02: LimitedHinge's authored perp-axis zero-reference is parsed then discarded — every elbow/knee joint's angle limits apply around a synthesized reference frame

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2448
**Finding ID**: PHYS-02 (source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 4 — PHYSAL (extract→translate boundary)
**Location**: `crates/nif/src/blocks/collision/constraints.rs:150-218` (decoded, byte-pinned); `crates/nif/src/import/collision/ragdoll.rs:324-333` (`limited_hinge_joint` — perp fields never read); `crates/nif/src/import/types.rs:1026-1033` (no perp field on `ImportedJointKind::LimitedHinge`); `crates/physics/src/ragdoll.rs:381-386` (`build_joint` synthesizes an arbitrary perp via `any_perp(axis)`)
**Status**: NEW

## Description
`bhkLimitedHingeConstraintCInfo` authors "perp axis" vectors defining the hinge's zero-angle reference frame — the plane `min_angle`/`max_angle` are measured from. `LimitedHingeCInfo` decodes and byte-pins all four vectors, but the extract→canonical step reads only `axis_a`/`pivot_a`/`axis_b`/`pivot_b`; the perp vectors are read into the struct and never touched again — `ImportedJointKind::LimitedHinge` has no field for them. At the solver boundary, `build_joint` explicitly synthesizes an arbitrary orthogonal frame instead (`any_perp(a1)`/`any_perp(a2)`), with a comment acknowledging "only the limit's zero-reference is offset."

## Evidence
Confirmed directly: `ImportedJointKind::LimitedHinge` (`crates/nif/src/import/types.rs:1026-1033`) has fields `axis_a, pivot_a, axis_b, pivot_b, min_angle, max_angle` — no perp fields. `limited_hinge_joint` (`ragdoll.rs:324`) constructs it without referencing `perp_axis_in_a1/a2/b1/b2`. `build_joint`'s LimitedHinge arm calls `frame_rot(a1, any_perp(a1))`/`frame_rot(a2, any_perp(a2))`.

## Impact
The real-data reference test confirms FNV elbows/knees decode as `LimitedHinge`, and Oblivion/Skyrim baselines list 7/8 `bhkLimitedHingeConstraint` blocks per skeleton — this is every elbow and knee joint on every converged game (Oblivion/FO3/FNV/Skyrim), not an edge case. The enforced angle window is applied relative to an arbitrary synthesized zero-reference rather than the authored one, so the actual swing range is rotated by an uncontrolled per-joint amount from what the content author intended — visible implausible bending (locking straight, bending backward, clamping short) once a ragdoll activates. Not listed in physal.md §3/§5's documented-approximation list — currently an unacknowledged fidelity loss, distinct from the already-documented Ragdoll-type `plane_min`/`plane_max` simplification (#1982).

## Related
#1982 (CLOSED, FNV-D7-03 — the analogous, already-documented Ragdoll-type simplification this is the LimitedHinge sibling of).

## Suggested Fix
Thread `perp_axis_in_a1`/`b1` through `ImportedJointKind::LimitedHinge` and use as the authored secondary axis in `frame_rot` at the solver boundary, matching how the Ragdoll type already threads `plane_a`/`plane_b`. At minimum, add the same explicit doc-comment acknowledgment physal.md gives the Ragdoll-type approximation.

## Completeness Checks
- [ ] **TESTS**: A regression test threads a known perp-axis triple through extraction and confirms it reaches `build_joint`'s frame construction
- [ ] **CANONICAL-BOUNDARY**: If threaded, the field flows through the single extract→translate boundary, no second producer
