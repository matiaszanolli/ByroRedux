# #4961: REN-D5-2026-09-27-02: memory-budget.md's VRAM rough-budget row still caps the rebuild transient at "+2× projected, ≤ ~512 MB"; the "Vertex / index pools" peak says 1.66 GB against a 480 MB cap

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4961
- **Labels**: low,renderer,memory,documentation,doc-rot

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D5-2026-09-27-02**._

- **Severity**: LOW
- **Dimension**: Memory/Lifecycle
- **Location**: `docs/engine/memory-budget.md`, the "VRAM Rough Budget" table rows "Global geometry SSBO rebuild (#3298)" and "Vertex / index pools"
- **Status**: NEW (the first half comes from `5226d73e2`; the second is older drift in the same table)
- **Description**:
  - `5226d73e2` rewrote the `### Global geometry SSBO rebuild` section. It now says that with a budget reading "the doubling can reach 2× the `VERTEX_POOL_HARD_CAP` + `INDEX_POOL_HARD_CAP` figures". The summary table was not updated.
  - With `VK_EXT_memory_budget` (every desktop driver) the extra resident generation is up to one projected copy. That is at most 416 MB + 64 MB = 480 MB, so the two generations together reach about 960 MB. It is not "+2× projected, ≤ ~512 MB".
  - The same table gives the pools' peak as "~1.66 GB cap". The Mesh Registry rows put the caps at ~416 MB (4 M × 104 B) and ~64 MB (16 M × 4 B).
- **Impact**: Anyone doing 6 GB-target budget arithmetic from the summary table understates the transient by about 450 MB and overstates the steady pool by about 1.2 GB.
- **Suggested Fix**: Change the rebuild row to "+1× projected (the duplicate) while under 80% of the live budget; ≤ 256 MiB without a reading". Change the pools peak to ~480 MB, and re-derive the Peak total.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
