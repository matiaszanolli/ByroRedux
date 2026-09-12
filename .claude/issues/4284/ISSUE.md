# SF-2026-09-11-D8-03: Material::resolve_pbr's own contract-doc comment calls its NaN backstop unreachable, false since #2707 for 97.9% of Starfield meshes

**Issue**: #4284 — https://github.com/matiaszanolli/ByroRedux/issues/4284
**Labels**: medium,nifal,doc-rot,game:starfield,legacy-compat,documentation

**Severity**: MEDIUM
**Dimension**: Dimension 8 — NIFAL Canonical Material Translation for Starfield
**Location**: `crates/core/src/ecs/components/material.rs (Material::resolve_pbr, NaN-sentinel backstop, contract doc comment)`
**Status**: NEW

## Description
`Material::resolve_pbr`'s own contract-doc comment describes its NaN-sentinel backstop arm as "unreachable" — a claim that has been false since #2707, which made this arm live for the dominant Starfield material-reference-stub case (measured at 97.9% of Starfield meshes in this audit's sampled corpus). The comment poses a real risk: a future cleanup reading "unreachable" as license to delete the arm would ship raw NaN into `GpuMaterial` for the majority of Starfield content.

## Evidence
Verified during this audit: the NaN backstop arm is confirmed live and exercised on 97.9% of Starfield meshes (the material-reference-stub case #2707 introduced coverage for), directly contradicting the doc comment's "unreachable" claim.

## Impact
No live rendering defect today — the arm works correctly. The risk is entirely forward-looking: the false doc comment is a landmine for a future contributor who trusts it and removes "dead" code, which would then ship NaN values into the GPU material buffer for most Starfield content.

## Related
Adjacent to #2707 (made the arm live, but did not update this comment).

## Suggested Fix
Update `resolve_pbr`'s contract-doc comment to state the arm is live and reachable (citing #2707 and the 97.9% Starfield figure), replacing the stale "unreachable" claim.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_STARFIELD_2026-09-11.md — findings verified against live code during this publish run.*
