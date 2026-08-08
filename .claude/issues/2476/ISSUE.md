# REN-D20-NEW-02: Debug-UI HUD sums GPU bracket times into a total the timer module explicitly forbids

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2476
**Finding ID**: REN-D20-NEW-02 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 20 — Debug/Telemetry
**Location**: `crates/debug-ui/src/panels.rs:175-176` (producer: `byroredux/src/systems/metrics.rs:110-131`)
**Status**: NEW

## Description
`GpuTimerSnapshot`'s doc comment states the contract in as many words: every bracket's START is written at `TOP_OF_PIPE`, so queue-drain time from prior in-flight work is absorbed into the bracket — "the fields must NOT be summed into a 'total GPU ms' without that caveat, since overlapping queue-wait could be double-counted across adjacent brackets." The HUD does exactly that sum and presents it as an unqualified headline figure.

## Evidence
```rust
// crates/debug-ui/src/panels.rs:175
let gpu_total: f32 = m.gpu_pass_ms.iter().map(|(_, v)| *v).sum();
ui.label(egui::RichText::new(format!("GPU passes — Σ {:.3} ms", gpu_total)).strong());
```
Contract being violated — `crates/renderer/src/vulkan/gpu_timers.rs:124-132`: "Upper bound, not a precise attribution (#2040 / PERF-D9-01). ... the fields must NOT be summed into a 'total GPU ms' ..."

## Impact
The overlay is the primary tool used to chase frame-time pathologies. A Σ that double-counts queue-wait across 14 brackets will read materially higher than wall GPU time, and the adjacent "CPU draw_frame — Σ" label invites a direct GPU-vs-CPU comparison the GPU number cannot support. Risk is a misdiagnosed perf bug, not a crash.

## Related
#2040 / PERF-D9-01 (the finding that established the non-summability caveat); the sibling `GpuTimerSnapshot::*_active` finding in this report (same telemetry surface, same root cause of caveats not reaching the UI).

## Suggested Fix
Either drop the Σ from the GPU row (keep the per-pass grid, which is sound), or relabel it to something honest like "Σ upper bounds (overlaps double-counted)" and mirror the `gpu_timers.rs` caveat in a tooltip.

## Completeness Checks
- [ ] **TESTS**: N/A (UI label change); confirm no test asserts the old Σ label text
