# SF-2026-09-11-D9-02: RefrTextureOverlay::fill_from_bgsm claims exact BGEM parity with merge_external_material but drops base_texture and envmap_mask_texture, and fills env unconditionally with no env_mapping_enabled gate

**Issue**: #4287 — https://github.com/matiaszanolli/ByroRedux/issues/4287
**Labels**: medium,nifal,game:starfield,legacy-compat,bug

**Severity**: MEDIUM
**Dimension**: Dimension 9 — BGSM/BGEM External Material Flow
**Location**: `byroredux/src/cell_loader/refr.rs:316-340 (fill_from_bgsm, .bgem arm)`
**Status**: NEW

## Description
`RefrTextureOverlay::fill_from_bgsm`'s `.bgem` arm forwards `normal`, `glow`, `env`, `external_specular`, `lighting`, and `height` (from `grayscale_texture`) — but never forwards `bgem.base_texture` into `self.diffuse`, and never forwards an envmap-mask texture into an env-mask field. It also fills `self.env` unconditionally from `bgem.envmap_texture` with no gate on `bgem`'s own `env_mapping_enabled()`-equivalent flag. This reintroduces, on this second parallel resolver, exactly the two defects #2643 already fixed on the primary `merge_external_material` path.

## Evidence
`byroredux/src/cell_loader/refr.rs`'s `.bgem` arm has fill calls for normal/glow/env/external_specular/lighting/height but no `Self::fill(&mut self.diffuse, Some(bgem.base_texture...` call exists anywhere in the arm, and the `env` fill (`Self::fill(&mut self.env, Some(bgem.envmap_texture.as_str()), pool);`) has no preceding conditional on an environment-mapping-enabled flag.

## Impact
On any REFR-level texture-swap path that resolves a `.bgem` material (XATO/XTNM/XMSP-driven material swaps), the overlay silently loses the BGEM's base/diffuse texture and any environment mask it authors, and applies an environment map even when the BGEM did not request environment mapping — a visible-content divergence between the import-time (`merge_external_material`) and REFR-override (`fill_from_bgsm`) resolution paths for what should be identical BGEM semantics.

## Related
Regression of the pattern #2643 already fixed once, on the sibling `merge_external_material` path — this is the same defect class reintroduced on the second, parallel resolver.

## Suggested Fix
Add the missing `base_texture`→`self.diffuse` and envmap-mask→env-mask forwarding calls to the `.bgem` arm, and gate the `env` fill on the BGEM's environment-mapping-enabled flag, mirroring exactly what `merge_external_material` does post-#2643.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_STARFIELD_2026-09-11.md — findings verified against live code during this publish run.*
