# FNV-D3-01: compute_sky's single cloud_lod term ignores each WTHR cloud layer's own tile_scale

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4230
**Labels**: bug, renderer, low, legacy-compat, game:fnv, shaders
**Source**: `/audit-fnv` — `docs/audits/AUDIT_FNV_2026-09-11.md`, finding FNV-D3-01

**Severity**: LOW
**Dimension**: RT Lighting Pipeline (FNV Scenes) — `/audit-fnv` Dimension 3
**Location**: `crates/renderer/shaders/composite.frag:573` (`cloud_lod` computation), consumed at `:582,601,616,631` (cloud layers 0-3)

**Description**: `compute_sky`'s single analytic `cloud_lod` term models only the `1/elevation` UV stretch and ignores each WTHR cloud layer's own `tile_scale`, so layers 1-3 (baseline tile scales 0.20/0.25/0.30 vs layer 0's tuned 0.15) sample up to ~1 mip sharper than their own UV frequency warrants near the horizon.

**Evidence**: `float cloud_lod = log2(1.0 / max(elevation, 0.05)) * 0.5;` is computed once (`composite.frag:573`) and reused verbatim by all four cloud layers' `textureLod` calls (`:582` layer 0, `:601` layer 1, `:616` layer 2, `:631` layer 3), even though each layer applies its own distinct `tile_scale` (`params.cloud_params.z`, `cloud_params_1.z`, `_2.z`, `_3.z`) to its UV before sampling.

**Impact**: Cosmetic; reachable on FNV exteriors (`WastelandNV` WTHR authors multiple cloud layers), invisible in interiors.

**Related**: #730 (original `cloud_lod`/SH-13 rationale).

**Suggested Fix**: Fold each layer's own `tile_scale` into its own `cloud_lod` term, e.g. `log2(tile_scale_N / max(elevation, 0.05)) * 0.5` (or an equivalent per-layer offset relative to layer 0's tuned 0.15 baseline), so mip selection tracks each layer's actual UV frequency.

## Completeness Checks
- [ ] **SIBLING**: Verify the horizon-fade (`smoothstep(0.0, 0.12, elevation)`) and singularity floor (`max(elevation, 0.05)`) constants don't need matching per-layer treatment.
- [ ] **TESTS**: No golden-frame regression test currently pins sky cloud LOD; consider adding one if a fix lands.
