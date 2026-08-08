# SF-D5-2026-08-07-01: audit-infrastructure docs still reference IsCollisionOnly, removed 8 weeks ago

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2638
**Finding ID**: SF-D5-2026-08-07-01

**Severity**: LOW
**Dimension**: 5 (ESM + Cell Bring-up Regression Surface)
**Location**: `.claude/commands/audit-starfield/SKILL.md:202-203`, `.claude/commands/_audit-common.md:86`
**Status**: NEW (elevates an informal note from the 2026-08-03 report that survived a second pass unfixed)

## Description
Two audit-infrastructure docs still tell auditors to verify a component
removed 8 weeks of sessions ago. `IsCollisionOnly` was removed as dead code
by `e5868bac` (#1570, CLOSED 2026-06-15) — zero hits in any tracked `.rs`
file. The real BLAS-exclusion mechanism (still correct) is structural:
`spawn_trimesh_collider_ghost`/`spawn_packed_havok_proxy` both spawn
colliders with deliberately no `MeshHandle`, so they can never enter
`blas_specs` regardless of any marker component. `IsCollisionOnly` is
PascalCase, so `_audit-validate.sh`'s advisory-symbol heuristic (which only
scans snake_case tokens ≥7 chars) doesn't flag it — a second, smaller gap.

## Evidence
`.claude/commands/audit-starfield/SKILL.md:202-203` and
`.claude/commands/_audit-common.md:86` both still reference
`IsCollisionOnly`, which has zero hits anywhere in the tracked `.rs` tree.

## Impact
None on shipped behavior; purely repeat-work for future audit passes
re-deriving "this component doesn't exist" from scratch.

## Suggested Fix
Replace the `IsCollisionOnly` reference in both docs with the actual
mechanism (`spawn_trimesh_collider_ghost`/`spawn_packed_havok_proxy` are
`MeshHandle`-free by construction); drop it from `_audit-common.md`'s
Project Layout line. Optionally widen the validate-script's advisory regex
to also catch PascalCase identifiers.

## Related
#1570 (CLOSED), #2355 (the functional fix, landed same-day as this audit,
unrelated to this doc issue).

## Completeness Checks
- [ ] **TESTS**: N/A — doc-only change
