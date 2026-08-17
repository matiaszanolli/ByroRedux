# REN-D2-01: a missing GPU timer silently disables one-bounce GI

**Issue**: #3044
**Severity**: MEDIUM
**Labels**: `medium,renderer,bug`
**Source report**: `docs/audits/AUDIT_RENDERER_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_RENDERER_2026-08-16.md` (Dimension — Ray Queries / SSBO plumbing).

**Location**: `crates/renderer/src/vulkan/scene_buffer/ray_budget.rs` (`AdaptiveRayBudget::observe`, `::settings`) · `crates/renderer/src/vulkan/context/draw.rs` (the `measured_lighting_ms` binding) · `crates/renderer/src/vulkan/context/mod.rs` (the `gpu_timers` match arm) · `crates/renderer/shaders/triangle.frag` (the GI gate)

## Description

The adaptive ray-budget controller has exactly **one** input, and it comes from an explicitly best-effort subsystem.

`GpuPerFrameTimers::new` returns `Ok(None)` when the driver lacks `timestamp_compute_and_graphics`, and the construction site swallows a creation error into `None` as well — logging only that *"PERF-DIM7 instrumentation will read zeros"*.

`draw_frame` derives `measured_lighting_ms` from `self.gpu_timers.as_ref().map(..)`, so it is `None` on every frame in that case. `observe` then early-returns via `let Some(sample) = measured_lighting_ms.filter(..) else { return; }`, leaving `tier` at its cold-start value of **0 permanently**. There is no time-based or frame-count-based fallback promotion.

## Impact

On any device without GPU timestamp support — or where timer creation merely failed — **one-bounce GI is silently off for the entire session**. The only diagnostic is a log line about instrumentation reading zeros, which does not mention that a rendering feature was disabled as a consequence.

A missing *diagnostic* capability silently disables a *rendering* capability. That coupling is the defect, independent of how often the driver condition occurs.

## Suggested Fix

Decouple the two: when no timer sample is available, promote the tier on a frame-count or wall-clock fallback so GI reaches its normal budget, and log once at `warn` that the adaptive controller is running open-loop.

Failing that, at minimum make the log say what was disabled.

## Related

- #2807-adjacent PERF-DIM7 instrumentation
- `crates/renderer/src/vulkan/gpu_timers.rs`

## Completeness Checks
- [ ] **DECOUPLED**: Absent instrumentation no longer disables a rendering feature
- [ ] **OPEN-LOOP-LOG**: One clear warning names the consequence, not just the missing timer
- [ ] **SIBLING**: Any other consumer gated on `gpu_timers` checked for the same coupling
- [ ] **TESTS**: A unit test drives `observe` with `None` samples and asserts the tier still promotes

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3044 --json state` when live state is needed.*
