# FO4-D7-02: NIFAL two-phase boundary doc omits the third (BGSM-only) Phase-2 resolver, and its placement_lod exemption rationale is false for it

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4246

**Severity**: LOW
**Dimension**: 7 — NIFAL Canonical Material Translation (FO4)
**Location**: `byroredux/src/material_translate.rs:28-80`
**Status**: NEW

**Description**: The module doc's "two-phase boundary" table lists only two Phase-2 resolvers (`resolve_normal_alpha_spec_roughness`, `resolve_msn_z_source`), but a third (`resolve_unresolved_gloss_neutral_roughness`, #3905) has existed since 2026-09-05 and is precisely the FO4-relevant one (gated on `bgsm_pbr_scalars_authored`). The doc's `placement_lod` exemption rationale ("both resolvers early-return without the component, so calling them there would be a no-op") is true of the two listed resolvers but **false** of the third: its default `gloss_map_index = 0` when `MaterialTextureHandles` is absent is exactly the value that *satisfies* its predicate, so wiring it at a handle-less site per the doc's own stated rationale would silently neutralize near-mirror BGSM roughness.

**Evidence**: Confirmed in current code — `resolve_unresolved_gloss_neutral_roughness` is defined at `material_translate.rs:1074` and called from `byroredux/src/scene/nif_loader.rs:1313-1316`, but is absent from the module doc's Phase-2 table (`material_translate.rs:28-80`).

**Impact**: No runtime effect today (`placement_lod` is Oblivion-only and calls none of the three resolvers). Latent trap in exactly the place the file tells a future author it's safe to add a call.

**Related**: #3905 (added the resolver without extending this table), #3465 (wrote the exemption when only two existed), #3370 (prior correction of this same doc block).

**Suggested Fix**: Add the third resolver to the phase table, correct "Both" to the actual set, and restate the `placement_lod` exemption as "no texture handles and no BGSM merge runs there" rather than "all resolvers early-return".

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: N/A (documentation-only fix, no boundary code change)
- [ ] **TESTS**: N/A (documentation-only fix)
