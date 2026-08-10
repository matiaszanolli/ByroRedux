# Physical lighting backbone

This document defines the runtime contract behind playable, stable lighting.
Legacy records are inputs to a translator, not renderer semantics.

## Runtime contract

1. **Units.** Runtime distances and medium coefficients use `Meters` and
   `ExtinctionPerMeter`. Bethesda coordinates remain in world units only where
   the existing transform/TLAS ABI requires them; conversion uses the single
   `BETHESDA_UNITS_PER_METER` constant in `byroredux_core::lighting`.
2. **Emitters.** `Emitter` owns geometry, scene-linear radiant intensity,
   physical source radius, range, distance law, and visibility. `LightSource`
   adds animation controls and keeps source flags only for diagnostics.
3. **Legacy boundary.** NIF/LIGH fields are resolved by
   `LightSource::from_legacy_world_units`. Shaders never inspect legacy shadow
   flags or infer an emitter type from source-format data.
4. **Visibility.** CPU emitters and Vulkan TLAS instances share six explicit
   bits: architecture, static props, dynamic actors, foliage, glass, effects.
   A light uploads a union of bits; ray queries consume that union directly.
5. **Transport.** Surface, GI, water, caustic, and froxel passes use the same
   visibility vocabulary. Local surface/froxel attenuation selects either the
   bounded legacy curve or inverse-square metres with finite-source softening.
6. **Ray allocation.** `AdaptiveRayBudget` observes the slower of retired
   main-lighting and volumetric GPU timestamps (the upper-bound brackets are
   not summed). Four hysteretic tiers jointly bound glass/refraction claims,
   direct shadow samples, GI path depth/shaded hits, and froxel local lights.
   Downgrades react to overload; upgrades require 45 stable headroom frames.
7. **Temporal reconstruction.** Volumetrics default to one froxel per 8×8
   render pixels. World reprojection, density rejection, history-neighborhood
   variance clipping, and emission-weighted disagreement rejection prevent
   crawling noise and long fire trails. Local fire turbulence is independent
   of weather coverage and advects at a slower source-relative rate.
8. **Reference laboratory.** Deterministic tests pin unit conversion, layer
   independence, a candle-room legacy visibility case, inverse-square flux,
   quality-controller hysteresis, default froxel resolution, shader layouts,
   and committed SPIR-V reproducibility.

## GPU light ABI

`GpuLight.params` has one meaning in every pass:

| Lane | Meaning |
| --- | --- |
| x | Legacy falloff exponent |
| y | Luminous source radius in Bethesda world units |
| z | Explicit visibility-mask bits encoded as an exact `f32` integer |
| w | `AttenuationModel` discriminant encoded as `f32` |

`GpuRayBudget` occupies scene descriptor set 1, binding 11. Its first word is
the atomic glass-ray counter; the remaining words are immutable frame limits.

## Validation gates

```bash
cargo test -p byroredux-core lighting::tests --lib
cargo test -p byroredux-renderer --lib
cargo test -p byroredux --no-run
scripts/check-shader-artifacts.sh
```

The visual reference is a small enclosed room containing a candle, opaque
clutter, an actor proxy, cutout foliage, and a glass pane. Captures should pin
near/far illumination ratios, blocker-category behavior, penumbra width, GPU
time, selected quality tier, and temporal settling after a camera cut.
