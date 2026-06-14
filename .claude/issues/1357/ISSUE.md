# D7-04: BGSM_* material flag alias constants not yet migrated to canonical names (PBR_BSDF, TRANSLUCENCY, etc.)

**Severity**: LOW · **Source**: AUDIT_FO4_2026-05-30 (D7-04)

**Location**: `crates/renderer/src/vulkan/material.rs:483-487`

**Description**: Five `BGSM_*` alias constants (`BGSM_PBR`, `BGSM_TRANSLUCENCY`, `BGSM_MODEL_SPACE_NORMALS`, `BGSM_TRANSLUCENCY_THICK_OBJECT`, `BGSM_TRANSLUCENCY_MIX_ALBEDO`) are documented as "Pre-Stage-3 aliases — kept so external callers compile" but `pack_bgsm_material_flags` in `byroredux/src/cell_loader.rs` still uses them. The migration to canonical names (`PBR_BSDF`, `TRANSLUCENCY`, etc.) has not happened.

**Suggested Fix**: Update the `use` block in `cell_loader.rs::pack_bgsm_material_flags` to reference the canonical constants directly, then remove the alias block from `material.rs:483-487`.
