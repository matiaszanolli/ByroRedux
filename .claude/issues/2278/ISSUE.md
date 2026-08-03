# PERF-D9-01: gpu_timers.rs has no per-bracket "ran this frame" flag — an inactive pass and a genuinely-instantaneous one both read back 0.0

Filed from: `docs/audits/AUDIT_PERFORMANCE_2026-08-03.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2278
Labels: low, performance, bug

**Severity**: LOW
**Dimension**: Telemetry & Origin Cost (9)

**Location**: `crates/renderer/src/vulkan/gpu_timers.rs:55-70` (doc comment above `read_and_reset`)

## Description
The module's own doc comment states plainly: "There is currently no per-bracket 'ran this frame' flag exposed to consumers; `0.0` is ambiguous between 'inactive' and 'genuinely instantaneous.'" `read_and_reset` builds a fresh `GpuTimerSnapshot::default()` each call and only fills fields whose `active_bits` bit was set, but `active_bits` itself isn't surfaced to the `bench-stats`/`skin.coverage` consumers that need to distinguish "didn't run" from "ran in 0 ms."

## Evidence
`gpu_timers.rs` doc comment (confirmed present, lines ~55-70):
> "Some frames skip the skin chain entirely (no skinned draws, no RT) or skip TAA (disabled). On those frames the bracket timestamps are never written, and `read_and_reset` builds a fresh `GpuTimerSnapshot::default()` each call, only filling in fields whose `active_bits` bit was set — so an inactive bracket reads back `0.0`, not the prior frame's value. There is currently no per-bracket 'ran this frame' flag exposed to consumers; `0.0` is ambiguous between 'inactive' and 'genuinely instantaneous.'"

## Impact
Telemetry-correctness only, no runtime cost — but it undermines the instruction to "cite the GPU timer, don't guess" for other performance dimensions, since a `0.0` for e.g. the skin-dispatch bracket on a frame with no skinned draws is indistinguishable from a broken timer.

## Suggested Fix
Expose `active_bits` (or a per-field `Option<f32>`/bool pair) on `GpuTimerSnapshot` so `bench-stats` can print "n/a (didn't run)" instead of `0.0`.

## Completeness Checks
- [ ] **SIBLING**: Check other telemetry consumers (`skin.coverage`, debug UI stats) for the same 0.0-vs-inactive ambiguity
- [ ] **TESTS**: A regression test pins this specific fix (e.g. asserting `GpuTimerSnapshot` exposes per-bracket active/inactive state distinctly from a measured 0.0)
