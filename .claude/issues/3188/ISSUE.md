# NIFAL-D9-2026-08-20-01: nifal.md documents a translate_material signature narrowed a month ago, and has no record of the mesh-water category that crossed the layer this delta

Issue: https://github.com/matiaszanolli/ByroRedux/issues/3188
Finding: NIFAL-D9-2026-08-20-01
Labels: low,nif-parser,documentation
Source: docs/audits/AUDIT_NIFAL_2026-08-20.md

Filed from `docs/audits/AUDIT_NIFAL_2026-08-20.md` (Dimension 9 — Completeness / cross-cutting). NIFAL canonical-translation finding — see `/audit-nifal`.

**Severity**: LOW
**Tier violated**: documentation — the spec is what every NIFAL sweep reads before the code.
**Game Affected**: n/a

**Location**: `docs/engine/nifal.md:525` (the stale signature); the whole file (zero occurrences of "water").

## Not a duplicate of #3075 — different drifts, disjoint fix scope

#3075 (OPEN) covers the collision shape count (`:317`, `:605`) and the understated material boundary set at `:66-99`. **Neither** of the two drifts below falls inside that issue's fix scope. Fold this into the #3075 fix pass if convenient, but it needs its own checklist.

## Description — two new drifts

### 1. Stale boundary signature

`nifal.md:525` states the boundary is:

```
translate_material(mesh, paths, extra_material_flags) -> Material
```

That `mesh` parameter was removed on 2026-07-27 by `05d68926`, which narrowed the signature to `(source: &ImportedMaterial, mesh_name: Option<&str>, paths: ResolvedPaths, extra_material_flags: u32)`.

That narrowing is **the single most important checkable invariant in Dimension 1**, because it is what makes "material translation cannot depend on geometry" *provable* rather than merely intended. `.claude/commands/audit-nifal/SKILL.md` records the narrowing and names a widening back to `&ImportedMesh` an explicit regression — while the spec still prints the pre-narrowing form. **The spec documents the regression state as current.**

### 2. Mesh water is absent entirely

A case-insensitive grep for "water" over `docs/engine/nifal.md` returns **nothing**. A whole category began crossing the layer in this delta — `ImportedMaterial.is_water_shader` -> `WaterPlane` + `WaterMaterial` + `WaterFlow` + `WaterVolume`, with four new helpers in the layer's own module and two consuming spawn sites.

It is documented only in `docs/engine/watal.md:343-368`, from the *water subsystem's* point of view. The layer spec's §2 leak inventory and §3 boundary list have no entry — so the three mesh-water findings filed from this same sweep (a duplicated composition, a divergent kind-derived value, four uncited constants) **had no spec to be checked against when they landed**.

## Evidence

```
$ sed -n '525p' docs/engine/nifal.md
- **The boundary**: `byroredux/src/material_translate.rs::translate_material(mesh,

$ grep -n "fn translate_material" -A 5 byroredux/src/material_translate.rs
268:pub(crate) fn translate_material(
269:    source: &ImportedMaterial,
270:    mesh_name: Option<&str>,
271:    paths: ResolvedPaths,
272:    extra_material_flags: u32,
273:) -> Material {

$ grep -ci water docs/engine/nifal.md
0
```

## Impact

Same failure mode as #3075 and its four predecessors: an auditor who trusts the spec (as `_audit-common.md` instructs) is told the boundary still takes an `ImportedMesh`, and is given no reason to look at mesh water at all.

This file now has a **standing drift problem across six consecutive sweeps**.

## Suggested Fix

- Correct the signature at `:525` — and the `De-duplication` bullet below it, which still names `byroredux/src/cell_loader/spawn.rs` rather than the current `byroredux/src/cell_loader/spawn/mesh_instance.rs`.
- Add a "Mesh water" subsection to §2 naming the four helpers in `byroredux/src/material_translate.rs`, the two consuming spawn sites, and the parked `damage_per_second`.
- Consider stating derived counts as pointers to the live dispatch/boundary functions rather than numbers, so they cannot drift again (the same remedy #3075 proposes for the shape count).

## Related

- #3075 (OPEN), #2488, #2306, #2301, #2299 — all prior `nifal.md`-drift findings. Six consecutive sweeps.
- The three mesh-water findings from this sweep, which the missing §2 entry left unguarded.

## Completeness Checks
- [ ] **SIGNATURE**: `:525` matches `byroredux/src/material_translate.rs:268-273` exactly, including the `mesh_name: Option<&str>` narrowing that makes the no-geometry invariant provable
- [ ] **MESH-WATER**: §2 and §3 name the four `material_translate.rs` water helpers, both spawn sites, and the parked `damage_per_second`
- [ ] **DRIFT-PROOF**: derived counts/signatures replaced by pointers where practical
- [ ] **SIBLING**: `docs/engine/physal.md`, `exal.md`, `watal.md` checked for the same signature/category drift
- [ ] **PATH-GATE**: `.claude/commands/_audit-validate.sh` still passes
