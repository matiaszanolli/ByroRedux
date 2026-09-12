# Issues 4023, 4028, 4030, 4041

All four from `docs/audits/AUDIT_RENDERER_2026-09-06.md` (23-dimension `/audit-renderer` sweep at `229306ce`).

## #4023 — REN-2026-09-06-D21-01 (LOW, Cornell Harness)
`mat.set`'s field table has no arm for the four shader-consumed `glass_*` scalars
that define glass appearance: `glass_fresnel_color` → `GpuMaterial.glass_fresnel_{r,g,b}`,
`glass_refraction_scale`, `glass_blur_scale`, `glass_blur_scale_factor`. All four
are assigned verbatim in `to_gpu_material` and read by the shader, but
`cornell.rs`'s `glass()` constructor leaves them at `Material::default()` with no
console path to sweep them.
Fix: add `glass_refraction_scale`, `glass_blur_scale`, `glass_blur_scale_factor`
(scalar, `ior` precedent: no clamp, finite check) and `glass_fresnel_color` (vec3)
arms to `MatSetCommand`, extend `USAGE`.

## #4028 — REN-2026-09-06-D3-04 (LOW, GPU-Struct Layout)
`CameraUBO` is the one mirrored GPU struct #3564's `assert_mirror_list_is_complete`
mechanism doesn't cover, because `shader_sources_declaring` requires `decl` to
start the trimmed line and `CameraUBO` is always preceded by `layout(...)`.
No live drift today (5 sites, matches SOURCES) — this is a guard gap, not a bug.
Fix: give `shader_sources_declaring` an optional "may be preceded by a
`layout(...)` qualifier" mode, call
`assert_mirror_list_is_complete("uniform CameraUBO {", SOURCES, "#3684")` in
`camera_ubo_glsl_copies_stay_in_lockstep`. Strict behavior must stay default.

## #4030 — REN-2026-09-06-D3-06 (LOW, GPU-Struct Layout)
`gpu_instance_layout_tests.rs`'s `max_instances_stays_within_mesh_id_encoding_ceiling`
still declares a local `const MESH_ID_ENCODING_CEILING: usize = 0x7FFF_FFFF;`
instead of using `shader_constants_data.rs`'s `MESH_ID_STABLE_MASK` (added by
`bf8ded3d`/#3881, defined as `!MESH_ID_NO_HISTORY_BIT`). One residual literal
copy of a value that's supposed to have one source of truth.
Fix: replace the local const with `crate::shader_constants::MESH_ID_STABLE_MASK as usize`.

## #4041 — REN-2026-09-06-D6-01 (LOW, NIFAL Material doc-rot)
`.claude/commands/audit-renderer/SKILL.md` Dimension 6's "Single boundary" bullet
(reworded by #3904's fix, `2853464f`) names three `translate_material` callers,
two of which are wrong: `cell_loader/spawn.rs` doesn't exist as a caller anymore
(moved to `cell_loader/spawn/mesh_instance.rs`), `cell_loader/placement_lod.rs`
(the exterior placement-LOD spawner) is omitted entirely, and `cornell.rs` is
listed as if production, when its `translate_material` call is inside
`#[cfg(test)] mod tests`.
Fix: replace the caller enumeration with the invariant + its guard test
(`every_exterior_spawner_inserts_a_boundary_material` in
`byroredux/src/material_translate.rs`), name the Cornell harness as the one
documented exemption (its production half constructs `Material` literals
directly and that's legitimate, per #2477/#2514).

## Domain
Renderer (`byroredux-renderer` for #4023/#4028/#4030 — MatSetCommand actually
lives in `byroredux/src/commands/scene.rs`, binary crate `byroredux`, but the
GpuMaterial/shader-contract pieces are `byroredux-renderer`). #4041 is a
docs-only fix to an audit skill file, no crate.
