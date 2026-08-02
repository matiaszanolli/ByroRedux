# REN-D6-01: Fire-refraction's ior field overload and 8 hand-translated material fields are undocumented / outside the NIFAL boundary

Severity: medium
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2232

**Dimension**: 6 (Material/NIFAL boundary)
**Location**: `crates/core/src/ecs/components/material.rs:26` (`SurfaceBehavior::ior`) and `crates/renderer/shaders/include/bindings.glsl:38` (`GpuMaterial.ior`, "per-draw optical IOR"); `byroredux/src/material_translate.rs` and the NIF import material walker (both NIF load sites doing hand-translation outside the NIFAL boundary)
**Status**: NEW

**Description**: `GpuMaterial.ior` already has two documented meanings (ordinary Fresnel IOR at `bindings.glsl:38`, glass IOR via `SurfaceBehavior` at `material.rs:26`) but a third: for `MATERIAL_KIND_FIRE_REFRACTION`, `triangle.frag` reads `mat.ior` as `distortionStrength = clamp(mat.ior, 0.0, 1.0)` — a 0-1 distortion scalar, not a refractive index — and this third meaning is documented nowhere on the canonical type. Separately (pre-existing, not new this session): 8 raw material-decision fields are still hand-translated at both NIF load sites instead of going through the NIFAL parser→`Material` boundary.

**Evidence**: `triangle.frag` fire-refraction branch: `float distortionStrength = clamp(mat.ior, 0.0, 1.0);` — no accompanying doc update at either `bindings.glsl:38` or `material.rs:26` noting this third, incompatible-range meaning.

**Impact**: A future reader of `GpuMaterial.ior`'s doc comment would reasonably assume it's always a physical IOR (~1.0-2.5) and could misuse or "fix" the fire-refraction 0-1 range as a bug. The pre-existing hand-translation gap perpetuates the exact NIFAL-boundary violation pattern the abstraction layer exists to prevent (see `/audit-nifal`).

**Suggested Fix**: add a doc note at both `ior` field sites listing all three discriminated meanings by `materialKind`. Separately, enumerate and migrate the 8 hand-translated fields into the canonical `translate_material` path as NIFAL-boundary follow-up work.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
