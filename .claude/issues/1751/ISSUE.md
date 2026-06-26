# TD2-002: compute-pipeline create dance duplicated ~5x; helper already exists in bloom.rs

_Filed 2026-06-26 as #1751 from docs/audits/AUDIT_TECH_DEBT_2026-06-26.md (immutable snapshot; query `gh issue view 1751` for live state)._

**Severity**: MEDIUM (duplicated logic with shotgun-edit history) · **Dimension**: 2 — Logic Duplication
**Location**: helper `crates/renderer/src/vulkan/bloom.rs:984` (private); re-inlined at `ssao.rs:343-369`, `volumetrics.rs:371-394`+`:537-560`, `skin_compute.rs:319-346`+`:773-797`; "module-on-partial" variant at `taa.rs:365`, `caustic.rs:367`, `svgf.rs:578`+`:923`
**Status**: NEW · **Audit**: TD2-002

## Description
`bloom::create_compute_pipeline` does the full load_shader_module → PipelineShaderStageCreateInfo → ComputePipelineCreateInfo → create_compute_pipelines → destroy_shader_module (Ok + Err) sequence. The inner `create_compute_pipelines(...).map_err(|(_,e)|e)` is byte-equivalent across ~9 sites.

## Evidence — shotgun-edit history
- bloom helper + first inline copy born SAME commit `33f48b56e`.
- `e2a4a8259` touched the same line in `caustic.rs:370` AND `ssao.rs:346`.
- `dde22c37e` added the same SAFETY comments to both `caustic.rs:381` and `ssao.rs:356`.

## Impact
Module-destroy ordering / pipeline-cache wiring is hand-replicated; a single semantic change already had to be applied to 2+ copies by hand. Divergence risk on shader-module leak handling.

## Suggested Fix
Promote `create_compute_pipeline` into `crates/renderer/src/vulkan/pipeline.rs` (next to `load_shader_module`) as `pub(crate)`; route ssao + volumetrics(×2) + skin_compute(×2) + bloom through it. The taa/caustic/svgf "module stored on partial" variant should switch to the self-managed-module form (module is destroyed right after creation, so storing it buys nothing).

## Completeness Checks
- [ ] **SIBLING**: all ~9 compute-pipeline-create sites routed through the one helper
- [ ] **DROP**: shader-module destroy still happens on both Ok and Err paths
- [ ] **TESTS**: each affected pass still builds + runs (no leaked module / no double-free)
