# Batch fix: #779, #2152, #2265, #2569

All four are renderer/shader issues (domain: **renderer**, crate: `byroredux-renderer`).

## #779 — PERF-N6: triangle.frag missing `layout(early_fragment_tests)`
Re-flag of D1-M3. `triangle.frag` has two `discard` paths (alpha-threshold based, not RT-dependent),
so early-Z is spec-legal but not declared. Costs 2-3x RT query invocations on overdraw.
Fix: add `layout(early_fragment_tests) in;` after `#version`/`#extension` lines, recompile SPIR-V,
add a grep-style regression test.

## #2152 — CHAIN-D2-05: ReSTIR reservoir ping-pong reads uninitialized device memory
`ReservoirBuffers` (crates/renderer/src/vulkan/restir.rs:52-60,102-131) allocated with
`create_device_local_uninit`, never cleared on creation or resize. Shader-side validation gates
garbage reservoirs but SVGF/TAA (analogous consumers) do clear history on init — ReSTIR is the
outlier. Fix: `vkCmdFillBuffer(0)` in `ReservoirBuffers::new` and `recreate_on_resize`
(needs `TRANSFER_DST` usage bit added).

## #2265 — TD7-001: MAX_TRANSPARENT_SKIPS / MAX_OPAQUE_LAYERS triple-declared
Same 8-layer ray-walk cap hand-declared in 3 GLSL files under 2 different names:
- `crates/renderer/shaders/include/raytrace.glsl:64`
- `crates/renderer/shaders/water.frag:252`
- `crates/renderer/shaders/include/shadow_transport.glsl:11`
Fix: single source of truth in `shader_constants_data.rs` + `shader_constants.glsl` include,
replace all 3 local declarations.

## #2569 — OBL-D4-02: Legacy Lambert diffuse differs by factor of PI between paths
`lighting.glsl:154-166` (no-cluster directional fallback) uses non-/PI Lambert; `triangle.frag:2321-2332`
(clustered per-light path) uses /PI then `* vec3(0.8)`. Need to determine which is correct/intentional
before changing — issue itself flags this needs live RenderDoc validation per project's
speculative-shader-fix policy. Investigate both sites' history/intent before altering visual output.

## Classification
Domain: **renderer** → test target `byroredux-renderer`.
