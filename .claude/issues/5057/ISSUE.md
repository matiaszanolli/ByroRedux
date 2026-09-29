# #5057 — PERF-D5-2026-09-29-01: Early-fragment-test eligibility admits only material_kind == 0, although lighting-shader kinds 1–16 have no discard or depth-write path

**Labels**: low, bug, performance, renderer, shaders, pipeline

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-09-29.md` — finding `PERF-D5-2026-09-29-01`

**Severity**: LOW (an optimization gap in a landed feature; benefit unmeasured)

**Dimension**: GPU Pipeline

**Location**: `crates/renderer/src/vulkan/context/types.rs:374-384` (`DrawCommand::allows_early_fragment_tests`); kind source `crates/nif/src/import/material/dedicated_shader.rs:488` (`material_kind = shader_type`); discard sites `crates/renderer/shaders/triangle.frag:463,477,1208,1249,1290`

**Status in report**: NEW

## Description

the certificate's doc asks for other kinds to be reviewed against every discard in `triangle.frag`. That review is mechanical:
- `triangle.frag` never writes `gl_FragDepth`.
- Its discards are the alpha test (already excluded by `alpha_threshold == 0.0`), the blend-only transparent-texel cull (already excluded by `!alpha_blend`), and the `MATERIAL_KIND_EFFECT_SHADER` (101) and `MATERIAL_KIND_FIRE_REFRACTION` (103) blocks.
- `BSLightingShaderProperty` kinds 1–16 (env map, glow, parallax variants, face/skin/hair tint, eye env) only change shading.

Every such draw is still routed to the late-test pipeline.

## Impact

on Skyrim and FO4 interiors, where env-mapped, glow and skinned-actor surfaces are a large share of opaque pixels, the early-Z saving is not realized for those draws. The opaque interval is about 95% of MedTek's main pass (26b report).

## Suggested Fix

replace `== 0` with an explicit allow-list of reviewed kinds, pinned by a source-scan test that fails when a new `discard` or `gl_FragDepth` appears under a `materialKind` branch. A/B it with `BYRO_PROFILE=1` `opaque_fragment_invocations` and `main_opaque_ms` at a pinned tier, and repeat the Kendall visual gate.

Validated at HEAD 9fcfdc3fc: `DrawCommand::allows_early_fragment_tests` (`crates/renderer/src/vulkan/context/types.rs`) still requires `self.material_kind == 0`; `triangle.frag` has no `gl_FragDepth` write and its discards are alpha-test, blend-only cull, effect-shader and fire-refraction branches.

## Completeness Checks
- [ ] **SIBLING**: every `discard` / depth-write site in `triangle.frag` (the early variant `triangle_early.frag.spv` is built from it) reviewed against the allow-list
- [ ] **TESTS**: a source-scan pin (via `source_scan::production_text`) fails when a new `discard` / `gl_FragDepth` appears under an allow-listed `materialKind` branch
