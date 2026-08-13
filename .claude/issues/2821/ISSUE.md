# REN-D20-02: GpuTimerSnapshot _active flags ignored by four telemetry readers, reintroducing 0.0-vs-not-run ambiguity

Labels: low, renderer, bug

## Description

#2278's fourteen `GpuTimerSnapshot::*_active` flags exist so `0.0 ms` can be told apart from "this bracket did not run", and #2513 taught the egui panel to render `n/a` — but the other four readers of the same fields still print the raw `f32`: `gpu_breakdown` (the SLOW-FRAME / 1 Hz log line, i.e. the **primary hitch-triage surface**), `skin.coverage` (under a comment still stating the ambiguity as unavoidable), the `bench:` summary line (consumed by `scripts/fsr_bench_report.py`, so a skipped bracket lands in the TSV as a hard `0.000`), and `ctx.upscaler` (whose `UpscalerTelemetry::gpu_ms` has no `_active` mirror at all). Re-opens exactly the ambiguity the plumbing was built to close. Note any bench-line format change must move in lockstep with the report script.

## Location

`byroredux/src/systems/debug.rs`, `byroredux/src/commands/assets.rs`, `byroredux/src/main.rs`, `byroredux/src/commands/world_info.rs`

## Source

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D20-02).

https://github.com/matiaszanolli/ByroRedux/issues/2821
