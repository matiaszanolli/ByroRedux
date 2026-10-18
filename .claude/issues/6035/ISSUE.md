# REN-EXT-L-06: the new shadow-filter images and the reservoir tail regions are not billed in `render_extent_bytes_per_pixel_x1024` and not ledgered in `memory-budget.md`

Labels: low,renderer,memory,bug
Source: docs/audits/AUDIT_RENDERER_2026-10-18.md

- **Severity**: LOW
- **Dimension**: Memory/Lifecycle
- **Location**: `SHADOW_FILTER_BYTES_PER_PIXEL` (`shadow_filter.rs`, defined but referenced nowhere else); `render_extent_bytes_per_pixel_x1024` and its independent twin in `acceleration/tests/predicates_tests.rs`; `ReservoirBuffers` (`restir.rs`: `CLAIM_STRIDE` 4 B and `UNSHADOWED_STRIDE` 8 B tails); the "ReSTIR Reservoirs" section of `memory-budget.md`
- **Status**: NEW (out of the exterior area; not deduped against sibling suites)
- **Description**: The reservation counts `RESERVOIR_STRIDE × MAX_FRAMES_IN_FLIGHT` (64 B/px). Since #5765 the reservoir buffer is 32 + 4 + 8 = 44 B/px per slot, and `ShadowFilterPipeline` holds 24 B/px per slot (three RGBA16F images). The doc formula still reads `RESERVOIR_STRIDE + 4`, and the page has no shadow-filter row. The independent-list guard repeats the omission, so it passes.
- **Impact**: `blas_budget_for_heap` reserves about 72 B/px too little: around 150 MB at 1080p render extent and around 600 MB at 4K native. The BLAS budget is one third of the unreserved remainder, so the budget is over by a third of that (about 50 MB at 1080p). Eviction starts slightly late on small-VRAM cards.
- **Suggested Fix**: add both terms to `render_extent_bytes_per_pixel_x1024` and to the guard's list, and ledger them in `memory-budget.md`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' arms, other callers)
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_RENDERER_2026-10-18.md` (`/audit-suite --area exterior`, HEAD 6f44874ac). Fix with `/fix-issue <number>`._
