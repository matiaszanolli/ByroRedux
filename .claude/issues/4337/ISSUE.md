# #4337 — TD3-001: `GpuTerrainTile` grew 96 → 144 B (#4057), but two engine docs still say 96 B and name a test that no longer exists

**Labels**: medium, renderer, terrain-exterior, tech-debt, documentation, doc-rot
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4337

- **Severity**: MEDIUM (GPU-struct size drift) · **Dimension**: 3 · **Kind**: doc-rot · **Effort**: trivial
- **Location**: `docs/engine/exal-groundcover.md:769-771`, `docs/engine/memory-budget.md:96`
- **Status**: NEW · **Age**: `b01ef9260` (2026-09-06)
- **Finding**: `exal-groundcover.md` says "96 bytes of `uint[8] × 3`, pinned by *gpu_terrain_tile_is_96_bytes* and by `ArrayStride 96`", and `memory-budget.md` gives 96 B / ~96 KB. The live pin is `gpu_terrain_tile_is_144_bytes` (`crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:327`). `exal-groundcover.md:1129` contradicts its own §11.1.
- **Suggested Fix**: Say "was 96 B; now 144 B, pinned by `gpu_terrain_tile_is_144_bytes`", update the budget row to ~144 KB, and fold `GpuTerrainTile` into the size-claim scanner proposed under Medium Investment 3.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: If practical, a hygiene/gate check prevents this doc-rot class recurring
