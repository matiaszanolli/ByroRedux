# #4786: PERF-D5-2026-09-23-04: With nothing burning, every dispatching frame still reads and rewrites all three transport fields, and drains the moment buffer, for an all-zero field

**Severity**: LOW
**Labels**: low, performance, renderer, bug
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23.md (PERF-D5-2026-09-23-04)

- **Severity**: LOW
- **Dimension**: GPU Pipeline
- **Location**:
  - `volumetrics_inject.comp:2594-2612` (unconditional `transportCombustion` + 3 `imageStore`), `:2639-2658` (`combustionMedium`, `accumulateCombustionLightMoment`, `flameSourceSampleRatio`);
  - `volumetrics.rs:1175` (`combustion_moment_dirty[frame] = true` on every dispatch) → drain `:1444-1467`.
- **Status**: NEW. This is the `dt = 0` residue of #3131. #3835 removed the drain only for fog-free frames.
- **Description**:
  - In a fogged exterior or dusty interior with no transport emitter and no linger, `dt = 0`, and each froxel still issues 3 trilinear history fetches and 3 RGBA16F stores (48 B/froxel of traffic) to carry a field that is identically empty.
  - The host still decodes and zeroes the 8 KB moment buffer every frame, because `dispatch` marks it dirty unconditionally even though the atomics cannot fire without emissive transported medium.
- **Impact**: *est.* 44 MB/frame at a 720p render extent, 100 MB at 1080p and 400 MB at native 4K (≈ 0.1 / 0.2 / 0.8 ms of bandwidth at a 500 GB/s effective rate), plus a few µs of CPU. It is bounded, but it is the common case in every fogged cell.
- **Related**: #3131, #3835, REN-D8-2026-09-23-03. That finding's option (b), zeroing the expired residual, makes a "field known-empty" latch trivially sound.
- **Suggested Fix**: Add a CPU "transport field empty" latch, armed once both FIF slots have been written with `dt = 0`, no emitter, and the linger expired (after REN-D8-03's residual clear). While it is set, pass a UBO flag that makes `main` skip `transportCombustion` / the stores / the moment and flame-ratio calls, and skip setting `combustion_moment_dirty`. Re-arm on the first transport emitter.

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-09-23.md` (`/audit-suite --preset volumetrics-deep`, HEAD `2237da9c3`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related passes (other compute passes / history buffers / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
