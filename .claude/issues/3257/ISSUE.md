# 3257: ECS-2026-08-24-10: submersion_system's disturbance_events is the one per-frame Vec::new() the 7ab70089 scratch-hoisting sweep missed

**Severity**: LOW · **Report**: `docs/audits/AUDIT_ECS_2026-08-24.md` (ECS-2026-08-24-10)

## Description

`perf(watal): reuse per-frame interaction storage` (`7ab70089`) converted `make_water_interaction_system`'s five per-frame collections to hoisted scratch, but `submersion_system` in the same file still allocates `let mut disturbance_events = Vec::new();` per call. Bounded and small (pushes only for water planes within `DISTURBANCE_RADIUS` of the camera), far below the original sweep's ~100/frame target class.

## Location

`byroredux/src/systems/water.rs:250`

## Suggested Fix

`submersion_system` is a plain `fn(&World, f32)` (can't hold closure state) — needs either a `WaterDisturbanceScratch: Resource` mirroring `FootstepScratch`, or conversion to a `make_*_system()` factory like its sibling in the same file. Low priority.

## Completeness Checks
- [ ] **TESTS**: N/A — perf-only change, existing behavior tests should stay green
