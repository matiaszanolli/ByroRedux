# 3236: NIFAL-D9: audit-nifal SKILL.md still claims translate_material has exactly two callers

**Severity**: LOW (self-referential doc-rot in the audit skill file itself) · **Dimension**: NIFAL Completeness/cross-cutting · **Report**: `docs/audits/AUDIT_NIFAL_2026-08-23.md` (NIFAL-D9-2026-08-23-02)

## Description

`.claude/commands/audit-nifal/SKILL.md:109` reads: *"Callers (MUST be exactly two, both routing through the boundary): `byroredux/src/scene/nif_loader.rs` (loose-NIF path) and `byroredux/src/cell_loader/spawn.rs` (cell path)."*

The live production call graph has **three** callers:
```
byroredux/src/scene/nif_loader.rs:1013
byroredux/src/cell_loader/spawn/mesh_instance.rs:753
byroredux/src/cell_loader/placement_lod.rs:514
```
(A fourth apparent hit, `byroredux/src/cornell.rs:1783`, is inside a unit test, not a production call site.)

This is distinct from the already-open `NIFAL-D9-2026-08-16-01`, which flags the same 3-vs-2 count but against `docs/engine/nifal.md` — a different document that was not touched by that fix. This SKILL.md line has drifted for at least a week (since 2026-08-16) across two sweeps without being caught, because those sweeps' Dimension 9 checks cited `nifal.md`'s drift but not the skill file's own.

## Impact

A future Dimension-1 sweep that trusts this line at face value would either (a) misreport `placement_lod.rs` as a `single-boundary` violation (it is not — it correctly routes through `translate_material`, it's simply an undocumented third caller), or (b) fail to notice if a genuine fourth, non-conforming construction site appeared, because the reference count it's diffing against is already wrong. Low severity because the underlying boundary is intact and correctly used by all three callers.

## Suggested Fix

Update `SKILL.md:109` to name all three current production callers and drop "MUST be exactly two" in favor of "MUST all route through this one fn" — matching the phrasing already used correctly for the Particles/Animation dimensions in the same file ("multiple callers of one boundary is correct, not a violation").

## Completeness Checks
- [ ] **SIBLING**: Fold into or alongside #3075 (the `nifal.md` fix ticket for the same 3-vs-2 count)
