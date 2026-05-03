**Severity**: MEDIUM
**Dimension**: Material Table (R1)
**Source**: AUDIT_RENDERER_2026-05-01.md
**Related**: #776 (R1-N1 demonstrates this class of regression is non-hypothetical)

## Locations
- [crates/renderer/src/vulkan/scene_buffer.rs:172-216](../../tree/main/crates/renderer/src/vulkan/scene_buffer.rs#L172-L216) — struct fields + their docstrings explaining the retention
- [crates/renderer/shaders/caustic_splat.comp:153-155](../../tree/main/crates/renderer/shaders/caustic_splat.comp#L153-L155) — consumer of `avg_albedo`
- [crates/renderer/src/vulkan/scene_buffer.rs:1402-1418](../../tree/main/crates/renderer/src/vulkan/scene_buffer.rs#L1402-L1418) — offset_of! tests pin layout but say nothing about *why* fields are retained

## Description

Phase 6 (commit `22f294a`) collapsed ~30 per-material fields off `GpuInstance` (400 B → 112 B), but explicitly retained two:

- `texture_index` — UI exception. The UI quad path appends an instance with a per-frame texture handle without going through the material table.
- `avg_albedo[rgb]` — caustic-compute exception. `caustic_splat.comp` reads it from descriptor set 0 / binding 5 without a `MaterialBuffer` binding; migrating the caustic compute pipeline was deferred to a follow-up R1 cleanup.

Both retentions are documented in field docstrings but **not enforced by any test or build check**. A future refactor sweep that re-migrates these fields without simultaneously fixing the consumer paths would silently break the UI (in the texture_index case — exactly what R1-N1 / #776 demonstrates) or the caustic compute (in the avg_albedo case).

## Impact

Silent regression risk on the next R1-style sweep. R1-N1 is the same class of bug, already exercised this audit cycle.

## Suggested Fix

Add two targeted invariant checks:

1. **Build-time grep test** that asserts `ui.vert` contains `inst.textureIndex` (mirroring the existing `gpu_material_size_is_272_bytes` style):
   ```rust
   #[test]
   fn ui_vert_reads_per_instance_texture_index() {
       let src = include_str!(\"../../shaders/ui.vert\");
       assert!(
           src.contains(\"fragTexIndex = inst.textureIndex\"),
           \"ui.vert must read texture_index from the per-instance struct, \
            not from materials[…] — UI quad path doesn't intern a material. \
            See R1-N1 / #776.\"
       );
   }
   ```

2. **Comment-level cross-link** in `caustic_splat.comp` asserting `avg_albedo` is read from `instances[]` not `materials[]` until the caustic pipeline gets its own `MaterialBuffer` binding.

Alternatively, file a `R1-cleanup` follow-up that fully migrates both consumers (intern a UI material, add MaterialBuffer to caustic descriptor set 0) so the per-instance fields can drop on the next pass — but this issue specifically tracks the prevention layer for the deferred-state.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check whether other shaders (svgf_temporal.comp, taa.comp, composite.frag) read any per-instance fields that should be migrated; document those as additional retention exceptions if found
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: The fix IS the test. Add to `crates/renderer/src/vulkan/material.rs` test module so the existing `gpu_material_size_is_272_bytes` neighbour catches it.
