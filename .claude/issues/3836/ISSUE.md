# #3836: PERF-D1-2026-09-05-03: `scene_has_effect_soft_material` runs an ungated O(all `Material`) + O(all `ParticleEmitter`) scan at the head of every `build_render_data`

Filed from `docs/audits/AUDIT_PERFORMANCE_2026-09-05.md` (PERF-D1-2026-09-05-03) via `/audit-publish`, 2026-09-05.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3836 --json state`.

---

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-09-05.md` (PERF-D1-2026-09-05-03), published from `/audit-suite volumetrics-deep`. Premise re-verified against HEAD at publish time.

> Note: `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/render/mod.rs:34-53`, called unconditionally at the top of `build_render_data`
- **Status**: NEW
- **Description**: Answers a scene-wide `bool` ("does any material or emitter
  carry `EFFECT_SOFT`?") by iterating every `Material` component and, if none
  matched, every `ParticleEmitter` component, every frame. `.any()`
  short-circuits on the *first* match, so the expensive case is the common
  one — a scene with none — which walks the full set to conclude `false`. On
  the FO4 InstituteBioScience baseline (~3,949 draw commands) that is a few
  thousand bitmask tests per frame plus two storage read-lock acquisitions,
  for a value that only changes when content loads/unloads.
- **Evidence**:
```rust
// render/mod.rs:34-53
fn scene_has_effect_soft_material(world: &World) -> bool {
    let mesh_materials_have_soft_effect = world.query::<Material>().is_some_and(|materials| {
        materials.iter().any(|(_, material)| {
            material.effect_shader_flags & ...EFFECT_SOFT != 0
        })
    });
    if mesh_materials_have_soft_effect { return true; }
    world.query::<ParticleEmitter>().is_some_and(|emitters| { ... })
}
```
  Called unconditionally from `build_render_data` before the scratch
  `clear()` block, with no caching and no dirty gate.
- **Impact**: A few tens of microseconds per frame in a dense cell; more
  significantly it's O(scene size) work inside a function whose stated design
  premise is caller-owned amortised scratch, and it scales with exactly what
  exterior streaming grows.
- **Related**: `byroredux/src/systems/bounds.rs:157-173` already demonstrates
  the correct pattern in this codebase (a `structural_generation()` key,
  full recompute only when it moves); #3477/#3475/#3142 are the same
  "rescan-every-tick to answer a rarely-changing question" family.
- **Suggested Fix**: Cache the flag against
  `(Material::structural_generation(), ParticleEmitter::structural_generation())`,
  recomputing only when a material/emitter is added or removed.
- **Confidence**: High.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
