# FO4-D7-01: normal_alpha_spec heuristic overwrites already-resolved BGSM roughness

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2606
**Finding ID**: FO4-D7-01

**Severity**: MEDIUM
**Dimension**: 7 (Canonical Material)
**Location**: `byroredux/src/material_translate.rs:256-372` (`normal_alpha_spec_applies` / `resolve_normal_alpha_spec_roughness`)
**Status**: NEW

## Description
`normal_alpha_spec_applies`/`resolve_normal_alpha_spec_roughness` has no
`from_bgsm`/`BGSM_AUTHORED` exclusion, so it can overwrite already-resolved
canonical FO4 BGSM roughness. This is confirmed **live**, not latent:
`ImportedMaterial::default().env_map_scale` is `0.0`
(`crates/nif/src/import/types.rs:569`), which satisfies the heuristic's
`env_map_scale <= 0.3` gate by default — meaning any BGSM material that
doesn't explicitly set a higher `env_map_scale` falls into this heuristic's
overwrite path even though it already has authoritative BGSM-sourced
roughness.

## Evidence
```rust
// byroredux/src/material_translate.rs:256-372
// normal_alpha_spec_applies / resolve_normal_alpha_spec_roughness
// — no from_bgsm / BGSM_AUTHORED check gating the overwrite
```
```rust
// crates/nif/src/import/types.rs:569
// ImportedMaterial::default().env_map_scale == 0.0
// satisfies the <= 0.3 gate by default, not just when explicitly authored low
```
Cross-referenced against existing open #2330, which documents these same
call sites but only as a documentation-precision gap — it does not cover
this finding's substantive claim (an actual overwrite of authoritative BGSM
roughness), so this is a related-but-distinct, non-duplicate finding.

## Impact
BGSM-authored FO4 materials with `env_map_scale` left at its default (i.e.
most BGSM materials that don't explicitly raise it) have their
already-correct BGSM roughness silently overwritten by this heuristic,
compounded by the fact that `MAT_FLAG_PBR_BSDF` is now unconditionally set
for every BGSM material (#1352) — the Disney PBR lobe is live and sensitive
to roughness, so this directly affects rendered appearance.

## Suggested Fix
Add a `from_bgsm`/`BGSM_AUTHORED` exclusion to
`normal_alpha_spec_applies`/`resolve_normal_alpha_spec_roughness` so it never
overwrites roughness that already came from an authoritative BGSM source.

## Related
Existing #2330 (documentation-only, does not cover this substantive
overwrite bug); FO4-D7-02, FO4-D7-03 (same BGSM-merge-boundary drop class).

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Fix belongs at the `byroredux/src/material_translate.rs` (`translate_material`) boundary — gate on `BGSM_AUTHORED`, don't special-case per-game elsewhere
- [ ] **TESTS**: A regression test with a BGSM material at default `env_map_scale` pins that roughness survives translation unchanged
