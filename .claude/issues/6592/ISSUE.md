# REN-D12-01: A comment in `dispatch_skin_and_cluster.rs` still names the deleted `cmd_blas_refit_end` timer method

Labels: low,renderer,documentation,doc-rot
Source: docs/audits/ (2026-10-31 report)

**Severity**: LOW
**Dimension**: Renderer / Debug-Telemetry (doc-rot)
**Source**: `docs/audits/AUDIT_RENDERER_2026-10-31.md` REN-D12-01

## Location
`crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs` (~:301): the comment above the unconditional AS-write to AS-read barrier before the TLAS build says "it avoids disturbing the `cmd_blas_refit_end` GPU-timer bracket". Re-verified present at HEAD `670f81917`.

## Description
#6358 (`0288f468d`) replaced the 25 `cmd_<pass>_start/_end` method pairs with `cmd_start` / `cmd_end(TimerPass::...)`. Its grep for stale names covered docs and tests but not this prose reference. The sentence cites a method that no longer exists; the bracket it refers to is `cmd_end(TimerPass::BlasRefit, ...)` in `skinned_blas_refit.rs`.

## Evidence
`grep -rE "cmd_[a-z_]+_(start|end)\b" crates byroredux docs/engine .claude/commands` (excluding `cmd_start`/`cmd_end`, the geometry-phase and fragment-invocation methods) returns exactly this one hit outside historical `docs/audits/` files.

## Impact
None at runtime. A reader chasing the #1194 bracket by method name finds nothing.

## Suggested Fix
Reword to "the `TimerPass::BlasRefit` GPU-timer bracket".
