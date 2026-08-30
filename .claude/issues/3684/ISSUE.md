# #3684 — PERF-D4-2026-08-30-04: `CameraUBO` is the only hand-duplicated GPU struct with no field name/order/type lockstep test — it is pinned by size alone

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D4-2026-08-30-04`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: low,performance,renderer,shaders,test-gap,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3684

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: LOW
- **Dimension**: SSBO Sizing & Upload
- **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:359` (`pub struct GpuCamera`), `crates/renderer/src/vulkan/reflect.rs:606-641`
  (`camera_ubo_size_matches_gpu_camera_in_every_shader`),
  `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:66-79`
  (`gpu_camera_is_368_bytes`); the five GLSL declarations at
  `crates/renderer/shaders/include/bindings.glsl:280`, `triangle.vert:106`, `water.vert:83`,
  `cluster_cull.comp:57`, `caustic_splat.comp:68`
- **Status**: NEW (test-gap; **no live drift** — all five declarations verified identical at
  368 B this session)
- **Description**: Every other multi-copy GPU struct in this crate has a parsed, field-by-field
  lockstep test: `GpuInstance` across five GLSL mirrors plus the Rust struct
  (`gpu_instance_glsl_copies_stay_in_lockstep`, #2748), `GpuLight` across four
  (`gpu_light_glsl_copies_stay_in_lockstep`, #1916), `GpuMaterial` against
  `include/bindings.glsl` including a per-field scalar-type check
  (`gpu_material_glsl_field_order_matches_rust_struct`, #1657 / #2688). `CameraUBO` — declared
  by hand in five GLSL sources and read by six shaders — has neither. Its only guards are
  `size_of::<GpuCamera>() == 368` on the Rust side and a SPIR-V *block size* reflection pin on
  the shipped `.spv`. Both are blind to a within-size reorder (`skyTint` ↔ `sunDirection`,
  say — two adjacent `vec4`s in a struct that is entirely `vec4`s) and to a type flip
  (`uvec4 renderDebug` → `vec4`, whose contents are bitcast flags). #2688 established that exact
  type-flip class as "byte-lethal" for `GpuMaterial` and added a check; the camera got none.

  The parser infrastructure to close it already exists in the same file
  (`parse_glsl_struct_fields_typed` / `parse_rust_struct_fields_typed`,
  `shader_contract_tests.rs:1260-1300`).

  Adjacent, same root: four shader sources still direct the reader to `scene_buffer.rs` for the
  camera contract — `caustic_splat.comp:62`, `cluster_cull.comp:51`, `cluster_cull.comp:59`,
  `triangle.vert:99` (the last spells out `crates/renderer/src/vulkan/scene_buffer.rs`). That
  file was split into `scene_buffer/` in Session 34; the struct now lives in
  `scene_buffer/gpu_types.rs`.
- **Evidence**:
  ```rust
  // reflect.rs:632-641 — size only, no field identity
  let size = uniform_block_size_by_name(spv, "CameraUBO")…;
  assert_eq!(size, expected, "{name}.spv CameraUBO is {size} B but GpuCamera is {expected} B …");
  ```
  ```rust
  // shader_contract_tests.rs:1746-1748, on the GpuInstance test — naming the precedent
  /// … the full lockstep guard `GpuMaterial` and `GpuLight` already have.
  ```
- **Impact**: None today. The exposure is that the camera UBO's five hand-maintained copies are
  the least-guarded GPU contract in the renderer, in a repo where this exact class has recurred
  eight times (#417, #1447, #1493, #1657, #1916, #2688, #2748, #3231) and where a same-size
  reorder produces wrong lighting/motion-vector math with no validation-layer signal.
- **Related**: #2748, #1916, #1657, #2688, #1447; #3447 (the stale "352 B … plus ten" prose in
  `gpu_camera_is_368_bytes`'s own doc comment is already listed in that issue's locations and is
  **not** re-reported here).
- **Suggested Fix**: Add `camera_ubo_glsl_copies_stay_in_lockstep` alongside the `GpuLight` and
  `GpuInstance` tests, parsing `uniform CameraUBO` out of the five GLSL sources with the
  existing typed parser and comparing name, order and scalar type against
  `pub struct GpuCamera`. Repoint the four stale `scene_buffer.rs` shader comments at
  `scene_buffer/gpu_types.rs` in the same change.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
