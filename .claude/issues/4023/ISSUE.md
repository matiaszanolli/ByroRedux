# #4023 — REN-2026-09-06-D21-01: the four `glass_*` `GpuMaterial` scalars are the harness's only shader-consumed material lanes with no `mat.set` arm

**Labels**: low, renderer, shaders, test-gap, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D21-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: Cornell Harness
- **Location**: `byroredux/src/commands/scene.rs` (`MatSetCommand::execute`), `byroredux/src/cornell.rs` (`glass`), `crates/renderer/src/vulkan/context/mod.rs` (`to_gpu_material`)
- **Status**: NEW
- **Description**: `mat.set`'s field table has been extended three times
  specifically to close "the Cornell harness cannot reach this live" gaps —
  #2477 (`material_flags`), #2514 (`subsurface`/`sheen`/`sheen_tint`/
  `anisotropic`), #2823 (the three translucency lanes). Four shader-consumed
  scalars remain unreachable, and they are precisely the ones that define the
  glass appearance the harness exists to bisect:

  - `glass_fresnel_color` → `GpuMaterial.glass_fresnel_{r,g,b}`
  - `glass_refraction_scale` → `GpuMaterial.glass_refraction_scale`
  - `glass_blur_scale` → `GpuMaterial.glass_blur_scale`
  - `glass_blur_scale_factor` → `GpuMaterial.glass_blur_scale_factor`

  All four are assigned verbatim in `to_gpu_material` and read by the shader
  (`DEFAULT_GLASS_REFRACTION_SCALE` / `DEFAULT_GLASS_BLUR_SCALE` in
  `crates/core/src/ecs/components/material.rs` are emitted as GLSL macros the
  shader divides by — see #3459). `cornell.rs`'s `glass()` constructor sets only
  `diffuse_color`, `material_kind`, `alpha`, and `GLASS_SURFACE_BEHAVIOR`
  (`roughness 0.10 / metalness 0.0 / ior 1.45`), leaving all four at their
  `Material::default()` values with no console path to sweep them.

  Note this is *not* a re-report of the glass-stipple / IGN refraction jitter
  observation — it is the reason that observation cannot be A/B'd from the
  harness in the first place.
- **Evidence**: `MatSetCommand::execute`'s `match field.to_ascii_lowercase()`
  arms cover metalness, roughness, alpha, glossiness, emissive_mult,
  specular_strength, env_map_scale, ior, subsurface, sheen, sheen_tint,
  anisotropic, the two translucency scalars, four colour lanes, `material_kind`
  and `material_flags` — no `glass_*` arm, and none of the four appears in the
  `USAGE` string. `to_gpu_material` assigns all four.
- **Impact**: Harness capability gap. Any glass-path bisect requires editing
  `cornell.rs` and rebuilding, which is the workflow #2477/#2514/#2823 each
  concluded was too slow to be used in practice.
- **Related**: #2477, #2514, #2823, #3459 (the shader/host constant sync for two
  of these four), `docs/engine/nifal.md` (BGEM v21+ glass authoring).
- **Suggested Fix**: Add `glass_refraction_scale`, `glass_blur_scale`,
  `glass_blur_scale_factor` (scalar) and `glass_fresnel_color` (vec3) arms to
  `MatSetCommand`, following the existing `ior` precedent of no range clamp with
  the `floats` finite check as the guard, and extend `USAGE`.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
