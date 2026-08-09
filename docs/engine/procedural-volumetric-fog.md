# Procedural volumetric fog and emissive media

## Decision record

**Status:** froxel core plus authored global/local conversion landed (physical
spectral single scattering, temporal history, RT visibility, FSR contract,
XCLL/WTHR → engine-native medium, smoke/fog particles → clustered local
primitives, boot-generated tileable Perlin-Worley base/detail density,
weather-classified coverage). Emissive media (fire) landed as an emission
source term on the same primitives plus derived light sources; a voxel fire
simulation is the open follow-up.

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
- XCLL/WTHR near/far ramps are converted once at the cell/weather translation
  boundary. Runtime volumetrics consume extinction in inverse metres,
  single-scatter albedo, and coverage; they never evaluate the legacy ramp.
- The authored linear fog colour is normalized by its strongest channel and
  multiplied into the global single-scatter albedo. This preserves the legacy
  hue without changing extinction or raising the medium's peak scattering
  energy; authored black remains a finite, purely absorptive medium.
- The extinction fit minimizes
  `Σ (exp(-sigma_t*d)-T_legacy(d))²/d²` over the authored interval. Skyrim's
  authored maximum opacity is included in the target transmittance. Invalid,
  degenerate, or explicitly zero-opacity ramps produce a disabled medium.
- WTHR stores separate physical day/night media. Time-of-day and weather
  transitions interpolate those canonical coefficients directly.
- WTHR classification drives Nubis-style occupancy: pleasant 0.40,
  unclassified 0.55, cloudy 0.70, snow 0.80, and rainy 0.86. Precipitation
  takes priority for combined flags; ordinary weather-transition interpolation
  blends coverage without a shader-side game-format branch.
- The injector samples a deterministic 64³ R8 base volume (three tileable
  Perlin octaves plus Worley billows) and a 32³ R8 erosion volume (two Worley
  octaves plus Perlin detail). Both are generated once at renderer boot,
  uploaded through staging, then sampled with trilinear repeat. Total resident
  density data is 288 KiB.
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
- Alpha-over particle systems whose host or texture identifies fog, smoke,
  mist, steam, vapor, cloud, or dust are replaced at the NIF→ECS boundary.
  Additive flame/ember/magic particles remain billboards.
- Particle preset selection inspects the sprite texture as well as the host
  node. This covers generic Bethesda hosts such as `SuperSpray01-Emitter`
  whose only smoke intent is `fxsmokewispsthin01.dds`. When the retained
  color-curve endpoints are both transparent, conversion uses a conservative
  0.35 alpha proxy rather than collapsing the authored medium to vacuum; a
  decoded texture-average alpha remains the higher-fidelity follow-up.
- Cell-placed alpha-over fog/smoke/mist/steam/vapor/cloud/dust meshes are
  intercepted before texture upload, raster entity creation, or BLAS build.
  Their geometry AABB is extruded along thin axes into a soft box primitive;
  ordinary alpha surfaces remain unchanged. Legacy baked `fxlightrays` meshes
  remain suppressed because shafts emerge from medium scattering plus
  BLAS/TLAS visibility.
- Particle alpha seeds physical extinction through Beer-Lambert after expected
  live-particle occupancy is applied. Emitter shape, lifetime, velocity,
  cone, gravity, particle size, placement rotation, and placement scale define
  an ellipsoid with a soft radial density profile.
- Local primitives are transformed to absolute world space, frustum/distance
  culled, and assigned to a camera-centered 16×16×16 world-space grid with at
  most eight near-sorted volume references per cluster. Each froxel evaluates
  only its cluster list. Their directional and local-light visibility uses the
  same scene TLAS/BLAS ray queries as atmospheric fog.

## Emissive media (fire)

Fire is not a separate subsystem. A flame and its smoke are one physical
material — soot — distinguished only by temperature, so both are `FogVolume`
primitives and differ only in their coefficients:

| | extinction | single-scatter albedo | emission `L_e` |
|---|---|---|---|
| flame | high | **0.25** (soot absorbs) | blackbody at its temperature |
| smoke | moderate | **0.9** (cooled soot scatters) | none |

- The froxel emission source term is `sigma_a * L_e` with
  `sigma_a = sigma_t * (1 - albedo)`, evaluated per channel against the
  *locally sampled* density, so emission inherits the primitive's procedural
  structure instead of filling it uniformly. It is added unconditionally —
  independent of the phase function and of the shadow ray — because a flame
  radiates from being hot, not from being lit.
- No new render pass, buffer, or sort order. `volumetrics_integrate.comp`
  already treats the injection buffer's rgb as radiance added per unit length
  and multiplies by slab thickness and accumulated transmittance, which *is*
  the emission integral. Fire therefore composites, denoises, reprojects, and
  meets the FSR contract through the machinery fog already uses.
- Emission colour and brightness both derive from one temperature via
  `byroredux_core::radiometry`: chromaticity from Planck's law integrated
  against the CIE 1931 observer, magnitude from the visible-band `Y` integral.
  The magnitude law is deliberately **not** Stefan-Boltzmann `T^4`, which
  describes total exitance — mostly infrared at flame temperatures — and would
  understate how sharply a cooling flame dims. Note that across the whole
  flame/ember range the blue primary is outside the sRGB gamut and clamps to
  zero, so hue comparisons in that range must use green-over-red.
- Temperatures are physical properties of the combustion regime, not look
  development: 1850 K for a hydrocarbon diffusion flame's luminous zone,
  1100 K for embers and charcoal. Optical depth is pinned across the primitive
  width rather than inverted from authored alpha, because an additive
  emitter's alpha encodes brightness, not opacity — this also keeps a bonfire
  exactly as translucent as a candle.
- Additive emitters named for flame/ember are replaced at the NIF→ECS
  boundary, the same way alpha-over smoke already is. Magic sparkles stay
  billboards: their colour comes from an authored palette, and inventing a
  temperature for them would fabricate physics the content never described.
  Set `BYRO_FIRE_VOLUMES=0` to keep thermal emitters on the billboard path for
  an A/B.
- An emissive volume also derives a point light, so it illuminates surfaces
  rather than only glowing. Emergent radiance uses the exact slab solution
  `L_e * (1 - exp(-tau_a))` (it saturates at `L_e` instead of growing with
  size), radiant intensity is that times projected area, and reach is
  inverse-square down to a cutoff irradiance. A fire whose extent already
  contains an authored LIGH is suppressed, so derived lights are additive only
  where the original engine had nothing — which is also what makes the
  explosion path viable, since transient fireballs cannot have hand-placed
  lights.
- Emissive froxels use a shorter temporal history weight
  (`DEFAULT_EMISSIVE_HISTORY_WEIGHT`, `fog_reference.y`) blended in by the
  emissive fraction of the source term. This is deliberately not a rejection
  on radiance delta: the sun visibility test is a single jittered *binary*
  sample that legitimately flips at shadow boundaries, so rejecting on that
  delta would suppress accumulation exactly at the god-ray edges M-LIGHT v2
  added it to clean up.

### Open calibration

`FLAME_REFERENCE_RADIANCE` is the one exposure choice in the chain; everything
else is physics. It is derived rather than eyeballed — the path integral
reduces to `optical_depth * (1 - albedo) * L_e`, about `0.3 * L_e`,
independent of flame size — and the resulting torch reach lands within a
factor of two of vanilla authored torch LIGH radii, which is a cross-check on
the whole chain rather than an input to it. It still wants a visual A/B
against real content before being treated as final.

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
| XY divisor | 12 | 72×40×64 | 0.10–0.11 ms | Perlin-Worley/detail volumes; repeated FNV warm frames |
| XY divisor | 16 | 54×30×64 | — | pending |
| Z slices | 32 | 72×40×32 | — | pending |
| Z slices | 64 | 72×40×64 | 0.10–0.11 ms | default |
| Z slices | 128 | 72×40×128 | — | pending |
| Samples/froxel | 1 | 72×40×64 | 0.10–0.11 ms | temporal reprojection enabled |
| Samples/froxel | 4 | — | — | follow-up quality mode |
| Directional visibility | RT, 1 ray | 72×40×64 | included above | TLAS/BLAS path |
| Directional visibility | cascade, 1 tap | — | — | follow-up non-RT path |
| Base density | 64³ R8, 3 Perlin + 1 Worley | 72×40×64 | included above | 256 KiB, deterministic and tileable |
| Detail erosion | 32³ R8, 2 Worley + 1 Perlin | 72×40×64 | included above | 32 KiB, deterministic and tileable |
| Aerial LUT | off | 72×40×64 | included above | analytic fallback active |
| Aerial LUT | on | — | — | 32³ LUT follow-up |

The 2026-07-29 Prospector rotation matrix measured 0.100–0.106 ms combined
inject+integrate after the texture-volume change, versus 0.110–0.125 ms for
the prior three-octave ALU field. Shipping rotation mode 1 moved from 0.116 to
0.105 ms; the four-mode mean fell from 0.1163 to 0.1035 ms (about 11%).

Target ranges for the reference 160×90×64 grid remain 0.2–0.5 ms inject and
0.3–0.8 ms integrate. Record inject and integrate separately before treating
the current combined timer as a final budget verdict.

## Corpus findings

- The canonical XCLL size map remains game-dispatched and corpus-verified:
  Skyrim uses the 92-byte layout with fog power/max; Starfield, not Skyrim,
  owns the distinct 108-byte height-fog block with near/far height
  midpoint/range fields.
- Skyrim WTHR's 32-byte FNAM now retains all eight floats: day/night near,
  far, power, and maximum opacity.
- Fallout 4's extended WTHR layout is not the FO3/FNV stride. Its canonical
  608-byte NAM0 is 19 color groups by 8 time-of-day slots; the parser now
  walks that stride without bleeding late-TOD colors into the next group.
  Its 72-byte FNAM retains all 18
  [xEdit-defined fields](https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.5/Core/wbDefinitionsFO4.pas#L15106-L15125),
  including the ten form-version 119/120 near/far height and high-density
  values. The current homogeneous fit consumes distance and maximum opacity;
  power and height are preserved for the authored height-profile slice rather
  than guessed into the wrong curve.
- A raw recursive survey found zero `VOLI` records in `Fallout4.esm` and all
  six official Fallout 4 DLC masters (`DLCRobot`, `DLCCoast`, `DLCNukaWorld`,
  and the three workshop masters). VOLI therefore cannot calibrate Fallout 4
  defaults from this corpus. The reusable probe lives at
  `crates/plugin/examples/dump_voli_subs.rs`.
- FNV `FreesideAtomicWrangler` contains two alpha-over
  `fxsmokewispsthin01.dds` particle emitters under generic
  `SuperSpray01-Emitter` hosts. Both become `FogVolume` entities in the live
  ECS; the old sprite texture is no longer uploaded for them.

## Follow-up boundary

1. extend authored-mesh replacement to the loose-NIF route and add the optional
   tri-planar 2D-mask density path for silhouettes that need texture fidelity;
2. map the verified Starfield height-fog block without guessing its curve;
3. add the 32³ aerial-perspective LUT and a non-RT cascade visibility variant;
4. extend the existing glass transmittance hook with ratio tracking and a
   majorant grid for path-traced heterogeneous media.
