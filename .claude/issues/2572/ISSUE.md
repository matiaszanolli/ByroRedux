# OBL-D5-02: resolve_normal_alpha_spec_roughness post-mutates canonical roughness outside translate_material, with no canonical-tier test of the combined result

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2572
**Finding ID**: OBL-D5-02

**Severity**: LOW
**Dimension**: NIFAL Canonical Material Translation for Oblivion
**Location**: `byroredux/src/material_translate.rs` (called from `cell_loader/spawn.rs:1553`, `scene/nif_loader.rs:934`)
**Status**: NEW

## Description
Both load paths call `resolve_normal_alpha_spec_roughness` consistently (no divergence today), but the gate (`normal_alpha_spec_applies`) is live for a meaningful swath of non-metal Oblivion content — Oblivion stores tangent-space normals in `NiTexturingProperty`'s bump slot with the specular mask in the DDS alpha, and `env_map_scale` is 0.0 unless the SLSF1 env bit is authored — so the alpha-normal roughness formula (derived from `NiMaterialProperty.shininess`) silently overrides the classifier's default matte roughness across Oblivion architecture/clutter. Whether the resulting values are correct is unmeasured (no Oblivion BSA reachable in that audit session — a ready-made census harness exists at `crates/nif/examples/_tmp_obl_d5_nifal.rs`).

## Evidence
Confirmed directly: `resolve_normal_alpha_spec_roughness` is called from both `spawn.rs:1553` and `nif_loader.rs:934`, both post-`translate_material`.

## Impact
Two concrete test gaps: the unit tests exercise the resolver as a pure function only, and the canonical-completeness harness deliberately bypasses classifiers so it never covers Oblivion's actual roughness population. Correctness of the resulting values is unmeasured.

## Suggested Fix
Run the census harness at `crates/nif/examples/_tmp_obl_d5_nifal.rs` against real Oblivion data to measure the actual roughness population; add canonical-tier test coverage of the combined `translate_material` + post-mutation result, not just the pure-function resolver.

## Completeness Checks
- [ ] **TESTS**: Canonical-tier test covers the combined `translate_material` + `resolve_normal_alpha_spec_roughness` result for representative Oblivion content
