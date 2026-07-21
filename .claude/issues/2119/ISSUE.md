# REN-D17-01: glass-branch F0 comment says "mat.ior defaults to 1.5 (glass)" — stale after 41eedfe1

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/2119
**Labels**: documentation, renderer, low

**Severity**: LOW
**Dimension**: Disney BSDF
**Location**: `crates/renderer/shaders/triangle.frag`, glass-fragment F0 block (~line 1213, the `glassF0 = vec3(f0Dielectric)` site)
**Status**: NEW (doc-rot introduced by commit `41eedfe1`, same-day)

**Description**: Canonical glass source is now `ior = 1.45` (`GLASS_SURFACE_BEHAVIOR` in `crates/core/src/ecs/components/material.rs`, applied via `classify_glass_into_material`); `1.5` is now the generic-dielectric default (`DEFAULT_DIELECTRIC_IOR`), no longer glass-specific. The comment's `(glass)` parenthetical on `1.5` is inaccurate — a glass fragment reaching this branch carries `mat.ior == 1.45`, not `1.5`.

**Evidence**:
```glsl
// triangle.frag, ~line 1213
// mat.ior defaults to 1.5 (glass); BGSM-authored glass can ...
```
```rust
// crates/core/src/ecs/components/material.rs
pub const GLASS_SURFACE_BEHAVIOR: SurfaceBehavior = SurfaceBehavior { roughness: 0.10, metalness: 0.0, ior: 1.45 };
pub const DEFAULT_DIELECTRIC_IOR: f32 = 1.5;
```

**Impact**: None at runtime (the code reads `mat.ior` regardless of the comment). Risk is a future editor re-hardcoding `1.5` for glass and undoing the 1.45 behavior split.

**Suggested Fix**: Reword to state `mat.ior` is 1.45 for canonical glass (`GLASS_SURFACE_BEHAVIOR`), 1.5 is the generic-dielectric default (`DEFAULT_DIELECTRIC_IOR`), and BGSM-authored glass can diverge from both.

## Completeness Checks
- [ ] **SIBLING**: Check for the same stale "(glass)" framing anywhere else the `1.5`/`DEFAULT_DIELECTRIC_IOR` constant is referenced in shader comments

Filed from `docs/audits/AUDIT_RENDERER_2026-07-20.md`.
