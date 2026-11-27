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
source-primitive proxy. Calibration of the solid boundary and real-content
visual calibration remain open follow-ups
([follow-up boundary](#follow-up-boundary)).

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
- Sébastien Hillaire, [Physically Based and Unified Volumetric Rendering in
  Frostbite](https://advances.realtimerendering.com/s2015/Frostbite%20PB%20and%20unified%20volumetrics.pptx)
  (SIGGRAPH 2015, Advances in Real-Time Rendering course slides). The M55
  parity reference; its feature checklist is
  [PRD §8.1](../prd/volumetric-lighting.md#81-modern-parity-checklist).

## Runtime contract

Cloud and local-medium optics share `include/medium_transport.glsl`:
Henyey–Greenstein scattering and the cancellation-safe homogeneous slab
integral are used by the sky march and fog/combustion passes. This is the
first shared implementation seam with [SKYAL](skyal.md); cloud self-shadowing
and multiple scattering still need to be adapted to local smoke. Combustion
continues to own its temperature, soot, emission, and velocity evolution.

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
- Wind is the canonical `WindField` already resolved from EXAL/WTHR and shared
  with foliage and water. The renderer passes its direction, base speed, gust
  amplitude, and frequency into the combustion solver. RK2 backtracing uses
  wind as an external X/Z advection velocity with mild vertical shear; local
  combustion velocity remains separate, so weather changes do not integrate
  into runaway speed. Solid-boundary rejection applies to the combined local
  flow plus wind, allowing smoke to bend around walls while a blast's pressure
  impulse and vorticity still create roll-up rather than a rigid translated
  billboard.
- XCLL/WTHR near/far ramps are converted once at the cell/weather translation
  boundary. Runtime volumetrics consume extinction in inverse metres,
  single-scatter albedo, and coverage; they never evaluate the legacy ramp.
- Local authoring is likewise translated once into a canonical `FogProfile`
  (`Homogeneous`, `Smoke`, `Flame`, or an explosion profile). Explosions are
  split into conventional `OilExplosion` and high-yield `NuclearExplosion`;
  `FogSource` is diagnostic provenance only. ECS and renderer code consume the
  profile directly and never infer behavior from game identity, import source,
  blend mode, or a coincidental combination of optical coefficients.
- Authored LIGH records and procedural combustion both cross the render
  boundary as the same canonical `Emitter`: position in metres, radiant
  intensity in watts per steradian, and range in metres. The shared GPU
  encoder owns the only Bethesda-unit conversion, so runtime lighting has no
  game-specific unit or schema branches.
- The authored sRGB fog colour is decoded once at the render boundary, then
  its linear-light value is normalized by its strongest channel and multiplied
  into the global single-scatter albedo. This preserves the legacy hue without
  changing extinction or raising the medium's peak scattering energy; authored
  black remains a finite, purely absorptive medium.
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
  Thermally identified flame, ember, gas-flame, and explosion emitters cross
  the same boundary as canonical combustion sources regardless of blend mode;
  non-thermal additive magic particles remain billboards until a later source
  regime gives them an energy/plasma spectrum.
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

## Weathered ground response

Rain and snow are surface state, not another fog volume. `WeatherSurfaceState`
keeps two history-dependent channels for the active exterior: absorbed rain
wetness and accumulated snow coverage. Rain raises wetness quickly; clear-air
drying is modulated by sun intensity and wind. Snow accumulates independently,
then melts more slowly under rain and sunlight. Interior weather sampling can
continue for window portals without depositing precipitation on sealed floors.

The camera upload carries those channels as unorm16 values in the existing
`GpuCamera.render_debug.w` lane. Exterior LAND fragments turn them into a
geometric-slope mask: broad low-frequency low spots become darker, smoother,
cool dielectric puddles, while snow grows from patchy coverage into a rough,
bright layer. The first pass intentionally limits this to streamed terrain;
ordinary exterior architectural floors need an authored/exposure classification
before they can share the effect without making interior floors wet.

This split is important for later effects: rainfall can feed puddle retention
and runoff, snowfall can gain depth/compaction and occlude ground detail, and
magic/energy weapons can contribute their own thermal or plasma surface state
without being misclassified as weather or fire.

## Emissive media (fire)

Fire is not a separate subsystem. A flame and its smoke are one physical
material — soot — represented by the same canonical `FogVolume` source
contract and transported combustion field. `FogProfile` controls how a source
injects fuel, heat, soot, oxidizer, and momentum; it does not select a separate
renderer. Homogeneous authored fog remains analytic, while `Smoke`, `Flame`,
and both explosion profiles feed the field and are not also rendered as pinned
analytic bodies:

| | extinction | single-scatter albedo | emission `L_e` |
|---|---|---|---|
| flame | high | **0.25** (soot absorbs) | blackbody at its temperature |
| smoke | moderate | **0.9** (cooled soot scatters) | none |

The current source regimes are deliberately distinct even when they share the
same transported state: ordinary diffusion flame (1850 K, orange/yellow and
soot-rich), gas flame (3400 K, blue-shifted and cleaner), oil explosion (2800 K
flash with a compact radial fireball and dense soot), and nuclear explosion
(4200 K flash with a broader pressure phase, slower cap/stem transition, and
longer-lived buoyant aerosol). This is the source-physics seam that future
magic and energy weapons should extend; their spectrum belongs in a new
regime, not in a texture tint applied after transport.

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
  radial heat/momentum impulse. Oil blasts use a compact fireball and fast soot
  envelope; nuclear blasts use a wider, slower-expanding cap/stem source with
  stronger buoyant lift. A reset field seeds the explosion's current normalized
  age directly; warm history receives a delta-scaled impulse so the full blast
  cannot be added every frame. After an emitter disappears, a 78-second CPU
  latch keeps the solver advancing while soot decays below 3%.
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
  3400 K for a blue-shifted gas flame, 1100 K for embers and charcoal, 2800 K
  for an oil fireball, and 4200 K for a nuclear flash. Optical depth is pinned across the primitive
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
- Transport fidelity: the dye fields (chemistry + spectral optics) are advected
  with a single-pass BFECC error correction (Selle et al. 2008) on top of the
  RK2 backtrace. The correction differences the field against a
  forward/backward trace pair to recover the one-step dissipation semi-
  Lagrangian transport smears away, clamped to the eight previous-grid texels
  feeding the source tap so the scheme stays unconditionally stable. It reads
  only the previous frame-in-flight slot — no intermediate image — and is
  gated to cells where advection actively imports medium. Velocity is never
  corrected (a dynamics error term injects energy at curl discontinuities).
  With recovered detail available, the curl-noise forcing scale and interface
  vorticity confinement were turned down (1.6 → 1.1, 0.75 → 0.55 m/s) so
  motion reads as emergent from transport. `BYRO_BFECC=0` reproduces plain
  semi-Lagrangian transport in the same binary for A/B.
- Optics: soot is spectral, not gray. The transported single-scatter albedo is
  `[0.22, 0.16, 0.12]` — warm-weighted because fresh soot absorbs blue more
  strongly — so cooled plumes scatter and transmit warm instead of reading as
  neutral gray dust. Point/spot in-scattering uses a fixed soot-like
  dual-lobe Henyey-Greenstein phase (forward 0.35 / backward −0.12, mix 0.65)
  instead of isotropic 1/4π, matching the asymmetry the sun path gets from
  authored weather. And the local-medium in-scatter carries a two-octave
  multiple-scattering energy compensation (`1 + 0.6·ω + 0.3·ω²` in the
  medium's spectral albedo) so optically thick media do not read as
  single-scatter-dark; extinction, authored fog, and emission are untouched,
  keeping fog-only scenes bit-identical.

### Game-data-independent lab

`--combustion-lab` builds a six-by-four-by-eight-metre Cornell room using the
canonical 70 Bethesda units/metre conversion. A persistent 1850 K flame and a
delayed 2800 K oil explosion feed ordinary `FogVolume` profiles beneath a
rigid hood whose underside is 1.24 m above the floor. Add
`--combustion-lab-nuclear` to replace that impulse with the 4200 K nuclear
cap/stem case; its open upper volume leaves the high-yield cloud visible while
the oil case remains the solid-boundary regression fixture. The harness
contains no game identity or import shortcut: it validates the same
parser-independent ECS and GPU contracts that shipped particle effects reach
after translation.

```bash
cargo run --release -- --combustion-lab
BYROREDUX_RENDER_DEBUG_MODE=volume cargo run --release -- --combustion-lab
cargo run --release -- --combustion-lab-nuclear
BYROREDUX_RENDER_DEBUG_MODE=volume cargo run --release -- --combustion-lab-nuclear
```

In the isolated volume view, verify a hot basal flame, a buoyant gray plume,
occlusion through the hood depth with lateral reappearance above an edge, a
separate explosion core, and a cooled soot cloud after its radiance falls. In
the final composite, verify that the delayed transported-field light moments
warm nearby surfaces and then recede with cooling. The named `volume` view
maps both radiance and opacity, so non-emissive soot remains inspectable.

The lab is also a permanent regression gate: `combustion_lab_golden_frame`
(`byroredux/tests/golden_frames.rs`) captures frame 60 of the frozen-clock
scene and per-pixel compares it against `tests/golden/combustion_lab_60f.png`,
so the fire look — soot spectrum, local-light phase, multiple-scatter gain,
BFECC advection — cannot silently drift.

The 2026-09-21 look A/Bs ran here (same binary, env-flipped): BFECC raised
cooled-smoke gradient energy +23.6% (p90 +26%) with the smeared thin halo
−19% and `gpu_volumetrics` flat at ~0.45 ms; after the forcing retune the
plume still carries +14% detail over the pre-BFECC operating point with ~31%
less curl forcing. The spectral-soot wave moved smoke R/B tint 3.086 → 3.165
with flame energy within 0.2%.

### Open calibration

`FLAME_REFERENCE_RADIANCE` is the one exposure choice in the chain; everything
else is physics. It is derived rather than eyeballed — the path integral
reduces to `optical_depth * (1 - albedo) * L_e`, about `0.3 * L_e`,
independent of flame size. The earlier claim that the resulting torch reach
lands within a factor of two of vanilla authored torch LIGH radii did not
survive a device check (2026-10-31): the reach is `sqrt(luma / 1e-3)` clamped
to 64 m, so a hearth sits at the clamp (about 14-17x a vanilla torch's cull
radius) while a small brazier's derived luma is ~0.01 against ~1 for an
authored light. A synthetic runtime lifecycle A/B
on 2026-08-17 confirmed that the cooling phase restores nearby material detail
and expiry removes both the volume and reduced field light. A visual A/B on
Skyrim hearths (2026-10-31, `BYRO_FIRE_VOLUMES` on/off, controlled for
time of day) shows no blow-out: the Bannered Mare hearth region is ~18%
brighter, the frame ~3.5%. The exposure choice stands; the reach does not (see
above).

## Configuration

```text
--froxel-xy-divisor <2..32>   default 8
--froxel-z-slices <16..256>   default 64
--fog-grid-far-m <32..512>    default 128
BYROREDUX_RENDER_DEBUG_MODE=volume   isolated raw integrated froxel field
BYRO_BFECC=0                          plain semi-Lagrangian transport (A/B)
BYRO_SUN_ALPHA=0                      froxel sun trace reads every triangle as solid (A/B, VLM-SH-009)
BYRO_FROXEL_SUN_OCCLUSION=0           diagnostic: ignore exterior froxel sun blockers (surface shadows and interiors unchanged)
BYRO_FROXEL_SUN=0                     diagnostic: disable the froxel sun term (local lights and surface lighting unchanged)
BYRO_FREEZE_FROXEL_PATTERN=1          diagnostic: freeze all volumetric time (the noise phase AND the combustion clock: fire and smoke stop after their first frame, #7388)
BYRO_SUN_DUST=<per metre>             experiment, off by default: extra scattering on the open-sky sun term only
```

An explicit `--froxel-xy-divisor` below what the device can store is raised to
the smallest fitting value at pipeline init and on resize (#4781): the grid's
X/Y are 3D-image dimensions bounded by `maxImageDimension3D` (2048 on Mesa ANV
and lavapipe), not by the 2D limit the render extent is checked against. The
local-grid irradiance image is twice the froxel width (#6827), so the froxel
width is bounded by half the limit. The default 8 covers an 8192-wide render at
a 2048 limit (a 16384-wide one is raised to 16), so this only raises sub-8
divisors at very large render extents, with a warning.

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
| XY divisor | 8 (combustion) | 160×90×64 | 0.45–0.46 ms | `--combustion-lab` live, flame + post-explosion plume, 720p native TAA, 2026-09-21; BFECC on/off identical within 0.01 ms |
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

### Exterior fixtures, inject and integrate separately (VLM-SH-008, 2026-11-07)

RTX 4070 Ti, 1280x720 TAA, default 160x90x64 grid, `renderer-stepped` static camera at each fixture's default
spawn (`m-exteriors.sh` grids, radius 1), 150 frames, the bench line's `gpu_volumetrics_inject` and
`gpu_volumetrics_integrate`. "Off" is `BYRO_SUN_ALPHA=0` (the opaque sun trace of before VLM-SH-009).

| Fixture | Inject, noon | Inject, 07:00 | Inject, 07:00, alpha trace off | Integrate |
|---|---:|---:|---:|---:|
| FNV WastelandNV (0,0) | 0.418 ms | 0.598 ms | 0.254 ms | 0.020-0.022 ms |
| FO3 MegatonWorld (-1,-7) | 0.304 ms | 0.398 ms | 0.208 ms | 0.020 ms |
| Oblivion Tamriel (0,0) | 0.520 ms | 0.583 ms | 0.313 ms | 0.020-0.022 ms |
| Skyrim SE Tamriel (2,-4) | 0.305 ms | 0.308 ms | 0.197 ms | 0.020-0.022 ms |
| FO4 Commonwealth (0,0) | 0.768 ms | 0.741 ms | 0.456 ms | 0.021-0.022 ms |

The alpha-aware trace adds 0.11 to 0.34 ms of inject at 07:00 (FNV +0.34, FO3 +0.19, Oblivion +0.27, Skyrim
+0.11, FO4 +0.29): every froxel the binary trace calls blocked is walked again nearest-first. Integrate is
constant. The finished frames match the off captures to within 0.08 % of pixels differing by more than four
levels (FO4 the most, max 60 levels; FNV, FO3, Oblivion and Skyrim under 0.01 %). Noon "off" was not run, and
the Goodsprings pinned-pose figures above (0.18 to 0.26 ms) are a different pose from FNV's spawn here. No
cheaper structure exists today: the visibility layers do not separate alpha-tested surfaces (the foliage layer
is the decal render layer), so the second walk cannot be skipped by mask.

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

### Exterior shafts and the haze floor (2026-11-07, #6726)

Measured on an RTX 4070 Ti at 1280×720, TAA, in `renderer-stepped` with a pinned camera, where a repeat
capture is bit-identical (noise exactly 0). Reproduce with `docs/smoke-tests/exterior-shafts.sh` (the lab)
and `BYRO_HAZE_FLOOR` / `fog.haze` (the floor).

- **Clear weather is thin.** FNV `WastelandNV` at rest: `gpu_extinction` 0.000349 / m, about 4.5 % optical
  depth over the 128 m froxel range (`env.health`).
- **The froxel sun term already resolves shafts.** In `--shaft-lab` (the engine's fallback exterior fog, 0.000622 / m
  on the GPU, about 1.8 times FNV's clear weather; a 10° sun behind 12 slats, camera on the
  sun's axis, in-scatter term only) the lit-versus-shadow contrast across the wall is 0.190 with no floor,
  0.170 at the first default and 0.111 at 0.004 / m; the solid-wall control reads 0.001–0.002. Off the axis
  (20 m to one side) each view ray crosses the 3 m slat pitch every few metres and averages the stripes
  away, so the lab's camera sits on the axis and the gate says so.
- **What a floor buys is absolute brightness, at a cost in relative contrast.** Lit air reads 66, 98, 193 on
  the term's 8-bit scale at floor off, 0.0015 / m and 0.004 / m, and the contrast falls as above; the
  side regions of the finished frame darken by 1.5 % and 7.9 %. So the floor's shape is a thin noon value
  that rises steeply toward the horizon, where long shafts are actually seen (VLM-SH-010).
- **Default: `0.0006 / m × (1 + 6 × (1 − sin e)³)`, faded to zero around the horizon.** That is 0.0006 / m at
  noon, 0.0027 / m at a 10° sun and 0.0042 / m at the horizon. It makes the lab's open scene read 0.155
  against the control's 0.002 (`exterior-shafts.sh` PASS).
- **Cost to the authored look, FNV Goodsprings, floor off versus default, the same pinned pose:** noon moves
  the frame by 1.3 / 255 mean absolute (+2.8 % mean luminance, +5.3 % in the sky); sunrise by 7.7 / 255 (a
  dark frame, so +47 % relative). Both are a faint veil, not a repaint. The owner reviews these
  (VLM-SH-011); the floor is adjustable at run time.
- **On content, FNV Goodsprings, floor off versus default, exposure settled (VLM-SH-006, 2026-11-07).** At 06:30
  (sun 6.4 degrees, intensity 1.0), same engine and paused clock, a capture taken 16 s after each change, with an
  off, default, off sequence as the control: frame mean 131.4 off, 126.9 default, 131.3 off again; mean absolute
  difference 1.6 / 255 off to off (the noise floor) against 34.5 / 255 off to default. The floor does not change the
  overall brightness; it changes the structure: crisp distant ridgelines become layered aerial perspective, valley
  over valley. (An earlier version of this entry quoted +9.5 % and +7.0 % frame-mean gains; they were captured right
  after a clock jump, before auto-exposure had adapted, and are withdrawn.) No crisp beams appear in the scenes
  tried (a radio dish, a steel derrick with chain-link, a rock cluster): the lattice and rocks cast shadow volumes
  in the haze, and shafts from cloud gaps are outside the froxel range by the PRD's Q3 decision. A fixture whose
  occluder has regular gaps at the camera's distance is still needed for VLM-SH-006 beyond the lab.
- **Alpha-tested cutouts (VLM-SH-009, 2026-11-07).** The froxel sun trace used to read every triangle as solid
  (`traceShadowBinary`, BLAS built `OPAQUE`), so canopy, fences and grates cast a wall of shadow into the haze
  where the surface shadow, already alpha-aware, showed holes. Under open sky a sun ray the binary trace calls
  blocked is now re-walked nearest-first (`sunRayBlockedAlphaAware` in `volumetrics_inject.comp`), stepping past
  cutout holes through the shared `alphaComparePass` and the shadow transport's eight-layer budget; interiors
  keep the binary trace. The lab's matched variants (`--shaft-lab-card`, `-card-geo`, `-card-solid`) read the
  cutout panel at 129.18 against 129.16 for the same stripes as real holes (an opaque trace read 116.36). Cost on
  FNV Goodsprings, same pose, per-pass `gpu_volumetrics_inject`: 0.18 to 0.26 ms at noon, 0.19 to 0.35 ms at
  07:00, where more froxels are in terrain shadow and take the second walk; finished frames are identical, as
  no cutout caster is in view. `BYRO_SUN_ALPHA=0` is the same-binary A/B switch. Content with real canopy is
  not yet captured: Skyrim Riverwood at 17:30 and 18:36, three poses (under the sawmill, two west-facing
  through pines), read identically on and off (0.00 % of pixels differ by more than 4 levels), because the
  trees there are thin or farther than the froxel range; a pose inside dense broadleaf or a fence line at
  close range is still needed.
- **Shaft range (VLM-SH-005, 2026-11-07).** The exterior lab with the camera at 25 to 150 m
  (`--shaft-lab-distance`), contrast of the open wall minus the solid control over the wall body: 0.233 (25 m),
  0.237 (40), 0.154 (55), 0.131 (70), 0.071 (90), 0.031 (110), 0.004 (128), 0.001 (150). Against the gate's 0.05
  bar the shaft reaches between 90 and 110 m and is gone at the 128 m far plane. Why it falls well before the
  plane is not isolated here: the froxel slices thicken with distance, the wall's gaps shrink in the frame, and
  the sun clears the wall top beyond about 70 m, and this sweep does not separate them.
- **Real-content search, 2026-11-07.** The in-scatter radiance view (`render.debug volumetric_radiance`) at 07:00 on
  the five fixtures' default spawns shows tree-trunk shadow volumes in the haze (FO3, FO4) and leaf silhouettes
  at the canopy edge (Oblivion), but no crepuscular beams. The Oblivion leaf shapes are surface coverage: the same
  capture with `BYRO_SUN_ALPHA=0` differs by at most one level (0.00 % of pixels above three), so the alpha trace
  does not change that frame. Under the shipped haze a real canopy shaft is not visible at these poses; only the
  lab shows crisp beams. Whether to raise the exterior floor for content (PRD Q7 was decided at the engine
  default) is the owner's call, with these captures as the evidence.
- **Floor strength on content, 07:00 default spawns (2026-11-07), for the owner's Q7 call.** `BYRO_HAZE_FLOOR`
  `0.0006,6` (shipped), `0.003,6` and `0.01,6` on FO3, Oblivion and FO4 (the same poses, `renderer-stepped`):
  frame mean 45.2 / 76.4 / 116.8 (FO3), 46.8 / 58.4 / 79.5 (Oblivion), 56.2 / 69.2 / 93.3 (FO4), mean absolute
  change from the shipped floor 31 / 72, 12 / 33 and 13 / 37 levels. At `0.01` a lit wedge becomes visible under the
  Oblivion canopy and the FO4 trunks gain lit columns; at `0.003` the gain is a mild haze with no distinct beam.
  So a floor strong enough to show shafts on content also lifts the whole frame by tens of levels (a wash, not a
  beam), which is the tension VLM-SH-011 asks the owner to weigh. A floor gated to sun-lit froxels might lift the
  shaft without the wash, untested; none is built. Recipe: `BYRO_HAZE_FLOOR=<base>,<gain> BYRO_HOUR=7 byroredux ... --bench-mode
  renderer-stepped --bench-camera static --screenshot`.
- **Sun-term-only dust on content: the first real-content shaft (`BYRO_SUN_ALPHA` sibling `BYRO_SUN_DUST`,
  2026-11-07, off by default).** The floor above lifts the sky ambient too, which is the wash. The interior already
  solves that by adding scattering to the sun term alone (VLM-INT-007); the same term under open sky leaves the
  extinction, the ambient and the shadowed air alone. Same 07:00 poses, shipped floor, `BYRO_SUN_DUST` per metre:
  0.01 / 0.03 / 0.08 change Oblivion by 0.5 / 1.3 / 2.7 levels (mean absolute), FO4 by 0.1 / 0.3 / 0.8 and FO3 by
  0.0 (its visible froxels are not sunlit at that hour); 0.5 gives Oblivion +8.1 and FO4 +4.2, and **2.0 gives a
  distinct warm shaft under the Oblivion canopy beyond the rock, with the shadowed grass unchanged, and a sunlit
  haze behind the FO4 trunks** (mean change 18.6 and 11.3 levels, concentrated in the lit air). Cost: none
  measurable (inject 0.58 and 0.74 ms, as without). Caveats the owner should weigh: 2.0 per metre of scattering
  with no matching extinction is an artistic strength, not a measured dust (the interior uses 0.04); it is flat
  in sun elevation (the floor's horizon gain is not applied); and one pose per title is thin evidence. Recipe:
  `BYRO_SUN_DUST=2.0 BYRO_HOUR=7 byroredux ... --bench-mode renderer-stepped --bench-camera static --screenshot`.
  **Elevation shaping (same day).** Flat at 1.0 per metre the same dust washes every noon frame out (mean change
  54 / 54 / 61 / 76 / 30 levels on FNV / FO3 / Oblivion / Skyrim / FO4), though at noon FO3 also shows the shadow of
  a wire as a dark line across the lit haze. So the dust now fades with the sun's elevation sine,
  `1 - smoothstep(0.2, 0.7, sun_y)` (full below about 12 degrees, none above about 44). At 2.0 per metre across the
  five fixtures, 12:00 and 07:00: every noon frame is unchanged (mean absolute change 0.00 to 0.01), Oblivion
  at 07:00 keeps its canopy shaft (+18.6, 25 % of pixels above 16 levels), FO4 at 07:00 gains a sunlit haze behind
  the trunks (+9.2, 13 %), FO3 and Skyrim at 07:00 barely move (their visible froxels are not sunlit), and FNV at
  07:00 does not move. The shaping edges are engine choices for the owner, like the value.
  **Correction, same day: 2.0 per metre is not a usable default.** The same dust on more poses and hours, on
  versus off (mean absolute change in levels): 17:30 on the original grids FNV 1.8, FO3 6.9, Oblivion 72.1, Skyrim
  41.3, FO4 9.5; a second grid per title at 07:00 FNV 43.1, FO3 10.7, Oblivion 15.8, Skyrim 25.6, FO4 70.2. Oblivion
  at 17:30 becomes a golden fog, and FO4 on its second grid a grey one (trunk shadow columns do show inside it).
  The good 07:00 result above was a pose in which the sunlit air was seen away from the sun. Lowering it does not
  fix that: on the four strongest cases, 0.25 gives +16.9 (Oblivion 17:30), +4.4 (Skyrim 17:30), +31.7 (FNV g2) and
  +29.9 (FO4 g2); 0.6 gives +33.3, +11.6, +35.3 and +58.7. Sunlit haze brightens wherever the view crosses lit
  air, so the term is a haze-strength dial whose acceptable value depends on the pose, and the shaft is only the
  part of it where an occluder shades the air. The 0.01 to 0.08 range changed the first pose by 0.5 to 2.7 levels
  and is the only region tried that stays quiet. So what exists is a mechanism with no tuned default; a value
  chosen from one pose per title would be wrong for the others. A distance fade (full within 20 m, none beyond
  70 m) was tried on the same four cases and reverted: it changed them by under 1.5 levels (Oblivion 15.7 against
  16.9 at 0.25, FNV 32.2 against 31.7), so the wash is not the far field. A broader phase lobe for the dust alone
  (Henyey-Greenstein g = 0.3 instead of the haze's tight forward lobe) was also tried and reverted: it was worse,
  0.25 giving +24.0 (Oblivion 17:30), +6.6 (Skyrim), +36.8 (FNV g2) and +43.0 (FO4 g2), because the haze's own phase
  is already tight and the broad lobe adds dust off-axis. Three variants (shaping by elevation, by distance, by phase)
  leave one that helps, the elevation fade, and it only removes the noon wash. What stays unsolved is that the lit
  air brightens the frame in step with the shaft contrast; a fix needs the owner's view of how much haze a shaft may
  cost, or a scene-adaptive exposure-aware gain, neither of which is built.
- **Per-fixture noon and low-sun in-scatter (VLM-SH-006, 2026-11-08,
  `docs/smoke-tests/exterior-shafts-content.sh`).** The script has since moved to the VLM-SH-012 views; this table is
  from its earlier default-spawn form. `render.debug volumetric_radiance` at the five fixtures' default
  spawns, `renderer-stepped`, 1280x720, linear in-scatter radiance (frame mean / 95th / 5th percentile) and the finished
  frame's mean level, at 12:00, 07:00 and 17:30:

  | Fixture | 12:00 | 07:00 | 17:30 |
  |---|---|---|---|
  | FNV | 0.0205 / 0.0319 / 0.0118; 72.6 | 0.0444 / 0.0690 / 0.0265; 27.8 | 0.0367 / 0.0579 / 0.0211; 30.2 |
  | FO3 | 0.1167 / 0.1552 / 0.0817; 98.2 | 0.1102 / 0.1461 / 0.0760; 53.4 | 0.1067 / 0.1386 / 0.0774; 45.0 |
  | Oblivion | 0.0522 / 0.1252 / 0.0185; 63.9 | 0.1043 / 0.2183 / 0.0482; 62.0 | 0.0211 / 0.0565 / 0.0066; 36.8 |
  | Skyrim | 0.0220 / 0.0414 / 0.0118; 38.5 | 0.0123 / 0.0225 / 0.0079; 9.5 | 0.0261 / 0.0496 / 0.0145; 13.7 |
  | FO4 | 0.0344 / 0.0732 / 0.0132; 110.8 | 0.0295 / 0.0662 / 0.0118; 47.7 | 0.0361 / 0.0803 / 0.0132; 49.4 |

  The in-scatter responds to the sun on every fixture (FNV roughly doubles at 07:00, Oblivion doubles at 07:00 and
  falls to 40 % at 17:30, Skyrim at 07:00 halves), but **a shaft contrast across a named occluder cannot be read off
  these frames.** The view samples the froxel column at each pixel's own depth, so a pole or trunk column (FO3's
  pole, FO4's trunks) reads darker because its pixels are nearer, with less air accumulated, not because the air
  behind it is shadowed; a lit and an occluded sample at one depth exist only where an occluder shades air in front
  of a far surface, and the five default poses have none. The lab (`exterior-shafts.sh`) is the one place the
  contrast is defined cleanly. VLM-SH-006's "contrast across a named occluder" is therefore restated for the owner
  as a depth-matched measurement (a pose chosen per fixture where a far wall or terrain behind an occluder is in
  view), which needs per-fixture pose authoring, not a probe.
- **Not done yet:** the per-fixture noon and low-sun captures on the other titles (VLM-SH-006), the range and
  cloud-shadow work (VLM-SH-004, -005), and the separate inject and integrate milliseconds (VLM-SH-008; the
  inject figure above is one pose, not the fixture set).

### Content shaft pose scouting (VLM-SH-012, 2026-11-26)

The owner named all five on 2026-11-27 (PRD VLM-SH-012; Oblivion's as temporary), and they are now checked into the content script
([record](#content-shaft-contrast-from-the-checked-in-script-vlm-sh-006--012-2026-11-27)). The table below is the scouting measurement.
Each was measured with PRD §2's content shaft
contrast from two runs of `exterior-shafts-content.sh` at the pose (`BYRO_CONTENT_CAMERA_POS` / `_FORWARD`), one with
`BYRO_FROXEL_SUN_OCCLUSION=0`, and the pair was then run again (2026-11-26 and -27, release build, 1280x720). Each
sample region is 100 x 32 pixels of distant surface (view depth at least 128 m, checked against a held-session depth
capture). It was **chosen at a strong part of that pose's shaft** from frozen held-session captures (by hand on FNV,
by a best-rectangle search on the others) and then measured on the script runs, so a contrast says how strong the
shaft is there, not across the frame.

| Fixture | Occluder (REFR) | Camera position | Camera forward | Region: columns, rows | Low sun: contrast, repeat | Noon | Finished-frame pixels the shadow changes by > 4 of 255 |
|---|---|---|---|---|---|---|---|
| FNV | `DLC03SatelliteDish01` (`000E7125`) | `-891,8327,-1988` | `0.97269,0.17365,0.15406` | 600–699, 204–235 | 07:00: 0.2346, 0.2346 | 0.0102 | 16.0 % |
| FO3 | `MegatonChurchofAtom` (`0001CBF2`) | `-4022,12068,26501` | `0.88773,0.43837,0.14060` | 571–670, 261–292 | 07:00: 0.1351, 0.1351 | 0.0082 | 38.5 % |
| Oblivion | `TreeSugarMapleForestSU` (`0006276B`) and neighbouring trees | `2048,3380,-1848` | `-0.80671,0.57358,0.14224` | 432–531, 149–180 | 17:30: 0.1454, 0.1454 | 0.0000 | 0.0 % |
| Skyrim | `SFarmhouseWindMillDirt02Whiterun` (`0005CA6C`) | `16747,-4350,18321` | `0.93935,0.30902,0.14878` | 616–715, 226–257 | 07:00: 0.2346, 0.2331 | −0.0002, −0.0006 | 13.6 % |
| FO4 | brick building in precombined geometry (no per-object REFR; `largebrickwall01`) | `2048,964,-1848` | `0.80907,0.57358,0.12814` | 724–823, 396–427 | 07:00: 0.1539, 0.1539 | 0.0000 | 25.8 % |

The last column is the low-sun hour, shadows on against off. Occluders were identified with `pick`, which tests
bounding spheres, at world points read from the depth captures: it names the structure, not the triangle that casts
the sampled shadow.

What limits each pose:

- **Visibility.** With the froxel sun shadow off, the FNV, FO3, FO4 and Skyrim frames carry a haze or glow that the
  shadow takes away; it is plainest on FO3 (a beam under the building's overhang) and faint on Skyrim. **The Oblivion
  shaft does not show in the finished frame at all**: no pixel changes by more than 4 levels.
- **Oblivion's occluder is a stand-in.** The engine draws SpeedTree trees as `sptplaceholderbillboard`, so real tree
  geometry would need the pose measured again.
- **Skyrim streams its occluder in.** The camera and windmill are in cell (4,−5), outside the 3x3 the script's
  `--grid 2,-4` loads first. Streaming follows the camera; the four low-sun radiance logs checked show
  `PelagiaFarmExterior` loaded before the screenshot. It is also the one pose whose repeat differs.
- **Eye height.** The Oblivion and FO4 poses are at the fixture's default spawn position (turned toward the shaft).
  The others were placed with the free camera, and no one has checked that a player can stand there.

FNV detail (weather `NVBlackMountainWeather` through the region override; sun at 07:00 `[0.964, 0.220, 0.148]`):

- Over the 17,903 distant pixels that lost more than 15 % at 07:00, the sun shadow removes 24.7 % of the froxel
  radiance. The pixels were picked by that threshold from the same captures, so this locates the shaft and is not a
  contrast over a fixed region. On those sky pixels the finished frame changes by less than 1 of 255, against a
  bright overcast sky; the same shadow is visible lower down, on the unlit crater wall beside the dish pedestal.
- No usable distant shaft at 12:00 or 17:30 at this pose. Two other views around the dish, in held sessions only,
  removed 17 to 18 % at 17:30, when the sun stands about 32 degrees high against 13 at 07:00. No view tried around
  the dish had a distant shaft at noon.
- **The same-frame contrast has the wrong sign.** In held sessions at the 07:00 pose, with lit and shaded regions
  picked from the captures, (lit − shaded) / lit was −0.650 with the froxel sun shadow on (lit 0.0871, shaded
  0.1437) and −1.165 with it off: fog scatters more sunlight toward the camera near the sun, which outweighs the
  shadow. This is why PRD §10 Q14 compares the same pixels.

Three traps for the next capture. Two live sessions do not share a fog noise state, which reads as shadow; freeze
them (`--bench-mode renderer-stepped` with `BYRO_BENCH_HOLD_FREEZE_SIM=1`). `depth.dump` re-arms after each
success, so the first dump after a camera move belongs to the previous pose; discard it. In a frozen held session,
`pick` after a camera jump listed only distant-LOD entries in the Skyrim cell the script runs stream in; read what
loaded from the engine log, not from `pick`.

### Content shaft contrast from the checked-in script (VLM-SH-006, -012, 2026-11-27)

`exterior-shafts-content.sh all`, which now holds the five named views, their sample regions and the content contrast
rule (PRD §2, §10 Q15). Release build of the engine source at `19f46251a`, RTX 4070 Ti, 1280x720, TAA,
`renderer-stepped` with the static camera, cloud seed 0, each fixture's own weather, shipped haze floor: 50 captures,
exit 0, all five views pass.

| Fixture | Noon | Low sun | Contrast | Repeat | Repeat noise | Finished-frame pixels the shadow changes by > 4 of 255 |
|---|---|---|---|---|---|---|
| FNV | 0.0102 | 07:00 | 0.2346 | 0.2346 | 0.0000 | 16.0 % |
| FO3 | 0.0082 | 07:00 | 0.1351 | 0.1351 | 0.0000 | 38.5 % |
| Oblivion | 0.0000 | 17:30 | 0.1454 | 0.1454 | 0.0000 | 0.0 % |
| Skyrim | −0.0006 | 07:00 | 0.2294 | 0.2237 | 0.0057 | 13.7 % |
| FO4 | 0.0000 | 07:00 | 0.1539 | 0.1539 | 0.0000 | 25.8 % |

- FNV, FO3, Oblivion and FO4 give the scouting contrasts to four decimals.
- **Skyrim moves between runs.** Its four low-sun readings so far are 0.2346 and 0.2331 (scouting) and 0.2294 and
  0.2237 (this run), a spread of 0.0109, and this run's repeat noise is about four times the scouting figure. All are
  far above the 0.05 bar and twice either noise figure. The cause is not established; it is the one view whose
  occluder streams in after the first grid loads (all ten of this run's logs show it loaded before the screenshot).
- The verdict logic was checked against saved scouting captures: it reproduces the scouting values, and fails when
  the shadow-off capture equals the shadow-on one, when the repeat pair reads zero (noise as large as the value), and
  when the shadow-off region holds no in-scatter.

### Haze floor off at the content views (VLM-SH-011, 2026-11-27)

`BYRO_HAZE_FLOOR=off exterior-shafts-content.sh all` against the shipped-default run above: same engine binary and
views, 50 captures, exit 0. The engine logs nothing when it accepts `off`; that it took effect shows in Oblivion's
frame-mean in-scatter (noon 0.0547 default, 0.0411 off; 17:30 0.0251, 0.0111).

**All five views meet the content contrast rule with the floor off too**, so at these views the rule does not
depend on the floor.

| Fixture, hour | Contrast, floor off → default | Frame mean (of 255), off → default | Mean absolute change | Pixels changed by > 4 | Mean absolute change between floor-on runs |
|---|---|---|---|---|---|
| FNV 12:00 | 0.0099 → 0.0102 | 90.45 → 90.44 (−0.01 %) | 0.145 | 0.06 % | 0.001 |
| FO3 12:00 | 0.0082 → 0.0082 | 94.58 → 94.58 | 0.000 | 0.00 % | 0.000 |
| Oblivion 12:00 | 0.0001 → 0.0000 | 114.93 → 115.08 (+0.13 %) | 0.479 | 0.28 % | 0.000 |
| Skyrim 12:00 | −0.0005 → −0.0006 | 108.51 → 107.86 (−0.60 %) | 0.748 | 0.53 % | 0.325 to 0.941 |
| FO4 12:00 | 0.0000 → 0.0000 | 121.42 → 121.42 | 0.000 | 0.00 % | 0.000 |
| FNV 07:00 | 0.2585 → 0.2346 | 68.49 → 70.32 (+2.67 %) | 1.929 | 15.31 % | 0.000 to 0.088 |
| FO3 07:00 | 0.1351 → 0.1351 | 98.54 → 98.54 | 0.000 | 0.00 % | 0.000 |
| Oblivion 17:30 | 0.1560 → 0.1454 | 118.90 → 118.90 | 0.861 | 0.70 % | 0.000 |
| Skyrim 07:00 | 0.2269 → 0.2294 | 88.94 → 88.83 (−0.12 %) | 0.197 | 0.13 % | 0.192 to 0.314 |
| FO4 07:00 | 0.1539 → 0.1539 | 109.42 → 109.42 | 0.000 | 0.00 % | 0.000 |

Frame columns are the finished frame with the froxel sun shadow on. The last column covers the three pairs among
three floor-on runs (two scouting runs and the script run above).

**Which views the floor reaches.** The in-scatter view moves clearly on FNV 07:00 and on Oblivion at both hours. It
is identical on FO3 and FO4 at both hours, and on Skyrim at noon against the script's floor-on run (it differs from
the two scouting runs by 0.010 mean absolute, as that run does). Skyrim at 07:00 differs from the three floor-on runs
by 0.03 to 0.10 levels (mean absolute), while those differ among themselves by 0.02 to 0.07; one floor-off run
cannot separate that from run variation. Skyrim's finished-frame changes are within what its floor-on runs differ by.
FNV noon is not: its finished frame differs from each floor-on run by 0.144 to 0.145, faintly (1.04 % of pixels by more
than one level) and across much of the frame, 69 % of the summed difference in the left half where the dish stands,
although its in-scatter view barely moves (1.86 % of pixels, by one level). The cause is not established.

**Contrast falls where the floor strengthens the shaft.** Contrast is a share. In the sample region, the linear
radiance the shadow removes rises with the floor from 0.0347 to 0.0488 on FNV at 07:00 and from 0.0024 to 0.0055 on
Oblivion at 17:30, while contrast drops. In the finished frame, the pixels the shadow changes by more than 4 levels
go from 7.6 % to 16.0 % on FNV and stay at 0.0 % on Oblivion; FO3 (38.5 %) and FO4 (25.8 %) do not move, and
Skyrim reads 12.8 % off against 13.6 to 13.7 % in floor-on runs.

**Outside any shaft.** The PRD does not define which pixels are in a shaft. The working definition used here: with
the floor off or on, the froxel sun shadow removes more than a share T of the pixel's linear in-scatter and moves the
mapped view by more than one level. At noon it marks at most 0.3 % of any frame for T = 0.05, 0.10 and 0.20, so the
noon rows above are the outside-shaft change whichever T is used. At low sun it marks much of the frame and T
matters:

| Fixture, hour | Signed mean change outside, off → default, T = 0.05 / 0.10 / 0.20 | Share of frame marked as shaft |
|---|---|---|
| FNV 07:00 | +0.333 / +0.431 / +1.574 | 74.7 / 68.5 / 48.3 % |
| FO3 07:00 | 0.000 at all three | 55.1 / 26.8 / 7.6 % |
| Oblivion 17:30 | +0.102 / +0.085 / +0.069 | 18.8 / 17.3 / 12.5 % |
| Skyrim 07:00 | −0.041 / −0.062 / −0.079 (within run variation) | 52.9 / 34.9 / 13.7 % |
| FO4 07:00 | 0.000 at all three | 64.4 / 46.8 / 30.3 % |

**Against the accepted noon change.** At §10 Q12 the owner accepted the noon change measured at the default spawns
(Oblivion +3.6 % frame mean, none elsewhere). At these views the noon frame mean moves +0.13 % on Oblivion, −0.01 % on
FNV (the unexplained difference above), not at all on FO3 and FO4, and within run variation on Skyrim. These are
different views from the ones reviewed then. The outside-shaft and run-to-run figures come from one-off scripts, not
from the content script. **Accepted:** later on 2026-11-27 the owner accepted the noon change in this record, and
VLM-SH-011 is met.

### Haze floor after the authored fog power (Q12 re-measure, 2026-11-19)

PRD VLM-SH-011 asked for the floor-off / floor-on captures to be redone once the authored fog power entered the
ramp fit (#6776), because the shipped default (`0.0006 / m x (1 + 6 x (1 - sin e)^3)`) was chosen against the old,
thinner extinction. Release build at HEAD `a7d4b49bb` (docs-only commits since), RTX 4070 Ti, 1280x720, TAA,
`renderer-stepped` with the static camera, cloud seed 0, the `m-exteriors.sh` fixture cells at their default spawn.
Two measurements: the authored extinction against the floor at thirteen clock hours (`env.health` in a held
session, `time.set` then `env.health`: `gpu_extinction=` and `haze_floor=`), and floor-off against shipped-default
frames (`BYRO_HAZE_FLOOR=off`, `exterior-shafts-content.sh`).

**Does the floor still bind?** The floor is a maximum, so it only acts where it exceeds the authored value.
Hours sampled: 5, 6, 7, 8, 9, 10, 12, 14, 16, 17, 18, 19, 20.

| Fixture | Weather at the spawn | Authored `gpu_extinction` over the day | Floor binds | Floor / authored |
|---|---|---|---|---|
| FNV | `NVBlackMountainWeather` (region override) | 0.000968 to 0.001373 /m | 3 of 13: 07:00, 08:00, 19:00 | 2.00, 1.29, 1.79; 0.62 at noon |
| FO3 | `WastelandClearMegaton` | 0.004791 to 0.007922 /m | 0 of 13 | 0.33 at most (06:00) |
| Oblivion | `Cloudy` | 0.000432 to 0.000469 /m | 11 of 13: 06:00 to 19:00 | 1.39 (noon) to 5.06 (19:00) |
| Skyrim | `SkyrimCloudyTU` | 0.003347 to 0.004344 /m | 0 of 13 | 0.76 at most (06:00, 20:00) |
| FO4 | `CommonwealthClear` | 0.001129 to 0.001853 /m | 2 of 13: 06:00, 20:00 | 1.48, 1.36; 0.97 at 07:00 |

Oblivion is the one title whose `WTHR` records carry no fog power (VLM-HF-002), so its authored haze is still the
thin pre-Phase-1 value and the floor supplies its haze all day. On the other four the fitted power has made the
authored haze thicker than the floor except near the horizon.

**What it does to the frame, floor off against shipped default** (same pose and process settings; the repeat noise is
zero in this mode, and FO3 comes out bit-identical, which is the control):

| Fixture | Hour | Frame mean off to default | Mean absolute change (of 255) | Pixels changed by more than 4 levels | In-scatter mean off to default |
|---|---|---|---|---|---|
| Oblivion | 12:00 | 55.84 to 57.85 (+3.6 %) | 2.01 | 21.1 % | 0.0394 to 0.0518 |
| Oblivion | 07:00 | 46.01 to 60.04 (+30.5 %) | 14.06 | 98.2 % | 0.0291 to 0.1038 |
| Oblivion | 17:30 | 39.18 to 40.58 (+3.6 %) | 1.48 | 16.6 % | 0.0101 to 0.0211 |
| FNV | 12:00 | 66.57 both | 0.000 | 0 % | 0.0205 both |
| FNV | 07:00 | 23.89 to 25.97 (+8.7 %) | 2.08 | 1.4 % | 0.0222 to 0.0444 |
| FNV | 17:30 | 28.06 both | 0.000 | 0 % | 0.0367 both |
| FO3 | 12:00, 07:00, 17:30 | identical | 0.000 | 0 % | identical |

The in-scatter ratios follow the extinction ratios (FNV 07:00 floor/authored 2.00, in-scatter x2.0; Oblivion 07:00
5.00, x3.6), so the table is the floor acting and nothing else.

**What this does not measure.** Shaft contrast on content (VLM-SH-006) still needed the depth-matched poses of
VLM-SH-012 at the time (named and recorded 2026-11-27, above), so no capture here says whether the floor makes a content shaft visible. Skyrim never binds, so it was
not captured; FO4 binds only at 06:00 and 20:00, which were not captured; Oblivion and FNV were captured at three
hours. The PRD's rev 20 figures (FNV Goodsprings noon 0.000582 to 0.002704 /m) are for a different weather than this
fixture's, which now resolves to Black Mountain through the region table; the two are not reconciled here.

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
  values. The fit consumes distance, maximum opacity, power and the near
  height range; the
  [field disposition table](#fog-and-god-ray-field-disposition-vlm-hf-009)
  lists every other field.
- A raw recursive survey found zero `VOLI` records in `Fallout4.esm` and all
  six official Fallout 4 DLC masters (`DLCRobot`, `DLCCoast`, `DLCNukaWorld`,
  and the three workshop masters). VOLI therefore cannot calibrate Fallout 4
  defaults from this corpus. The reusable probe lives at
  `crates/plugin/examples/dump_voli_subs.rs`.
- The authored fog power's curve form is read from shipped bytecode (2026-11-07, `scripts/sdp-fog-term.py` over
  FNV's `Shaders/shaderpackage010.sdp`): 121 `vs_2_0` shaders share one fog sequence, `fog = (1 - clamp((FogParam.x - d)
  / FogParam.y, 0, 1)) ^ FogParam.z` (`add rcp mul max min add log mul exp`), the fog colour passing through unchanged.
  With `x` = far, `y` = far - near and `z` = power that is `saturate((d - near) / (far - near)) ^ power`. `d` is the
  length of the clip-space xyz before the divide. The component roles are inferred from the arithmetic; the shader
  does not name them.
- God-ray and height-fog census, 2026-11-07
  (`crates/plugin/examples/fog_godray_census.rs`, which extends the VOLI-only
  probe's survey):
  - Skyrim SE `VOLI` records and the `WTHR` `HNAM` references to them live in
    `Update.esm` (36 `VOLI`, 69 weathers each carrying a 16-byte `HNAM`, 64 of 69 with every slot
    non-null), `Dawnguard.esm` (1 and 15), `Dragonborn.esm` (5 and 9); `Skyrim.esm` and
    `HearthFires.esm` have none. Dawnguard names 9 distinct `VOLI` of which 1 resolves
    in its own file, so the rest resolve in `Update.esm`.
  - Every `VOLI` is a 13-subrecord record of single floats (`CNAM` to `NNAM`). Across the 42 records
    `Custom Color - Contribution` is 0, `Density - Contribution` 0.3, `Density - Size` 10, `Density -
    Wind Speed` 15, `Density - Falling Speed` 0.3 and `Sampling Repartition - Range Factor` 40 in every record;
    only `Intensity` (1.0 to 4.0, 7 distinct in `Update.esm`) and the two phase fields
    (`Contribution` 0.70 to 0.85, `Scattering` 0.70 to 0.85) vary.
  - Fallout 4 has no `VOLI` and no `WTHR` `HNAM`. Its god rays are `GDRY` (61 records, `DATA` always 60 bytes: back
    colour, forward colour, intensity, air / back / forward scale, back phase, air colour, forward phase). They
    are referenced by 63 weathers' 32-byte `WGDR` (8 time-of-day slots, 40 of 63 weathers non-null; 54
    distinct `GDRY`), by 7 cells' `XGDR` and by 20 lights' `WGDR`; no `LGTM` carries one. `Intensity` runs 0 to 50
    (mean 3.5), back colour scale 0.01 to 18, forward colour scale 0.01 to 3, air colour scale 0.001 to 1,
    forward phase 0.22 to 0.92.
  - Authored fog power is not an edge case: 140 of 168 day/night values in `Skyrim.esm`'s 84 weathers
    and 116 of 142 in `Fallout4.esm`'s 71 are not 1.0 (means 0.51 and 0.48, minima 0.20 and 0.07).
  - FO4's height-fog extension is on 63 of 71 weathers, in 16 distinct day tuples. Near height mid is 0 to
    1004, near height range 1,000 to 25,000, high density scale 0.90 to 1.00 (5 values), far height mid -6,400
    to 12,000, far height range 10,000 to 15,000 (2 values). The commonest tuple (24 weathers, including
    `FXNukeWeather`) is (0, 10000, 1.00, 0, 10000): the neutral default. The semantics of the layers and the scale
    stay unverified: a census cannot give them, a matched capture of two weathers that differ only in
    those fields can.
- FNV `FreesideAtomicWrangler` contains two alpha-over
  `fxsmokewispsthin01.dds` particle emitters under generic
  `SuperSpray01-Emitter` hosts. Both become `FogVolume` entities in the live
  ECS; the old sprite texture is no longer uploaded for them.

### Fog and god-ray field disposition (VLM-HF-009)

State at HEAD on 2026-11-26, traced from the parser structs to
`byroredux/src/env_translate/sky_weather.rs`, `components/environment.rs`
(`CellLightingRes::from_cell_lighting`, the interior cell path),
`fog.rs` and `render/mod.rs`. **Medium**: reaches `FogMedium`. **Tint**:
reaches the renderer as the fog colour, outside `FogMedium`. **Excluded**:
not consumed, with a recorded reason. **Not ruled**: not consumed, and no
requirement, decision or verify item covers it yet.

| Source and field | Disposition | Reason or path |
|---|---|---|
| `WTHR` fog day / night near and far | Medium | `FogMedium::from_authored_fog` |
| `WTHR` fog day / night power | Medium | Same (VLM-HF-001) |
| `WTHR` fog day / night maximum opacity | Medium | Same |
| `WTHR` FO4 / FO76 / Starfield `FNAM` day / night near height range (`WeatherHeightFog`) | Medium | `with_authored_height_range`, through the same function |
| `WTHR` fog colour (`NAM0` fog group) | Tint | `fog_colour_radiance` in `render/mod.rs` |
| `WeatherHeightFog` day / night near height mid | Excluded | The shader anchors the profile to the measured ground near the camera (`with_authored_height_range` doc comment, #3956) |
| `WeatherHeightFog` day / night far height mid and range | Excluded until #6771 | Semantics unverified (VLM-HF-003) |
| `WeatherHeightFog` day / night high density scale | Excluded until #6771 | Semantics unverified (VLM-HF-003) |
| Skyrim `NAM0` "fog far" colour group (in `skyrim_extra_colors`) | Not ruled | — |
| `XCLL` / `LGTM` fog near and far, power, maximum opacity (interior cells) | Medium | `CellLightingRes::from_cell_lighting` |
| `XCLL` / `LGTM` fog colour (interior cells) | Tint | `fog_colour_radiance` |
| `XCLL` / `LGTM` fog far colour | Not ruled | Carried on `CellLightingRes`; read by diagnostics only |
| `XCLL` / `LGTM` fog clip | Excluded | Part of the legacy cubic-fog curve, which the physical path does not evaluate (`render/mod.rs` comment; VLM-NG-003) |
| FO4 / FO76 / Starfield cell height tail: near height range | Medium | `height_fog_tail()` into `from_authored_fog` (VLM-HF-005) |
| Cell height tail: near height mid | Not ruled | The weather field's reason above has not been recorded for the cell field |
| Cell height tail: far height mid and range, high density scale | Excluded until #6771 | Semantics unverified (`HeightFogTail` doc comment, VLM-HF-003) |
| Cell height tail: two high-altitude fog colours | Not ruled | — |
| Cell height tail: fog near, far, high near and high far scales | Not ruled | — |
| `VOLI`: all decoded fields (intensity, custom colour and contribution, four density fields, two phase fields, sampling range factor) | Excluded | No source settles their meaning (VLM PRD §10 Q8) |
| `GDRY`: all decoded fields (back, forward and air colour, intensity, three scales, two phases) | Excluded | Same (§10 Q8) |
| Links: `WTHR` `HNAM` and `WGDR`, `CELL` `XGDR`, `LGTM` `WGDR`, `LIGH` `WGDR` and god-ray near clip | Excluded | Same (§10 Q8) |

The "Not ruled" rows keep VLM-G-002 open. The PRD's profile field coverage
ratio is unpublished: its definition counts fields reaching `FogMedium` and
does not say how to count the **Tint** rows.

## Follow-up boundary

1. calibrate source, decay, boundary-slip, and field-light reduction against
   shipped fire, smoke, and explosion content. The solver already has
   conservative no-through solid blocking, triangle-normal tangential slip,
   mild wall friction, pressure response, and canonical `FogProfile` presets;
   it has no game or authoring-provenance branch;
2. extend authored-mesh replacement to the loose-NIF route and add the optional
   tri-planar 2D-mask density path for silhouettes that need texture fidelity;
3. consume more of the Starfield height-fog block than its near height range
   (shipped, VLM-HF-005) without guessing its curve
   ([field disposition](#fog-and-god-ray-field-disposition-vlm-hf-009));
4. add the 32³ aerial-perspective LUT only if VLM-AP-001 fails with the
   analytic tail (VLM PRD §10 Q4). A non-RT cascade visibility variant is a
   non-goal (VLM-NG-006);
5. extend the existing glass transmittance hook with ratio tracking and a
   majorant grid for path-traced heterogeneous media.

## M55 requirement records

Measurement history moved here word for word from the
[Volumetric Lighting PRD](../prd/volumetric-lighting.md) on 2026-11-26 (its revision 38), so that its
requirements state only what must hold. Each record is dated by its own text; where a record states a
gate or a status, the PRD's requirement is the current one.

### Far height layer census and capture pairs (VLM-HF-003)

**Census, 2026-11-07** (`fog_godray_census`): 63 of 71 `Fallout4.esm` weathers carry the layer, in 16 distinct day tuples; the commonest (24 weathers, e.g. `FXNukeWeather`) is the neutral default (near mid 0 / range 10,000, density scale 1.00, far mid 0 / range 10,000); the authored spread is near mid 0 to 1004, near range 1,000 to 25,000, density scale 0.90 to 1.00, far mid -6,400 to 12,000, far range 10,000 to 15,000 (two values). The semantics are still unverified and need the matched captures (filed as #6771). Those captures have to be of the original engine: the fields are unread here, so captures from this engine cannot differ. The census finds the pairs to observe, same distance ramp, power and maximum opacity and a different height tuple: `CommonwealthClear` (`0002B52A`) against `CommonwealthClearestSkies` (`001D670E`), which differ only in high density scale 0.95 against 0.90, and `CommonwealthGSFoggy` (`001BD481`) against `CommonwealthGSRadstorm` (`001C3D5E`), which differ only in far height mid 12,000 against 64 (their colours and clouds also differ).

### Grid seam cause, re-measurement and baseline (VLM-HF-007)

The cause was two disagreements between the grid and the tail, both fixed: the grid multiplies the medium by a noise factor (`proceduralDensityScale`, `mix(0.2, 1.35, density)`) whose mean at the default coverage is below 1 while the tail integrated the unmodulated coefficient (the host now passes the tail `fog_mean_density_scale(coverage)`, computed from the noise volumes' histograms), and the tail's height term did not clamp at the reference height as the grid's does (`heightFogOpticalDepth` now integrates the clamped profile piecewise). Re-measured with `fog-grid-seam.sh` (128 m against 512 m, repeat noise ~0): noon FNV hills right -14.2 to -1.8 (earlier build: -5.6 to -1.8), Skyrim terrain haze -4.2 to -0.1, sky +3.7 to +0.3, and at night FNV hills left +5.3 to +0.6, sky band +5.5 to +0.04; every region now moves at most about 2 levels. The literal bar (no more than the repeat's noise, which is zero in `renderer-stepped`) cannot hold where the grid resolves real shadows beyond 128 m, so the gate is a tolerance of 2.5 levels (`SEAM_TOLERANCE`), decided at §10 Q11. **Baseline at HEAD before Phase 4, 2026-11-07 (`docs/smoke-tests/fog-grid-seam.sh`): the requirement did not hold.** In `renderer-stepped` mode a repeat capture differs by at most 0.004 levels per region, and moving the grid from 128 m to 512 m shifts the distant regions by: FNV Goodsprings, hills right -5.56, hills left +0.18, sky band -0.63, whole frame -0.54; Skyrim Whiterun elevated pose, terrain haze -4.18, far ridge +6.02, forest +0.02, sky +4.83, whole frame +0.42. The regions are windows over distant content (no depth view exists), not a depth mask.

### Shaft range sweep (VLM-SH-005)

**Measured 2026-11-07** (`--shaft-lab-distance`, default 128 m grid, the gate's 0.05 bar, open minus solid contrast): 25 m 0.233, 40 m 0.237, 55 m 0.154, 70 m 0.131, 90 m 0.071, 110 m 0.031, 128 m 0.004, 150 m 0.001. The shaft passes out to somewhere between 90 and 110 m and is gone at the grid's far plane, so the observable range is about 90 to 100 m of the 128 m grid. The owner accepted this range at §10 Q10 (2026-11-19).

### Cutout card measurement (VLM-SH-009)

**Phase 0, measured 2026-11-07:** cutout 116.36, geometry 129.16, solid 93.40, a 9.9 % gap, so it failed with the opaque traversal, as predicted. **Met the same day** by the alpha-aware sun trace (§3 row 17): cutout 129.18, geometry 129.16, solid 93.40, a gap of 0.0 %. The criterion this requirement first carried, shaft contrast behind cutout texels against behind opaque texels, is withdrawn: it reads 0.106 on an opaque trace, because a view ray through a cutout also sees the lit haze beyond the panel and a ray that ends at an opaque texel does not. Only a see-through reference that is real geometry removes that difference.

### Haze floor re-measure and decision (VLM-SH-011)

**Re-measured 2026-11-19** ([fog design](procedural-volumetric-fog.md#haze-floor-after-the-authored-fog-power-q12-re-measure-2026-11-19)): the floor binds on 0 of 13 sampled hours on FO3 and Skyrim, 2 on FO4, 3 on FNV and 11 on Oblivion, whose records carry no fog power. Where it binds the frame moves (Oblivion 07:00 +30.5 % frame mean, noon +3.6 %; FNV 07:00 +8.7 %); where it does not, floor-off and shipped frames are identical. The owner reviewed those captures and decided at §10 Q12 (2026-11-19) to keep the floor and its default; the noon non-shaft change they accepted is the measured one (Oblivion +3.6 % frame mean, FNV, FO3 and the other fixtures none).

### Live beam-converter coverage (VLM-INT-006)

Found on 2026-11-07: the `fxlight` early return in `synth_child.rs` kept every FNV `FXLightBeam*` placement from loading its NIF, so all FNV beam converters measured 0 live until the return was narrowed (`Vault11a` 14 ÷ 14, `SLAbandonedShack03INT` 5 ÷ 5 afterwards). **Denominators, 2026-11-07:** placed beam references for every official master and DLC are in [interior-shaft-openings](interior-shaft-openings.md#placed-beam-census-every-title-and-official-dlc-vlm-int-006-denominators-measured-2026-11-07); Fallout 4 is the one title with many beams in Show Sky cells (657 of 1,604 interior placements), and, from #6774 (2026-11-08), every title has one: FO4 about 100 % (all 9 placed models), Oblivion 92.9 % (the #6731 gate now includes it; 66 volumes live in a 52-placement cell), Skyrim 0 of 1, Starfield 0 of 1 ([interior-shaft-openings](interior-shaft-openings.md)).

### Interior shaft dust measurement (VLM-INT-007)

Measured on FNV `NellisHangar1` (#6727): the converted planes are thin ribbons (about 8 m x 1.4 m), and without the dust the noon-minus-midnight radiance difference is about 0 over six cameras; with 0.04 / m it is 3.6 to 7.3 % of pixels above 3 levels (max 19), and in the shaft region noon reads 28.3 against 21.6 at midnight (repeat noise 0.16).

### Shaft range gate at 90 m (VLM-SH-005)

Added 2026-11-26, after the records above were moved here. `exterior-shafts.sh` now runs the lab gate at 90 m
as well as 55 m (release build, RTX 4070 Ti, 1280x720, TAA, `renderer-stepped`, static camera, cloud seed 0).

| Distance | Open contrast | Solid contrast | Repeat noise |
|---|---|---|---|
| 55 m, rows 335-519, columns 375-904 | 0.1542 | 0.0018 | 0.000000 |
| 90 m, rows 404-516, columns 479-800 | 0.0731 | 0.0009 | 0.000000 |

The 90 m crop is the 55 m one mapped onto the wall's front face and projected into the 90 m camera (45 degree
vertical FOV, pitched up the sun's 10 degrees); the same projection puts the outer slat edges at columns 375 and
905 at 55 m, which are the 55 m crop's own bounds. The 55 m crop cannot be reused: at 90 m it takes in the sky
above the wall and open haze beside it, and the solid wall then reads 0.0290, over its 0.02 bar. Growing the
90 m crop by 3 px or insetting it by up to 16 px a side leaves all three conditions passing (open 0.0724 to
0.0780).

