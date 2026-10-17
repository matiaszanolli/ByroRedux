# PERF-VOL-05: Docs contradict the shipped default froxel divisor, and the ledger omits the integration UBO

Labels: low,performance,documentation,doc-rot
Source: docs/audits/AUDIT_PERFORMANCE_2026-10-17.md

- **Severity**: LOW
- **Dimension**: GPU Memory Pressure (Dim 3), doc rot
- **Location**: `docs/engine/memory-budget.md:620-628` (and `:492`, `:581-604`); `crates/renderer/src/vulkan/volumetrics.rs:189-199` (`volumetrics_host_buffer_bytes`); `.claude/commands/audit-performance/SKILL.md` pass-inventory "Volumetrics" row
- **Status**: NEW
- **Description**: (a) The section's closing paragraph says "The `/4` quality default is denser … a 4× VRAM and 4× inject-dispatch change that has not been through a bench-of-record refresh", in present tense. The default has been 8 since #3611 era (`upscaling.rs:146`, with a comment explaining the retreat from 4), and the same section states 8 four times. (b) `volumetrics_host_buffer_bytes` and its ledger table omit `integration_param_buffers` (2 × 16 B `IntegrationParams` UBO, `volumetrics.rs:659`, `init.rs:739`). 32 B logical, so no budget effect, but the test `volumetrics_host_buffer_ledger_matches_the_code` cannot see it. (c) The audit SKILL's pass-inventory row says Volumetrics "Runs when `VOLUMETRIC_OUTPUT_CONSUMED`". That constant is permanently `true` (`volumetrics.rs:444`); the real gate is `requires_dispatch` plus TLAS, cluster-light and geometry inputs being present (`post_passes.rs:663-782`).
- **Evidence**: As cited. Ledger test passes (79 volumetrics tests green).
- **Impact**: Misleads a reader into thinking 4× the VRAM and dispatch is live.
- **Suggested Fix**: Rewrite the last paragraph of the section as history; add the UBO to the ledger or note it as below the allocation granularity; reword the SKILL row.

_Filed from `docs/audits/AUDIT_PERFORMANCE_2026-10-17.md` (area-scoped `volumetrics-deep` suite run, HEAD e16a624e9). Fix with `/fix-issue <number>`._
