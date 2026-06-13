## Finding REN2-18 — Renderer Audit 2026-06-11

- **Severity**: LOW
- **Dimension**: Water (doc-rot)
- **Location**: `crates/renderer/shaders/water.frag:52` ("x = time (seconds since cell load)") vs `byroredux/src/render/water.rs:43-46` (sources `TotalTime`)
- **Status**: NEW. Validated CONFIRMED at HEAD `1e8a25ab`.

## Description

The push-constant doc claims `time` is seconds since cell load; the actual source is engine-uptime `TotalTime` (`world.try_resource::<TotalTime>()`), which per `crates/core/src/ecs/resources.rs:164` is "accumulated wall-clock time since engine start", inserted once and never reset on cell load. The doc hides a known long-session f32 wave-quality bound (uptime precision degrades wave animation after hours of play).

## Suggested Fix

Fix the comment to "engine uptime (`TotalTime`)" and note the f32 precision bound. Fold into the doc-rot pass.

## Completeness Checks
- [ ] **SIBLING**: Check other time-consuming shaders (caustics, volumetrics) for the same wrong "since cell load" claim
- [ ] **TESTS**: N/A (doc-only)

---
Source: `docs/audits/AUDIT_RENDERER_2026-06-11.md` · Filed by `/audit-publish`
