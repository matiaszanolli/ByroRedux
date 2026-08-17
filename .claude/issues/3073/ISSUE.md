# NIFAL-D1-2026-08-16-01: parallax params bypass the canonical Material, six duplicated defaults

**Issue**: #3073
**Severity**: MEDIUM
**Labels**: `medium,nif-parser,renderer,bug`
**Source report**: `docs/audits/AUDIT_NIFAL_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_NIFAL_2026-08-16.md` (Dimension 1 — canonical boundary).

**Location**: `byroredux/src/cell_loader/spawn/mesh_instance.rs`:613-614 and five sibling sites · render-time fallback in the renderer

## Description

`parallax_height_scale` / `parallax_max_passes` **bypass the canonical `Material`**, with the same magic defaults duplicated at six sites plus a render-time fallback.

This is a NIFAL boundary violation in the precise sense the spec names: a material property that should be resolved once at the parser→`Material` boundary is instead re-derived at multiple downstream sites, including at render time.

## Evidence

The canonical home exists and is typed for it:
```rust
// crates/core/src/ecs/components/material.rs:402-403
pub parallax_max_passes: Option<f32>,
pub parallax_height_scale: Option<f32>,
```

But the values are also carried as plain `f32` on the renderer side and defaulted independently:
```rust
// crates/renderer/src/vulkan/context/mod.rs:147,150
pub parallax_height_scale: f32,
pub parallax_max_passes: f32,
// :421-422 — copied forward again
// crates/renderer/src/vulkan/water.rs:805-806 — a third default pair
```

Re-verified 2026-08-17.

## Impact

Six duplicated defaults mean six places to change and six chances to diverge — and the render-time fallback means a material that resolved one value at import can render with another.

Concretely relevant to #2997: FO4 slot-3 palette gradients currently reach `parallaxMapIndex`, and `GpuMaterial::default()`'s `parallax_height_scale = 0.04` is what makes the POM branch unconditionally live. Consolidating the value is part of making that fix legible.

## Suggested Fix

Resolve both values once in `Material::resolve_pbr` (or `translate_material`), store them on the canonical `Material`, and have every consumer read them from there. Delete the render-time fallback — per `docs/engine/nifal.md`, no per-game or per-material logic may be re-derived at render time.

## Related

- **#2997 (FO4-D5-06 — the POM branch this feeds)**
- `docs/engine/nifal.md` (the spec this violates)

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Values resolved once at the parser→`Material` boundary, never re-derived at render time
- [ ] **NO-DUPLICATION**: One default, not six
- [ ] **SIBLING**: `water.rs`'s pair included in the consolidation
- [ ] **SHADER-SYNC**: If `GpuMaterial` changes, the GLSL mirror in `bindings.glsl` moves in lockstep
- [ ] **TESTS**: A regression test asserts one authored value survives to the GPU struct

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3073 --json state` when live state is needed.*
