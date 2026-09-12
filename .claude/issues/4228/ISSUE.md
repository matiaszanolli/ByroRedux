# FNV-D2-01: material_translate.rs module doc undercounts the Phase-2 resolvers (missing resolve_unresolved_gloss_neutral_roughness)

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4228
**Labels**: documentation, low, legacy-compat, game:fnv, nifal, doc-rot
**Source**: `/audit-fnv` — `docs/audits/AUDIT_FNV_2026-09-11.md`, finding FNV-D2-01

**Severity**: LOW
**Dimension**: NIFAL Canonical Translation (FNV Slice) — `/audit-fnv` Dimension 2
**Location**: `byroredux/src/material_translate.rs:28-45` (module-doc Phase-2 resolver table/prose) vs the third resolver at `byroredux/src/material_translate.rs:1074-1093` (`resolve_unresolved_gloss_neutral_roughness`)

**Description**: The module doc's Phase-2 resolver table and prose ("**Both** Phase-2 resolvers…") lists only `resolve_normal_alpha_spec_roughness` and `resolve_msn_z_source`, undercounting a third Phase-2 resolver, `resolve_unresolved_gloss_neutral_roughness` (added by #3905), which also writes canonical `Material::roughness` post-handle-attach from the same two production call sites (`scene/nif_loader.rs:1313`, `cell_loader/spawn/mesh_instance.rs:1115`).

**Evidence**: Doc table at `material_translate.rs:36-45` enumerates exactly two Phase-2 resolvers. `resolve_unresolved_gloss_neutral_roughness` (defined at `:1074-1093`) is called from both of the same two production sites the doc claims are fully enumerated by "Both Phase-2 resolvers".

**Impact**: Documentation-only; no runtime divergence — all three resolvers are correctly wired at both call sites. A future auditor or contributor reading the module doc as the authoritative resolver inventory would miss this third resolver and could reintroduce the #3465 "stopped identifying the set" drift this doc was written to prevent.

**Related**: #3905 (added the resolver), #3465 (prior doc-rot fix in the same doc).

**Suggested Fix**: Add a third row to the Phase-2 resolver table naming `resolve_unresolved_gloss_neutral_roughness` and its write target, and update "Both Phase-2 resolvers…" to "All three Phase-2 resolvers…".

## Completeness Checks
- [ ] **SIBLING**: Check for the same enumeration in `docs/engine/nifal.md` and update if it also undercounts.
- [ ] **TESTS**: N/A — documentation-only fix; no regression test applicable.
