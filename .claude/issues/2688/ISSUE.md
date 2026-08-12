# SAFE-D6-01: GLSL-Rust GpuMaterial lockstep never pins the GLSL scalar type

**Issue**: #2688
**Filed**: 2026-08-12 via `/audit-publish` from `/audit-suite renderer-deep`

- **Severity**: LOW
- **Dimension**: 6 — R1 Material Table Layout Soundness
- **Location**: [gpu_instance_layout_tests.rs](crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs) (`parse_glsl_struct_fields`, `gpu_material_glsl_field_order_matches_rust_struct`) · [material.rs](crates/renderer/src/vulkan/material.rs) (`gpu_material_glsl_field_names_pinned`)
- **Status**: NEW
- **Description**: Three guards cover the contract:
  `gpu_material_size_is_348_bytes` (size),
  `gpu_material_field_offsets_match_shader_contract` (Rust offsets — verified
  complete, see PASS list), and
  `gpu_material_glsl_field_order_matches_rust_struct` (GLSL declaration order).
  `parse_glsl_struct_fields` reads the type token only to decide whether a line
  is a field declaration and then **discards it**, and the name-needle test
  matches bare identifiers (`"materialKind;"`), so a `uint ↔ float` flip inside
  `bindings.glsl` is invisible to `cargo test` while being byte-lethal for any
  field consumed via an implicit widening read.
- **Evidence**: `parse_glsl_struct_fields` pushes `id.to_string()` only; the
  needle list in `material.rs` is names-with-punctuation, no types. I verified
  by hand that all 87 GLSL/Rust type pairs match **today** (`float`↔`f32`,
  `uint`↔`u32`) — this is a missing guard, not a live drift.
- **Impact**: Bounded — most type flips (bindless index used as an array
  subscript, `materialKind` compared against a `uint` constant) fail glslang
  compilation, so the realistic residual is fields read only through an implicit
  `uint→float` widening. Defense-in-depth gap in a HIGH-severity-class contract,
  cheap to close.
- **Related**: #1657 / SF-D8-01 (added the order guard), #806.
- **Suggested Fix**: Have `parse_glsl_struct_fields` return `(type, name)` pairs
  and assert `float↔f32` / `uint↔u32` alongside the existing order comparison.

---


---
*Filed from [`docs/audits/AUDIT_SAFETY_2026-08-12.md`](docs/audits/AUDIT_SAFETY_2026-08-12.md) — `/audit-suite renderer-deep`, 2026-08-12. Finding ID `SAFE-D6-01`.*

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
