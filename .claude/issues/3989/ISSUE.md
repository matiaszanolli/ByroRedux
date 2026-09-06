# #3989 — REN-2026-09-06-D3-01: `shader-pipeline.md` marks two live lanes "reserved", one of them the exact lane the code names as the next expansion hazard

**Labels**: medium, memory, renderer, shaders, documentation, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D3-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: GPU-Struct Layout
- **Location**: `docs/engine/shader-pipeline.md` (§GPU Data Types — the `GpuCamera` table's `render_debug` row at offset 336, and the `material_flags` bit table's bit-10 row)
- **Status**: NEW
- **Description**: Two rows of the authoritative GPU-layout doc describe live lanes as free.
  1. `GpuCamera.render_debug` is documented as `"… z = diagnostic LOD-counter enable; w reserved"`. `w` is not reserved: `pack_weather_surface` (`crates/renderer/src/vulkan/context/assemble_camera_and_lights.rs`) writes rain wetness into its low 16 bits and snow coverage into its high 16, and `triangle.frag` decodes both. All five GLSL `CameraUBO` mirrors carry the correct comment; only the doc is wrong.
  2. The `material_flags` table lists bit 10 as `*(unused/reserved)*`. Bit 10 is `material_flag::BGSM_AUTHORED` (`crates/renderer/src/vulkan/material.rs`) — live host-side provenance, deliberately *not* mirrored to GLSL (`crates/renderer/build.rs` and `crates/renderer/src/shader_constants_data.rs` both carry an explicit "intentionally NOT emitted" note). `docs/engine/renderer.md` documents it correctly; `shader-pipeline.md` does not.
- **Evidence**:
  - `assemble_camera_and_lights.rs` builds `render_debug: [mode, lod_scale_bits, lod_telemetry, pack_weather_surface(sky_params.weather.surface_wetness, sky_params.weather.surface_snow)]`.
  - `triangle.frag` decodes it as `float(renderDebug.w & 0xFFFFu)` / `float(renderDebug.w >> 16u)`.
  - `pack_weather_surface` is unit-tested by `packs_wetness_low_and_snow_high`.
  - `DBG_BITS`' own doc comment in `shader_constants_data.rs` says future debug expansion "must coordinate with the history-dependent weather-surface payload already carried in `GpuCamera.render_debug.w`; that lane is no longer an unused flag word."
  - `material_flag::BGSM_AUTHORED: u32 = 1 << 10;`
- **Impact**: The `DBG_*` mask is now fully exhausted (32/32 bits, see the Coverage table), so the *next* debug-flag expansion is precisely the change the code warns must not silently claim `render_debug.w`. An author following the audit skill's own instruction — treat `shader-pipeline.md` as authoritative, do not re-derive — reads "w reserved" and takes a lane that carries per-frame weather state consumed by `triangle.frag`, silently breaking wetness/snow response with no test failure. Bit 10 has the mirror-image risk: a new shader-visible `MAT_FLAG_*` allocated at the "unused" bit 10 would collide with the host-side provenance bit that `cell_loader.rs` already sets.
- **Related**: This is the third instance of the same class in this struct family — #1928 (`VolumetricsParams.render_origin.w` documented free while `volumetrics_inject.comp` read it) and #2750 / REN-D3-2026-08-12-02 (`GpuCamera.dof_params` documented `zw = reserved (0)` over two live consumers). The `render_origin` row *immediately above* the wrong one carries an explicit "Not a free slot — same trap as `VolumetricsParams.render_origin.w` (#1928)" warning, and the next row repeats the mistake the warning names. Distinct from the **closed** #3447 / `REN-2026-08-27-D3-01` ("shader-pipeline.md still documents GpuInstance at 128 B and GpuCamera at 352 B"), whose fix `03407ae3` corrected the *size* literals in this same document — both sizes verified correct today (160 B / 368 B). This finding is about field semantics in two rows that fix did not touch. Also distinct from #3846, which is the same class in `bindings.glsl` rather than the doc.
- **Suggested Fix**: Change the offset-336 row to `w = packed weather surface (low 16 bits rain wetness, high 16 bits snow coverage), consumed by triangle.frag — not a free slot`, and change the bit-10 row to `MAT_FLAG` `BGSM_AUTHORED` — host-side only, deliberately not mirrored to GLSL. Both should carry the same "not a free slot" phrasing the `render_origin` row already uses.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
