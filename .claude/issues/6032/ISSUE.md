# REN-EXT-L-03: no static bound on the 32-bit vertex span the blade tiers address

Labels: low,renderer,test-gap,bug
Source: docs/audits/AUDIT_RENDERER_2026-10-18.md

- **Severity**: LOW (latent)
- **Dimension**: GPU-Struct
- **Location**: `groundcover_scatter.comp` (`firstVertex = sliceBase * vertsPerPoint`, `vertexCount = accepted * vertsPerPoint`); `groundcover_blade.vert` (`uint(gl_VertexIndex) / vertsPerPoint`); constants in `shader_constants_data.rs`
- **Status**: NEW
- **Description**: Tier 0 spans `GROUNDCOVER_MAX_CHUNKS × GROUNDCOVER_MAX_BLADES_PER_CHUNK × (15 × 4)` = 503,316,480 vertex indices, about 11.7 % of the u32 range. Nothing asserts that, and the constants have been retuned twice (candidate budget ×4 on 2026-09-16, chunk cap ×2 on 2026-10-14). A further ×8.5 would wrap `firstVertex` and `gl_VertexIndex` silently. The sibling model-tier span has exactly this kind of assertion (`GROUNDCOVER_MODEL_POINTS_PER_CHUNK << GROUNDCOVER_MODEL_RECORD_BITS <= 1 << 32`).
- **Suggested Fix**: add a `const _` beside `GROUNDCOVER_MAX_CHUNKS` asserting `MAX_CHUNKS × MAX_BLADES_PER_CHUNK × max(verts per point over tiers) <= u32::MAX`.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_RENDERER_2026-10-18.md` (`/audit-suite --area exterior`, HEAD 6f44874ac). Fix with `/fix-issue <number>`._
