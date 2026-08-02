# REN-D19-02: Starfield's packed bitangent sign isn't normalized to +/-1 like every other game's

Severity: medium
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2246

**Dimension**: 19 (Tangent space)
**Location**: `crates/nif/src/import/mesh/bs_geometry.rs:166` (bitangent-sign channel from `unpack_udec3_xyzw`)
**Status**: NEW

**Description**: Starfield's `BSGeometry` packs the bitangent sign via `unpack_udec3_xyzw`, but unlike every other game's import path, the resulting sign value isn't normalized/clamped to exactly `+1.0`/`-1.0` before being written to the vertex tangent's `w` component — a latent primary/secondary-ray disagreement wherever downstream shader code assumes `vertexTangent.w` is exactly `±1` (e.g. `tangentSign = vertexTangent.w < 0.0 ? -1.0 : 1.0;` in `material_sampling.glsl` already defensively re-normalizes it at the point of use, but any other consumer that reads `vertexTangent.w` directly without that re-normalization would disagree).

**Impact**: Any shader path that consumes `vertexTangent.w` directly (rather than through the defensively-clamped `perturbNormal` helper) can diverge from the primary-ray tangent frame on Starfield content specifically.

**Suggested Fix**: normalize the unpacked bitangent-sign value to exactly `+1.0`/`-1.0` at import time in `bs_geometry.rs`, matching every other game's import path, so no downstream consumer needs its own defensive re-clamp.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
