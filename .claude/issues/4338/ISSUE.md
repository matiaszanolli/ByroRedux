# #4338 — TD7-002: `GROUNDCOVER_MAX_CHUNKS` cut to 256 with only a prose headroom argument; the host truncates at the cap silently and in origin order

**Labels**: medium, renderer, terrain-exterior, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4338

- **Severity**: MEDIUM · **Dimension**: 7 · **Kind**: tech-debt · **Effort**: small
- **Location**: `crates/renderer/src/shader_constants_data.rs:290-296`, `byroredux/src/render/groundcover.rs:83` (origin sort), `:94` (silent `return`), `crates/renderer/src/vulkan/groundcover.rs:1006` (second clamp), `:23-31` (module doc)
- **Status**: NEW · **Age**: `673b21458` (2026-09-13, 1024 → 256)
- **Finding**: The cap is justified only in prose ("~67 in reach … ~4× headroom"), and the module doc itself says it "is no longer enough for §11.2's chunk-size sweep". Nothing ties it to `GROUNDCOVER_DRAW_DISTANCE` / `GROUNDCOVER_CHUNK_UNITS`. The gather sorts by `origin_xz` and returns at the cap, with no stat (`GroundCoverStats` has no truncated counter). The disturber list in the same file *is* sorted nearest-first "because the renderer truncates".
- **Impact**: Halving chunk size (the documented sweep) or raising draw distance past ~4 000 units drops every chunk past the 256th in west-to-east order. Grass vanishes on one side of the camera with nothing logged. Memory stays in bounds, which is why this is not HIGH.
- **Suggested Fix**: Add a test that the ceiling of `π(draw + bound)² / chunk²` is ≤ `GROUNDCOVER_MAX_CHUNKS`, add a `chunks_truncated` stat, and truncate nearest-first.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
