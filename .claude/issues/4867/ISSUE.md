# #4867: REN-D12-2026-09-24-04: named render-debug modes are not mutually exclusive with twelve of the legacy `DBG_VIZ_*` categorical bits

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D12-2026-09-24-04**._

- **Severity**: LOW (diagnostic-only path: needs `BYROREDUX_RENDER_DEBUG` at launch plus a runtime `render.debug`; production unaffected).
- **Dimension**: Debug/Telemetry
- **Location**: `crates/renderer/shaders/triangle.frag` — `main` (early legacy exits ~788-890; output else-if chain ~4463-4560); `context/render_debug.rs` `set_render_debug_mode`; `context/assemble_camera_and_lights.rs` (`jitter.z` upload).
- **Status**: NEW
- **Description**: Only the six selector views are gated `legacyDebugMode && (dbgFlags & …)`. `DBG_VIZ_NORMALS`, `TANGENT`, `RENDER_LAYER`, `GLASS_PASSTHRU`, `AO`, `MOTION`, `MATERIAL_STATE`, `GI_BOUNCE`, `FSR_TEMPORAL`, `NONFINITE`, `SHADOW_OFFSET` and `NORMAL_DIVERGENCE` are checked un-gated. `set_render_debug_mode` only writes `render_debug_mode`, and `render_debug_flags` (env) is OR-ed into `jitter.z` unconditionally. Downstream raw-output policy is decided by the named mode alone (`render_debug_requires_raw_output` returns `mode != FINAL`).
- **Impact**: Launch with `BYROREDUX_RENDER_DEBUG=0x200` (AO), then `render.debug direct_only`: the screen shows the AO scalar labelled direct-only. `render.debug final` with the same bit: triangle still paints AO, but composite, bloom, TAA, FSR and presentation treat `final` as non-raw, so the AO oracle is fogged, bloomed, smoothed and tone-mapped, the contamination the raw policy exists to prevent.
- **Suggested Fix**: Make the un-gated legacy categorical sites consult `legacyDebugMode`, or have `set_render_debug_mode` mask `render_debug_flags` down to the orthogonal ablation bits when leaving `LegacyFlags`. Pin it with a source test that enumerates the `DBG_VIZ_*` catalog and checks each `dbgFlags & DBG_VIZ_` site is gated. Shader change verified by `cargo test` plus `check-shader-artifacts.sh`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)

