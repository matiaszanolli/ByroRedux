# Procedural volumetric fog

## Decision record

**Status:** first production slice landed (froxel core, physical single
scattering, temporal history, RT visibility, FSR contract).

**Location:** `crates/renderer/src/vulkan/volumetrics.rs`,
`crates/renderer/shaders/volumetrics_{inject,integrate}.comp`,
`crates/renderer/shaders/composite.frag`.

**Decision:** fog and light shafts are one participating-medium calculation.
The inject pass evaluates procedural extinction and light visibility in a
view-aligned froxel grid; the integrate pass accumulates radiance and
transmittance once along Z; composite consumes one 3D sample. Directional and
local visibility queries run against the scene TLAS, whose instances reference
the renderer's static and skinned BLAS.

**Why:** baked billboard fog and screen-space radial shafts do not share depth,
visibility, motion, or FSR history with the scene. A froxel V-buffer gives all
of them one physical and temporal contract.

**References:**

- Bart Wronski, [Volumetric Fog: Unified Compute Shader Based Solution to
  Atmospheric Scattering](https://bartwronski.com/wp-content/uploads/2014/08/bwronski_volumetric_fog_siggraph2014.pdf)
- Sébastien Hillaire, [A Scalable and Production Ready Sky and Atmosphere
  Rendering Technique](https://sebh.github.io/publications/egsr2020.pdf)

## Runtime contract

- The grid derives from the render extent after the FSR preset query, never
  from output resolution.
- Defaults are one froxel per 12×12 render pixels, 64 Z slices, and a 128 m
  grid far plane.
- The first 5 m consume the first 1/8 of Z linearly. The remaining slices are
  exponential from 5 m to the configured far plane.
- The raw V-buffer stores `(in-scattered radiance.rgb, sigma_t)`. The
  integrated volume stores `(accumulated radiance.rgb, transmittance)`.
- One jittered sample is evaluated per froxel. Previous raw V-buffer history
  is reprojected with the previous camera matrix and blended at 0.92 steady
  state. Relative extinction changes exponentially reduce that weight.
- The medium uses extinction, single-scatter albedo, and a dual-lobe
  Henyey-Greenstein phase function (`g_forward=0.8`, `g_backward=-0.3`,
  forward mix `0.7`).
- Beyond the grid, geometry uses analytic exponential height fog. Horizontal
  rays take a separate constant-density branch to avoid the `0/0` limit.
- Dense fog MAX-blends into FSR's reactive and transparency/composition masks.
  Dither is applied in linear HDR before FSR, and the volumetric history resets
  through the same temporal-discontinuity dispatcher as TAA/FSR.
- When TLAS or clustered-light inputs are temporarily unavailable, the current
  integrated slot is cleared to neutral instead of retaining a previous
  cell's fog.

## Configuration

```text
--froxel-xy-divisor <4..32>   default 12
--froxel-z-slices <16..256>   default 64
--fog-grid-far-m <32..512>    default 128
```

Example:

```bash
cargo run --release -- --game fnv --cell GSProspectorSaloonInterior \
  --froxel-xy-divisor 12 --froxel-z-slices 64 --fog-grid-far-m 128
```

## Measurement table

Keep rows even when a path is not implemented; `—` means no data rather than a
fabricated zero. The initial smoke used an RTX 4070 Ti, driver 580.173.02,
1280×720 output with FSR Quality, FNV Prospector Saloon, after pipeline warmup.
The timer brackets inject plus integrate.

| Dimension | Value | Froxel extent | Volumetrics GPU | Status / evidence |
|---|---:|---:|---:|---|
| XY divisor | 8 | 107×60×64 | — | allocation/dispatch smoke passed; timed warmup pending |
| XY divisor | 12 | 72×40×64 | 0.12–0.13 ms | repeated measured frames; Vulkan smoke passed |
| XY divisor | 16 | 54×30×64 | — | pending |
| Z slices | 32 | 72×40×32 | — | pending |
| Z slices | 64 | 72×40×64 | 0.12–0.13 ms | default |
| Z slices | 128 | 72×40×128 | — | pending |
| Samples/froxel | 1 | 72×40×64 | 0.12–0.13 ms | temporal reprojection enabled |
| Samples/froxel | 4 | — | — | follow-up quality mode |
| Directional visibility | RT, 1 ray | 72×40×64 | included above | TLAS/BLAS path |
| Directional visibility | cascade, 1 tap | — | — | follow-up non-RT path |
| Procedural octaves | 3 ALU | 72×40×64 | included above | current default |
| Detail-volume octaves | 0 / 2 / 3 | — | — | boot-generated R8 volumes follow-up |
| Aerial LUT | off | 72×40×64 | included above | analytic fallback active |
| Aerial LUT | on | — | — | 32³ LUT follow-up |

Target ranges for the reference 160×90×64 grid remain 0.2–0.5 ms inject and
0.3–0.8 ms integrate. Record inject and integrate separately before treating
the current combined timer as a final budget verdict.

## Follow-up boundary

The next slice is data conversion, not another rendering rewrite:

1. fit engine-native extinction values from XCLL/WTHR curves and inspect FO4
   VOLI records;
2. replace detected fog billboards/particle emitters with clustered volume
   primitives;
3. generate tileable Perlin-Worley/detail volumes and drive Nubis coverage from
   weather;
4. add the 32³ aerial-perspective LUT and a non-RT cascade visibility variant;
5. extend the existing glass transmittance hook with ratio tracking and a
   majorant grid for path-traced heterogeneous media.
