# #3592 — REN-2026-08-30-D7-02: two live comments on the dedup path attribute `GpuMaterial` variant slots to `GpuInstance::default`, the struct R1 Phase 6 moved them off

**Labels**: `low,renderer,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3592 --json state`.

---

- **Severity**: LOW
- **Dimension**: Material Table
- **Location**: `byroredux/src/render/static_meshes.rs:534` and `:574` (`collect_static_mesh_draws`)
- **Status**: OPEN
- **Description**: The Skyrim+ `BSLightingShaderProperty` variant-payload block justifies its `material_kind`-gated zero fallbacks with "`GpuInstance::default` already zeroes the slots" (`:534`) and "the slot stays zeroed exactly as `GpuInstance::default` leaves it" (`:574`, added in the `#2602` hair-tint change in this sweep's delta). `GpuInstance` carries no such fields. Its full field list is `model, texture_index, bone_offset, vertex_offset, index_offset, vertex_count, flags, material_id, ior, avg_albedo_r/g/b, surface_id, skinned_vertex_address, _reserved, morph_delta_address, morph_weight_address, morph_target_count, _reserved2a/b/c` (`scene_buffer/gpu_types.rs:95`). `skin_tint_*`, `hair_tint_*`, `sparkle_*`, `eye_*` and `multi_layer_*` are all `GpuMaterial` fields (`material.rs:141-176`) with zero defaults in `GpuMaterial::default()` (`material.rs:433-435` for `hair_tint_*`); they were collapsed onto the material table by R1 Phase 6 and are explicitly named on the ban list in `gpu_instance_does_not_re_expand_with_per_material_fields` (`gpu_instance_layout_tests.rs:180`).
- **Evidence**: `grep -n "GpuInstance::default" byroredux/src/render/static_meshes.rs` → lines 534, 574. `grep -n "hair_tint" crates/renderer/src/vulkan/material.rs` → declared at 153-155, defaulted at 433-435, hashed at 1068-1070. No `hair_tint` / `skin_tint` / `sparkle` / `eye_` identifier exists in `pub struct GpuInstance`.
- **Impact**: Documentation only — the code is correct (the zeroes it writes are the `GpuMaterial::default()` values, so the neutral-output claim holds). But the comment is a per-instance/per-material attribution error sitting on the exact function that decides what goes into the dedup key, in the same file that already carries a `#3465` note about naming call sites by symbol. It invites a reader to look for the fallback on the wrong struct, or to conclude these are per-instance slots that could be re-widened.
- **Suggested Fix**: Change both references to `GpuMaterial::default` and, at `:534`, note that the material-table record — not the instance record — is what carries the variant payload after R1 Phase 6.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D7-02

## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
