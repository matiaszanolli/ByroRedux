# TD3-NEW-03: feature-matrix.md's Rendering table doesn't mention FSR 3.1 default upscaler

**Labels**: medium, renderer, tech-debt, documentation
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-07-25.md` (TD3-NEW-03)
**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2192

## Severity
MEDIUM

## Dimension
3 (Stale Documentation & Comments)

## Location
`docs/feature-matrix.md:39` (Rendering table, TAA row)

## Description
The Rendering table's only temporal-reconstruction row is `| **TAA** | ✓ All games | Halton(2,3) jitter, YCoCg variance clamp |`. Per ROADMAP.md's Session 60 closeout, FSR 3.1 Quality is now the engine's default upscaler (+40% to +68% net frame recovery, landed 2026-07-24), with `--upscaler taa` retained only as a fallback. `feature-matrix.md` has no row reflecting this. Same failure mode as the already-fixed TD3-101 recurring one file-section over.

## Evidence
`docs/feature-matrix.md:39`; ROADMAP.md's Session 60 closeout; `crates/renderer/src/vulkan/upscaling.rs`'s `UpscalerMode` default resolves to FSR Quality per commit `5c7acfe2`.

## Impact
A reader using feature-matrix.md as the "what renders today" reference would misjudge current upscaling behavior and the `--upscaler taa` fallback flag's existence.

## Related
TD3-101 (closed, 07-16 report) — same file, same recurring pattern class.

## Suggested Fix
Add an "Upscaling" row: `FSR 3.1 (default, Quality preset) / TAA native (--upscaler taa fallback)`, all games, with a one-line note on the presets and the FP32-permutation caveat already tracked in ROADMAP.md.

## Completeness Checks
- [ ] **SIBLING**: Same feature-matrix-lags-shipped-code pattern checked across other recently-shipped features (quest alias system, Follow/Escort/Guard/Patrol AI)
- [ ] **TESTS**: N/A — documentation-only fix
