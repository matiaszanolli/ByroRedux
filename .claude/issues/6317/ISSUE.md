# #6317: REN-L-01: `Reservoir` now has two GLSL copies, `include/bindings.glsl` and `shadow_filter.comp`, with no lockstep test between them

**Labels**: low,renderer,shaders,test-gap,bug

**Source**: docs/audits/AUDIT_RENDERER_2026-10-21.md · **Severity**: LOW · **Dimension**: GPU-Struct Layout · **Location**: `crates/renderer/shaders/shadow_filter.comp (struct Reservoir)`

- **Location**: `crates/renderer/shaders/shadow_filter.comp` `struct Reservoir` (its own comment: "Not yet pinned by a GLSL-copies lockstep test"); `shader_contract_tests.rs` classification `("Reservoir", MirroredPendingGuard(…))`; `restir.rs` `reservoir_stride_matches_shader` (asserts `RESERVOIR_STRIDE == 8 * 4` against a literal, reads no GLSL)
- **Status**: NEW
- **Description**: `every_shader_struct_is_classified` matches by name, so the second copy passes. The stride test reads neither copy. A field reorder in `bindings.glsl`, for example moving the packed normal out of `pad0`, would leave the filter decoding ratio and guides from the wrong lanes.
- **Suggested Fix**: have `shadow_filter.comp` include the shared definition (as `struct GpuMaterial` lives only in `bindings.glsl`), or add both copies to a GLSL-vs-GLSL `assert_glsl_matches_rust`-style comparison.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
