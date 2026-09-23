# #4788: PERF-D4-2026-09-23-01: #3834's cluster "prefix" upload is keyed on the highest touched cluster index, so any volume in the camera's +Z half uploads most of the 160 KB set

**Severity**: LOW
**Labels**: low, performance, renderer, bug
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23.md (PERF-D4-2026-09-23-01)

- **Severity**: LOW
- **Dimension**: SSBO Sizing & Upload
- **Location**: `volumetrics.rs:579-588` (`cluster_index = x + 16y + 256z`, `cluster_hi = max`), `:1228-1235` (upload `[..write_hi]` entries and `[..write_hi*8]` indices, unioned with the slot's previous hi).
- **Status**: NEW (partial effectiveness of #3834, closed). The original report proposed a min/max *range*; the fix shipped a prefix.
- **Description**: The index's major axis is the world-Z cluster layer, and each layer is 256 clusters × (8 B entry + 32 B indices) = 10 KB. One volume whose cluster range reaches layer *k* forces (k+1) × 10 KB. A fire one cell (16 m) in +Z of the camera already uploads ≥ 90 KB of the 160 KB, and a fire-rich cell (Markarth) sits near the full amount.
- **Impact**: About 90–160 KB of write-combined host writes per frame in fire-bearing cells (*est.* 20–60 µs CPU), which is the case #3834 targeted. It is CPU-only, and the GPU is unaffected.
- **Suggested Fix**: Track `cluster_lo` alongside `cluster_hi`, and upload `[min(lo, prev_lo), max(hi, prev_hi))` with the existing per-slot union. Alternatively, keep a per-Z-layer dirty mask and upload the touched layers.

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-09-23.md` (`/audit-suite --preset volumetrics-deep`, HEAD `2237da9c3`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related passes (other compute passes / history buffers / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
