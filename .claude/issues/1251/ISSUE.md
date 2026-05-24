# REN: add documented Disney material preset table (material::presets module)

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1251
## Problem

When ByroRedux synthesises a `GpuMaterial` for a NIF mesh that has no BGSM (most pre-FO4 content), we guess defaults — roughness=0.5, metallic=0.0, F0=0.04. The "no guessing" policy ([feedback_no_guessing](https://github.com/matiaszanolli/ByroRedux/) memory) wants citable references for any physical constant we pick.

We need a documented preset table keyed off the legacy `BSLightingShader` shader type / texture suffix / model heuristic, so when we encounter "polished metal" or "rubber tire" we pick from a known-good set.

## Reference

GLSL-PathTracer `assets/hyperion_rect_lights.scene` (MIT) ships the canonical Disney preset values:

| Material | color | roughness | metallic | other |
|---|---|---|---|---|
| Polished metal (silver) | `0.9 0.9 0.9` | `0.001` | `1.0` | |
| Glass | `1 1 1` | `0.0` | `0.0` | `specTrans 1.0, ior 1.45` |
| Car paint | `0.026 0.147 0.075` | `0.01` | `0.0` | `clearcoat 1.0, clearcoatGloss 1.0` |
| Lacquered plastic (orange) | `1.0 0.186 0.0` | `0.001` | `0.0` | `clearcoat 1.0, clearcoatGloss 1.0` |
| Painted matte (red) | `1.0 0.0 0.0` | `0.5` | `0.2` | mild metal sheen |
| Skin / wax / marble | `0.93 0.89 0.85` | `1.0` | `0.0` | `subsurface 1.0` |

## Proposed change

Add `pub mod presets` to [material.rs](crates/renderer/src/vulkan/material.rs):

```rust
/// Disney-BSDF material presets sourced from
/// knightcrawler25/GLSL-PathTracer (MIT) — `assets/hyperion_rect_lights.scene`.
/// Use as fallback when authored BGSM is absent.
pub mod presets {
    use super::GpuMaterial;
    pub fn polished_metal() -> GpuMaterial { /* ... */ }
    pub fn glass() -> GpuMaterial { /* ... */ }
    pub fn car_paint(base: [f32; 3]) -> GpuMaterial { /* ... */ }
    pub fn lacquered_plastic(base: [f32; 3]) -> GpuMaterial { /* ... */ }
    pub fn painted_matte(base: [f32; 3]) -> GpuMaterial { /* ... */ }
    pub fn skin_wax_marble(base: [f32; 3]) -> GpuMaterial { /* ... */ }
}
```

Each preset is a `pub fn` returning a constructed `GpuMaterial` (not a `const` — `GpuMaterial` is large; this avoids whole-struct literals everywhere).

## Why now

- Satisfies the no-guessing policy with a citable source.
- Sets up the legacy-NIF synthetic-material path with documented values instead of "0.5 / 0.0 / 0.04 because" magic numbers.
- Reusable in tests as known-good fixtures.

## Dependencies

Cleanest after the F0-from-IOR and Disney-diffuse issues land, since `glass()` will want to set `ior = 1.45, spec_trans = 1.0` and `skin_wax_marble()` will want `subsurface = 1.0`. Without those fields the preset table is incomplete.

## Credit

Reference data: knightcrawler25/GLSL-PathTracer (MIT). Attribution in module header.