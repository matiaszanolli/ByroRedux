# FNV-D7-01: Ragdoll body + dependent constraints dropped silently on bone-name miss (no telemetry)

**Issue**: #1718
**Severity**: MEDIUM
**Labels**: medium, legacy-compat, bug
**Dimension**: 7 — PHYSAL Ragdoll
**Location**: `byroredux/src/ragdoll.rs` — `template_from_imported` (body skip ~88-90, constraint skip ~108-111)
**Source audit**: AUDIT_FNV_2026-06-23 (FNV-D7-01)

## Description
`template_from_imported` skips any `ImportedRagdoll` body whose `bone_name` is absent from the skeleton's `name→EntityId` map with no log line. Any constraint referencing a dropped body is then also silently skipped. The only downstream signal is the `< 2 bodies` / empty-constraint early-`None` returns, which collapse a partially-resolved ragdoll to "no ragdoll" with zero diagnostic about why. Distinct drop site from #1539 (which is the constraint-kind drop at NIF import in `import/collision.rs`); this is the body-name-resolution drop at spawn.

## Impact
A real FNV skeleton whose bone-naming diverges from the ragdoll's authored bone names degrades/vanishes with no breadcrumb. Reference path (Doc Mitchell, 18 bodies) unaffected; risk is the long tail of FNV skeletons. Observability gap, not a reference-content correctness break — MEDIUM.

## Related
#1539 (D7-02), #1540 (D7-03)

## Suggested Fix
Emit a rate-limited / `Once`-gated `log::warn!` listing count + names of dropped bodies and dependent constraints, mirroring the skinning path's "N unresolved bones" telemetry. Consider folding into #1539's fix.
