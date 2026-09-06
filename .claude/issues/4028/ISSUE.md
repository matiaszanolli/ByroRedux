# #4028 — REN-2026-09-06-D3-04: `CameraUBO` is the only mirrored GPU struct without a mirror-discovery guard — the exact hole `fa5c4191` just closed for `GpuInstance`

**Labels**: low, renderer, shaders, test-gap, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D3-04), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs` (`camera_ubo_glsl_copies_stay_in_lockstep`, `assert_mirror_list_is_complete`, `shader_sources_declaring`)
- **Status**: NEW — residual scope gap of the **closed** #3564 (`0d4a2e70`, "discover GLSL mirror sites instead of hardcoding each guard's list"), not a re-file of it
- **Description**: #3564 replaced every mirror guard's hardcoded `SOURCES` list with `assert_mirror_list_is_complete`, which walks the shader tree and fails if any source declares the struct without being listed. It reached four of the five hand-mirrored GPU structs: `struct GpuInstance` (#2748 / #3564), `struct GpuLight` (#1916 / #3564), `struct WaterParams` (#3564), `struct GpuTerrainTile` (#2463 / #3564). `camera_ubo_glsl_copies_stay_in_lockstep` (#3684, landed after #3564) deliberately does not call it, because `shader_sources_declaring` requires `decl` to start the trimmed line and `CameraUBO` is always preceded by a `layout(...)` qualifier. Its own doc comment records the residual gap. `CameraUBO` is therefore the one mirrored GPU struct that #3564's mechanism still does not cover.
- **Evidence**:
  - `grep -rn "uniform CameraUBO" crates/renderer/shaders/` returns exactly the 5 sites in the test's `SOURCES` — **no live drift today**.
  - The test's doc: *"A sixth shader adding `CameraUBO` without joining this SOURCES list is a real but narrower gap … and isn't in the issue's own suggested fix."*
- **Impact**: Regression-guard gap, not a live defect. `fa5c4191` (#3829) landed yesterday after exactly this class cost 13 days of silently misaligned reads: `volumetrics_inject.comp`'s `GpuBoundaryInstance` was a 6th `GpuInstance` mirror outside both the `SOURCES` list and the discovery grep, kept a 128-byte stride through #3231's growth to 160, and misaligned every boundary-geometry read past index 0 with a fully green suite. `GpuCamera` is 368 B across 5 mirrors and grew twice in three weeks (352 → 368 for `exterior_sky_tint`, #3323); it is the most likely next struct to grow a sixth reader.
- **Related**: #3684 (the field-lockstep test); #3829 / `fa5c4191` (the realized cost of an untracked mirror); #3564 (introduced `assert_mirror_list_is_complete`).
- **Suggested Fix**: Give `shader_sources_declaring` an optional "declaration may be preceded by a `layout(...)` qualifier" mode (or a second helper that strips a leading `layout(...)` before the prefix test) and call `assert_mirror_list_is_complete("uniform CameraUBO {", SOURCES, "#3684")`. The existing strict behaviour must stay the default so `skin_vertices.comp`'s *comment* mentioning `struct GpuInstance` keeps not matching.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
