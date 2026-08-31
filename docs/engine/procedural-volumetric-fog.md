# Procedural volumetric fog and emissive media

## Decision record

**Status:** froxel core plus authored global/local conversion landed (physical
spectral single scattering, temporal history, RT visibility, FSR contract,
XCLL/WTHR → engine-native medium, smoke/fog particles → clustered local
primitives, boot-generated tileable Perlin-Worley base/detail density,
weather-classified coverage). Emissive media now feed a double-buffered
thermochemical/velocity field: world-space RK2 advection, fuel/oxidizer burn,
soot production, cooling, entrainment, drag, turbulence, and buoyancy run in
the froxel injector before radiative coefficients are derived. Conservative
TLAS-backed solid blocking prevents RK2 backtraces from pulling chemistry
through opaque geometry. A fixed-point GPU moment grid now reduces the
transported emissive field into delayed canonical surface lights without a
source-primitive proxy. Slip/pressure boundary response and real-content
visual calibration remain open follow-ups.

**Location:** `crates/core/src/ecs/components/fog_volume.rs`,
`byroredux/src/{fog,render/{fog_volumes,lights}}.rs`,
`crates/renderer/src/vulkan/volumetrics.rs`,
`crates/renderer/shaders/volumetrics_{inject,integrate}.comp`, and
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
- Defaults are one froxel per 8×8 render pixels, 64 Z slices, and a 128 m
  grid far plane.
- The first 5 m consume the first 1/8 of Z linearly. The remaining slices are
  exponential from 5 m to the configured far plane.
- The raw V-buffer stores `(source radiance per metre.rgb, sigma_t)`. The
  integrated volume stores `(accumulated radiance.rgb, transmittance)`. A
  separate R32F history sidecar stores the deterministic thermal-emission
  fraction because that provenance cannot be reconstructed after emission,
  scattering, and stochastic visibility have been summed into RGB.
- Two RGBA16F per-frame history volumes carry canonical combustion state:
  `(fuel, temperature K, soot extinction, visible-radiance calibration)` and
  `(world velocity vx/vy/vz, oxidizer)`. Each current froxel reconstructs an absolute world
  position, reprojects it into the previous camera grid, and performs an RK2
  semi-Lagrangian backtrace before sampling both fields with linear/clamp.
  At a dilute leading edge, velocity is extended from the strongest adjacent
  active cell whose flow points into the destination; chemistry still arrives
  only through the backtrace. Without this upwind extension an empty cell asks
  itself for velocity, receives zero, and a flame remains pinned to its source.
  Storage is view-aligned, but transport is world-space and therefore does not
  turn camera motion into fake fluid velocity.
- XCLL/WTHR near/far ramps are converted once at the cell/weather translation
  boundary. Runtime volumetrics consume extinction in inverse metres,
  single-scatter albedo, and coverage; they never evaluate the legacy ramp.
- Local authoring is likewise translated once into a canonical `FogProfile`
  (`Homogeneous`, `Smoke`, `Flame`, or `Explosion`). `FogSource` is diagnostic
  provenance only. ECS and renderer code consume the profile directly and
  never infer behavior from game identity, import source, blend mode, or a
  coincidental combination of optical coefficients.
- Authored LIGH records and procedural combustion both cross the render
  boundary as the same canonical `Emitter`: position in metres, radiant
  intensity in watts per steradian, and range in metres. The shared GPU
  encoder owns the only Bethesda-unit conversion, so runtime lighting has no
  game-specific unit or schema branches.
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
- One jittered sample is evaluated per froxel. Previous raw V-buffer and
  emission-provenance history are reprojected with the previous camera matrix.
  Steady media blend at 0.92; an emissive leading or trailing edge selects the
  shorter fire history through `max(current_emission, previous_emission)`.
  Relative extinction changes exponentially reduce either weight. Storing the
  current share rather than the widened value prevents a departed flame from
  sustaining its own fast-history classification indefinitely.
- Integration treats every hybrid-Z slab as a homogeneous medium and applies
  the exact Beer-Lambert source integral
  `(1 - exp(-sigma_t * dt)) / sigma_t`, with a cancellation-safe series near
  vacuum. Thick smoke therefore converges to its equilibrium radiance instead
  of growing brighter without bound under the former rectangle rule.
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
  Thermally identified flame, ember, and explosion emitters cross the same
  boundary as `Flame` or `Explosion` profiles regardless of blend mode;
  non-thermal additive magic particles remain billboards.
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
material — soot — represented by the same canonical `FogVolume` source
contract and transported combustion field. `FogProfile` controls how a source
injects fuel, heat, soot, oxidizer, and momentum; it does not select a separate
renderer. Homogeneous authored fog remains analytic, while `Smoke`, `Flame`,
and `Explosion` feed the field and are not also rendered as pinned analytic
bodies:

| | extinction | single-scatter albedo | emission `L_e` |
|---|---|---|---|
| flame | high | **0.25** (soot absorbs) | blackbody at its temperature |
| smoke | moderate | **0.9** (cooled soot scatters) | none |

- Chemistry stores soot directly as optical extinction per world unit, avoiding
  a second arbitrary density-to-optics conversion. Fuel burns above a 720 K
  ignition window at a rate proportional to fuel × oxidizer; heat release and
  oxygen-starved soot yield come from that same reaction. Exponential cooling,
  fuel dissipation, oxidizer entrainment, drag, divergence-free turbulent
  acceleration, and temperature-driven buoyancy advance the state each frame.
  Soot uses only a mild removal term (about a 15-second half-life); transport
  and mixing, rather than the thermal envelope, disperse the visible cloud.
- Persistent flames replenish a compact basal fuel/heat boundary, passive smoke
  replenishes soot and mild exhaust velocity, and explosions add a finite
  radial heat/momentum impulse. A reset field seeds the explosion's current
  normalized age directly; warm history receives a delta-scaled impulse so the
  full blast cannot be added every frame. After an emitter disappears, a
  20-second CPU latch keeps the solver advancing while soot decays below 3%.
- The froxel emission source term is `sigma_a * L_e` with
  `sigma_a = sigma_t * (1 - albedo)`, evaluated per channel against the
  transported soot/fuel state, so emission moves and cools with the fluid
  instead of filling or following an analytic primitive. It is unconditional —
  independent of the phase function and of the shadow ray — because a flame
  radiates from being hot, not from being lit.
- No new render pass or sort order. `volumetrics_integrate.comp` treats the
  injection buffer's RGB as a source coefficient per unit length and evaluates
  its exact homogeneous-slab integral against Beer-Lambert attenuation and
  accumulated transmittance. Fire therefore composites, denoises, reprojects,
  and meets the FSR contract through the machinery fog already uses. The
  scalar temporal-provenance sidecar and two RGBA16F transport histories carry
  no final radiance and are not sampled by composite.
- Canonical source colour and brightness are produced by
  `byroredux_core::radiometry`: chromaticity from Planck's law integrated
  against the CIE 1931 observer, magnitude from the visible-band `Y` integral.
  The field transports that source's luminance calibration separately from
  temperature; per-froxel cooling applies a cheap Planck ratio at the 555 nm
  photopic peak and recomputes blackbody chromaticity. The magnitude law is
  deliberately **not** Stefan-Boltzmann `T^4`, which describes total exitance
  — mostly infrared at flame temperatures — and would understate how sharply
  a cooling flame dims.
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
- Surface illumination is reduced from the transported field, not from the
  source primitive. Every emissive froxel integrates `j = sigma_a * L_e` over
  its frustum-cell volume, applies a local optical-depth escape probability,
  and atomically contributes fixed-point RGB intensity, centroid, and luminous
  volume to a camera-centred 8×4×8 grid. After that frame slot's fence, the CPU
  decodes at most 64 inverse-square `GpuLight`s and feeds them through the same
  cluster/GI contract as authored lights. An authored LIGH inside a source
  volume suppresses a reduced centroid only while that centroid remains inside
  the same volume; advected plume/blast emission outside it remains eligible.
- Explosion age is resolved once at the ECS→GPU boundary and enters the solver
  as a canonical normalized source parameter. It controls the finite ignition/
  smoke envelopes and reset seeding; subsequent temperature, soot, and motion
  evolve in the transported field. Surface illumination follows those evolved
  values after the normal frame-in-flight latency, so cooling, buoyant motion,
  wall blocking, and source expiry affect the visible medium and nearby
  surfaces through the same state.
- Solid interaction is canonical too. The core `VisibilityMask::SOLID` contract
  contains opaque geometry plus glass and excludes effect cards. The injector
  ray-queries that mask for both the semi-Lagrangian source→destination path
  and a forward velocity probe. A committed rigid hit recovers its triangle
  normal from the same instance/global vertex/global index buffers that back
  the TLAS, removes only inward normal velocity, and retains tangential slip
  with mild wall friction. Malformed, degenerate, or skinned hits remain
  no-through and fall back to a conservative opposing normal.
- Emissive froxels use a shorter temporal history weight
  (`DEFAULT_EMISSIVE_HISTORY_WEIGHT`, `fog_reference.y`) blended in by the
  larger of the current and reprojected previous emissive fractions. The
  previous share makes the response symmetric: newly arrived and newly
  departed fire both reject stale radiance quickly instead of leaving a slow
  tail. Provenance remains separate from the stochastic radiance term because
  the sun visibility test is a single jittered *binary* sample that
  legitimately flips at shadow boundaries; using that flip as a generic
  disocclusion would suppress accumulation exactly at the god-ray edges
  M-LIGHT v2 added it to clean up.

### Game-data-independent lab

`--combustion-lab` builds a six-by-four-by-eight-metre Cornell room using the
canonical 70 Bethesda units/metre conversion. A persistent 1850 K flame and a
delayed 2800 K one-shot explosion feed ordinary `FogVolume` profiles beneath a
rigid hood whose underside is 1.24 m above the floor. The harness contains no
game identity or import shortcut: it validates the same parser-independent ECS
and GPU contracts that shipped particle effects reach after translation.

```bash
cargo run --release -- --combustion-lab
BYROREDUX_RENDER_DEBUG_MODE=volume cargo run --release -- --combustion-lab
```

In the isolated volume view, verify a hot basal flame, a buoyant gray plume,
occlusion through the hood depth with lateral reappearance above an edge, a
separate explosion core, and a cooled soot cloud after its radiance falls. In
the final composite, verify that the delayed transported-field light moments
warm nearby surfaces and then recede with cooling. The named `volume` view
maps both radiance and opacity, so non-emissive soot remains inspectable.

### Open calibration

`FLAME_REFERENCE_RADIANCE` is the one exposure choice in the chain; everything
else is physics. It is derived rather than eyeballed — the path integral
reduces to `optical_depth * (1 - albedo) * L_e`, about `0.3 * L_e`,
independent of flame size — and the resulting torch reach lands within a
factor of two of vanilla authored torch LIGH radii, which is a cross-check on
the whole chain rather than an input to it. A synthetic runtime lifecycle A/B
on 2026-08-17 confirmed that the cooling phase restores nearby material detail
and expiry removes both the volume and reduced field light. A visual A/B against
real shipped fire content remains before treating the exposure choice as
final.

## Configuration

```text
--froxel-xy-divisor <2..32>   default 8
--froxel-z-slices <16..256>   default 64
--fog-grid-far-m <32..512>    default 128
BYROREDUX_RENDER_DEBUG_MODE=volume   isolated raw integrated froxel field
```

Example:

```bash
cargo run --release -- --game fnv --cell GSProspectorSaloonInterior \
  --froxel-xy-divisor 8 --froxel-z-slices 64 --fog-grid-far-m 128
```

## Measurement table

Keep rows even when a path is not implemented; `—` means no data rather than a
fabricated zero. The initial smoke used an RTX 4070 Ti, driver 580.173.02,
1280×720 output with FSR Quality, FNV Prospector Saloon, after pipeline warmup.
The timer brackets inject plus integrate.

| Dimension | Value | Froxel extent | Volumetrics GPU | Status / evidence |
|---|---:|---:|---:|---|
| XY divisor | 4 | 214×120×64 | 0.17–0.20 ms | exact slab transport + emissive sidecar, Vulkan validation runtime |
| XY divisor | 8 | 107×60×64 | — | **default**; allocation/dispatch smoke passed; timed warmup pending |
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

At the default 107×60×64 grid (`froxel_xy_divisor = 8`), the five RGBA16F
fields (raw, integrated, chemistry, velocity, `combustion_optical`) plus the
R32F emissive-history sidecar — `FROXEL_VOLUMES_PER_SLOT = 6`,
`FROXEL_BYTES_PER_SLOT = 44` — consume about 17.2 MiB per frame slot, ~34.5
MiB across 2 FIF. (At the older 214×120×64 divisor-4 grid the same six-volume
set would be about 69 MiB per frame slot — do not confuse this with the
retired four-volume/36-B-per-froxel figure of 56 MiB, which predates the
`combustion_optical` volume.) Target ranges for the
reference 160×90×64 grid remain 0.2–0.5 ms inject and
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

1. extend conservative no-through solid blocking with boundary-normal slip /
   pressure response, then calibrate source, decay, and field-light reduction
   against shipped fire, smoke, and explosion content. Canonical `FogProfile`
   values already act as solver emitter presets; the solver has no game or
   authoring-provenance branch;
2. extend authored-mesh replacement to the loose-NIF route and add the optional
   tri-planar 2D-mask density path for silhouettes that need texture fidelity;
3. map the verified Starfield height-fog block without guessing its curve;
4. add the 32³ aerial-perspective LUT and a non-RT cascade visibility variant;
5. extend the existing glass transmittance hook with ratio tracking and a
   majorant grid for path-traced heterogeneous media.
