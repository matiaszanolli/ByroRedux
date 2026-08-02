# REN-D9-01: No test verifies the committed skin-shader .spv matches the Rust-side stride/workgroup constants it must bake in

Severity: medium
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2234

**Dimension**: 9 (GPU skinning / BLAS refit)
**Location**: `crates/renderer/shaders/skin_vertices.comp` (`SKIN_WORKGROUP_SIZE`, `SKIN_OUTPUT_STRIDE_FLOATS`, `VERTEX_STRIDE_FLOATS`, `MAX_BONES_PER_MESH` — baked into the compiled `.spv` at build time); `scripts/check-shader-artifacts.sh` (verifies GLSL↔SPIR-V reproducibility but not constant-value agreement with Rust)
**Status**: NEW

**Description**: The skinning compute shader's workgroup size and vertex-stride constants are shared with Rust-side dispatch/buffer-layout code, but nothing verifies the *committed* `.spv` binary was actually compiled with the current values of those constants — `check-shader-artifacts.sh` only checks that the committed SPIR-V is byte-reproducible from the GLSL source, not that the GLSL's constant values agree with the Rust side. Currently benign (values agree today) but the mechanism that would catch drift doesn't exist.

**Impact**: A future change to a Rust-side stride/workgroup constant without a matching shader-side edit (or vice versa) would compile and pass `check-shader-artifacts.sh` cleanly while silently corrupting skinned-vertex output.

**Suggested Fix**: add a test (or extend `check-shader-artifacts.sh`) that cross-checks the GLSL-side constant literals against their Rust counterparts, following the same pattern used for `GpuInstance`/`GpuMaterial` lockstep guards.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
