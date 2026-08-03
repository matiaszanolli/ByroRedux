# Issues 2234, 2235, 2236, 2237

## #2234 — REN-D9-01: No test verifies the committed skin-shader .spv matches the Rust-side stride/workgroup constants it must bake in

**Location**: `crates/renderer/shaders/skin_vertices.comp` (`SKIN_WORKGROUP_SIZE`, `SKIN_OUTPUT_STRIDE_FLOATS`, `VERTEX_STRIDE_FLOATS`, `MAX_BONES_PER_MESH`); `scripts/check-shader-artifacts.sh`

`check-shader-artifacts.sh` only verifies GLSL↔SPIR-V byte-reproducibility, not that the GLSL constant literals agree with their Rust-side counterparts (dispatch/buffer-layout code). A future one-sided edit would compile clean and silently corrupt skinned-vertex output.

**Fix**: add a test (or extend the script) cross-checking GLSL constant literals against Rust constants, following the GpuInstance/GpuMaterial lockstep-guard pattern.

## #2235 — REN-D10-01: The new fog-volume system has no debug_assert tying it to the documented RT absolute-space precision ceiling

**Location**: `crates/renderer/src/vulkan/volumetrics.rs` (fog-volume upload/dispatch path)

Fog volume centers/extents are stored/consumed in absolute world coordinates but have no `debug_assert` against the documented RT float-precision ceiling, unlike other absolute-space consumers (e.g. caustic-splat `#markarth-precision`).

**Fix**: add `debug_assert!` at fog-volume upload time checking volume-center magnitude against the same precision ceiling used elsewhere.

## #2236 — REN-D11-02: Fire-refraction proxies overwrite the opaque receiver's G-buffer normal at any coverage, including near-zero

**Location**: `crates/renderer/shaders/triangle.frag` (fire-refraction branch)

`outNormal = octEncode(macroN)` is written unconditionally in the fire-refraction branch, unlike `outAlbedo`/`outRawIndirect` which are correctly gated/zeroed. This corrupts SVGF/TAA disocclusion and normal-dependent lighting for any receiver behind a near-invisible proxy.

**Fix**: gate the `outNormal` write by `proxyCoverage` (blend toward previously-written receiver normal, or skip below a threshold).

## #2237 — REN-D12-02: Fire-refraction's composition-phase sort key globally inverts back-to-front order against unrelated transparents

**Location**: `byroredux/src/render/mod.rs` (`draw_sort_key`, ~line 209; `MATERIAL_KIND_FIRE_REFRACTION` special-case ~line 224); `byroredux/src/render/static_meshes.rs:223`

The fire-refraction special-case in `draw_sort_key` inverts sort order relative to the *entire* alpha-over transparent set instead of only relative to other fire-refraction proxies, causing wrong back-to-front compositing against unrelated transparents (smoke, glass) sharing screen space.

**Fix**: scope the sort-key adjustment to only reorder relative to other fire-refraction proxies.

## Domain classification
- 2234: renderer (byroredux-renderer) — shader/build artifact test
- 2235: renderer (byroredux-renderer) — volumetrics precision assert
- 2236: renderer (byroredux-renderer) — triangle.frag shader logic
- 2237: binary (byroredux) — render/mod.rs draw_sort_key (binary crate, not byroredux-renderer)
