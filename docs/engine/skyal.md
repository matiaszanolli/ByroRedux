# SKYAL — Sky Abstraction Layer

**Status: PARTIAL (2026-10-20).** Shared sky evaluation, volumetric clouds,
the sky cubemap, and GGX-prefiltered reflection mips are implemented.
Clouds and local fog/combustion now share the phase-function and slab
integration implementation. Diffuse sky irradiance now comes from the baked
sky; weather-driven cloud morphology and a palette-preserving atmospheric
approximation are implemented. Since 2026-10-20 the sky composites far to
near — celestials, the games' own cloud textures on a 9 km upper shell, the
low volumetric deck — with physical aerial perspective (§2.4), the deck
decides coverage per column, and surfaces receive position-dependent cloud
shadows instead of a weather-wide sun dimming (§2.5). A full atmospheric LUT
and explicit authored cloud taxonomy remain unfinished. See §3.

Sibling of [NIFAL](nifal.md), [EXAL](exal.md), [PHYSAL](physal.md),
[WATAL](watal.md), [CHARAL](charal.md). SKYAL sits **downstream of EXAL**:
EXAL already translates per-game WTHR/CLMT into the canonical
`SkyParamsRes`, so everything here is game-agnostic by construction. Do
not add a per-game branch to any of it — that is what the EXAL boundary
is for (see `feedback_format_translation`).

---

## 1. Why this layer exists

### 1.1 The measured defect

Skyrim SE, Tamriel 5,-24, weather `SkyrimCloudy`, deterministic capture
(`--camera-pos 5000,10500,-49000 --camera-forward 0.2,0.15,-1.0
--bench-frames 180 --screenshot`), reproducible run-to-run to a mean of
0.078/255.

| Quantity | Authored (WTHR) | Rendered (pre-fix) |
|---|---|---|
| zenith RGB | `0.161, 0.380, 0.459` | `0.572, 0.822, 0.914` |
| zenith R/B (hue) | 0.35 | 0.81 |
| zenith→horizon gradient | large | ~4% |

The sky was a flat near-white wash. Decomposition through the
`composite_term` debug view (pre-bloom, pre-tonemap, linear) isolated it:

* pre-bloom `(0.335, 0.477, 0.539)` — **matches the analytic prediction**
  `mix(horizon, zenith, sqrt(sin(elev)))` for that view direction, i.e.
  the sky assembly was correct;
* post-bloom `(0.572, 0.822, 0.914)` — a flat **1.71×** on all three
  channels.

Bloom had no bright-pass anywhere in its chain. Over a large uniform
region a bloom pyramid's "local blurred average" converges to that
region's own value, so `scene += bloom * BLOOM_INTENSITY` degenerates
from a highlight glow into a **pure gain** — and the sky is the largest
uniform bright region an exterior frame has. ACES then compressed the
result into its shoulder, which is what destroyed the gradient and the
hue. The sky was not too bright; it was being multiplied into the part of
the curve that cannot represent it.

**Fixed** (`62a09fd9`) by a Karis/Jimenez soft knee at the pyramid seed.
Same capture after: `(0.424, 0.550, 0.591)`, a 1.01× bloom gain. See
`BLOOM_THRESHOLD` in `shader_constants_data.rs` for the constants and the
deliberate emissive trade-off.

### 1.2 What is still wrong

Fixing the gain does not make the sky good, because the sky *model* is a
two-colour vertical lerp:

* **No atmosphere.** `mix(horizon, zenith, sqrt(t))` cannot produce a
  real sky. There is no Rayleigh/Mie separation, so no sun-relative
  azimuthal variation at all: the sky is identical 180° away from the sun
  as toward it.
* ~~**Clouds are UV-projected planes.**~~ Up to four WTHR cloud textures
  were projected onto an infinite plane (`dir.xz / dir.y`), streaking
  toward the horizon behind a `smoothstep(0, 0.12, elevation)` fade.
  **Resolved:** the volumetric deck (§2.3) is the low cloud body and the
  textures sit on a curved upper shell behind it (§2.4).
* **Reflections and GI never see the sky.** `raytrace.glsl`'s miss
  returns `skyTint * 0.5 + sceneFlags.yzw * 0.5` — a single flat colour.
  Every sky reflection and every sky-lit indirect bounce in the engine is
  currently a uniform blob, regardless of where the sun is or what the
  clouds are doing.

---

## 2. Architecture

```
EXAL (per-game WTHR/CLMT)  →  SkyParamsRes  →  SkyDome  →  sky_radiance()
                                                              │
                                            ┌─────────────────┴──────────────┐
                                            │                                │
                                   background pixel                   sky cubemap bake
                                   (composite.frag)                   (sky_cube.comp)
                                                                             │
                                                    ┌────────────────────────┼─────────────┐
                                                    │                        │             │
                                              RT miss / GI          rough reflections   ambient
                                             (triangle.frag)         (prefiltered mips)   (SH)
```

### 2.1 Shared sky — LANDED (`b7abdaa5`)

`shaders/include/sky.glsl` holds the one implementation, parameterised by
a `SkyDome` value rather than a uniform block, because the consumers do
not share a descriptor set. Verified behaviour-identical to the inlined
original: mean 0.0122/255 final, 0.0262 pre-bloom, against a 0.0777
noise floor.

`sky_dome.rs` guards the seam's one silent failure mode: GLSL neither
zero-initialises a local struct nor warns about a field a builder forgot,
so a missed field is undefined data, not a diagnostic.

### 2.2 Sky cubemap — LANDED

**Why a cubemap rather than calling `sky_radiance` from the RT path.**
Three independent reasons, in increasing order of weight:

1. Cost: one evaluation per texel (6 × 128² ≈ 98k) instead of per ray.
2. Prefiltering: mip level selects roughness for glossy reflections;
   there is no other way to get that from an analytic sky.
3. **It is what makes volumetric clouds affordable at all.** A raymarched
   cloud layer cannot be evaluated per RT ray or per background pixel.
   Baked once per frame into a cubemap, it can.

There is also a hard plumbing reason: `triangle.frag` binds the bindless
arrays at **set 0**, `composite.frag` at **set 1**. `include/sky.glsl`
declares `textures[]` at set 1, so it cannot simply be included into
`triangle.frag`. A cubemap is one `samplerCube` sample with no sky code
and no set conflict.

**Shape.**

* `R16G16B16A16_SFLOAT` cube, 6 faces, 128² (tunable), `CUBE_COMPATIBLE`,
  with a mip chain for roughness prefiltering.
* Two views: `TYPE_2D_ARRAY` (storage, for the bake) and `CUBE`
  (sampled). `GpuImageDesc` needs a `flags` field and a `color_cube`
  constructor — extend it, do not add a second image path.
* `sky_cube.comp`: dispatch `(N/8, N/8, 6)`, one invocation per texel,
  `imageStore(skyFaces, ivec3(x, y, face), vec4(sky_radiance(dome, dir), 1))`.
* Bound at **scene set 1 / binding 20**, not into the `cubemaps[]`
  bindless array. The bindless route was rejected: `TextureRegistry`
  entries own their `Texture`, so an externally-owned per-frame view would
  need a new ownership kind threaded through the destroy path *and* would
  have to respect the fence-gated descriptor-write protocol (#92 / #2715).
  Binding 20 follows the `aoTexture` / `depthHistoryTex` precedent — a
  renderer-owned per-frame image written into the scene set once at init —
  which is simpler and has no ownership ambiguity.
* `GpuCamera::exterior_sky_tint`'s previously-reserved `w` lane carries a
  **ready flag**. The bake is optional (VRAM pressure) and binding 20 is
  PARTIALLY_BOUND, so consumers must gate on the flag rather than on the
  binding existing.
* The bake needs **two** descriptor sets: its own, plus the bindless array
  at set 1, because `include/sky.glsl` samples the WTHR cloud layers and
  the CLMT sun sprite by index. The bindless layout's `stageFlags` also
  had to gain `COMPUTE`. Both are accepted by the shader compiler and
  rejected at pipeline creation
  (VUID-VkComputePipelineCreateInfo-layout-07988) — validation-layer-only
  failures, invisible to `cargo test`. Both were hit for real.

**Face mapping** — derived from the Vulkan spec §16.5.4 cube-map face
selection table by inverting it (`|ma| = 1`, `u = 2s-1 = sc`,
`v = 2t-1 = tc`), not guessed:

| face | major axis | direction from (u, v) |
|---|---|---|
| 0 | +X | `( 1, -v, -u)` |
| 1 | -X | `(-1, -v,  u)` |
| 2 | +Y | `( u,  1,  v)` |
| 3 | -Y | `( u, -1, -v)` |
| 4 | +Z | `( u, -v,  1)` |
| 5 | -Z | `(-u, -v, -1)` |

**Consumers, in the order they should be wired:**

1. `raytrace.glsl` miss — replaces the flat `skyTint * 0.5 + ambient *
   0.5`. Largest visible win; reflections and sky GI become real.
2. `lighting.glsl`'s bounded-path escape — same substitution.
3. Rough reflections via prefiltered mips.
4. Diffuse ambient via an irradiance projection, replacing the
   hand-rolled `sceneFlags.yzw` term.

The **background stays analytic**. A 128² face cannot represent a sharp
sun disc, and the background pixel can afford the full evaluation. Once
volumetric clouds exist, the background samples the cubemap for the cloud
layer and keeps the disc analytic on top.

### 2.3 Volumetric clouds — LANDED, VISIBLE (`564d0d2f`, `9ac8a929`, `5d5d6ddd`)

`include/clouds.glsl` is the cloud body `sky_radiance` paints, so the
composite background (per clear-depth pixel) and the sky-cube bake (per
texel) draw the same clouds. It replaced the 2D procedural FBM body in the
same slot, on the same coverage lane. Ray-traced rays never march it; they
sample the baked cube.

**Shape** follows Schneider & Vos, "The Real-Time Volumetric Cloudscapes of
Horizon: Zero Dawn", SIGGRAPH 2015: a spherical-shell layer (1.5-5 km),
Perlin-Worley base remapped by coverage (subtracted, not multiplied),
Worley edge erosion, and a height gradient. The density volumes are the
two `volumetrics/noise.rs` already generates, uploaded once into a
context-owned `CloudNoiseVolumes` that both consumers bind. Composite is
mandatory, rebuilt on resize, and not PARTIALLY_BOUND, so the volumes can
belong to neither pipeline.

**Lighting** follows Hillaire 2016, "Physically Based Sky, Atmosphere and
Cloud Rendering in Frostbite" (SIGGRAPH 2016 PBS course notes):

| Term | Value | Source |
|---|---|---|
| Phase | two-lobe HG, g0 0.8, g1 -0.5, blend 0.5 | Hillaire slides 36/38 |
| Ambient | sky mean over both hemispheres, exact for this sky gradient (2/3, 5/6) | Hillaire §5.5.1 |
| Integration | energy-conserving analytic slab | Hillaire §5.6.3 |
| Multiple scattering | N = 2 octaves: scattering × aⁿ, extinction × bⁿ, eccentricity × cⁿ | Hillaire §5.8 Eq. 19-20 (after Wrenninge et al. 2013); N from Fig. 40 |
| a, b, c | 0.5 each | **No primary citation.** Skybolt's open-source value, adopted with the project owner's sign-off (2026-09-13). Satisfies a ≤ b. |
| Extinction | 0.12 m⁻¹ at density 1, albedo 1 | Top of the cumulus range, Hess, Koepke & Schult 1998, as cited in Hillaire §5.2 |
| Powder | not used | Schneider's term needs a view-dependent gradient the source leaves unspecified |

**Calibration.** The sun term is `SkyParams::sun_illuminance`, the same
`compute_directional_upload` value surfaces are lit by — not the sun-disc
colour times its raw 0-4 scale. This engine's diffuse compensates the
BRDF's 1/π, so a white surface facing the sun has radiance E. Hillaire
states a dense cloud "should converge to what an opaque diffuse surface
would look like". Any normalised phase averages 1/(4π) over the sphere,
so matching the white surface on average fixes K = 4π / Σaⁿ. The phase
keeps its angular shape: the silver lining exceeds E, the sun-away side
falls below it.

Before calibration the clouds rendered darker than the sky in every
measured case (luminance 0.21-0.31 against 0.525): single scattering with
a normalised phase was lit by a sun far too weak relative to the
LDR-authored WTHR sky.

**Sampling.** Both marches were aliasing at the sourced extinction:

* View march: Schneider's adaptive scheme (slides 74-80). Cheap base-shape
  samples run at `ray_length / mix(128, 64, dir.y)` until the iso-surface.
  The march then steps back once, never behind integrated distance, and
  switches to full samples of one mean free path at the local density. A
  zero full sample returns to cheap mode. The iteration bound of 512 is
  derived and never truncates. The fixed 48-step march it replaced had
  optical depth ~9 per step. Pinning the jitter to 0.5 turned the grain
  into terraces, which proved the steps — not the upscaler — were the
  cause.
* Self-shadow march: six samples (Schneider slide 89), spaced
  geometrically (Hillaire §5.5.2) from one mean free path out to the
  shell top along the sun. The six even 583 m steps it replaced produced
  the flat cyan-grey undersides that had been blamed on the two-octave
  limit.

**Cost** at 1280×720 render: `gpu_composite` ~1.2 ms on a cloud-filled
view and 0.7-1.2 ms over terrain (large run-to-run variance);
`gpu_sky_cube` ~0.1 ms before the cloud-shadow bake existed. With it
(2026-10-21, RTX 4070 Ti, TAA, FNV, sun up, coverage 0.4): `gpu_sky_cube`
0.21-0.23 ms inclusive, of which the nested `gpu_cloud_shadow` bracket is
0.034 ms, the same in an interior as in an exterior since the map is fixed
size (#6316). The march scales with pixel count, so the worst
view at 4K is roughly 10 ms.

Still open:

* ~~Faint horizontal striation in undersides~~ — **resolved.** Fixed
  light-sample distances were sweeping through the noise field. Each
  sample now slides within its geometric cell by `ratio^(jitter − 0.5)`,
  relying on temporal jittering as Hillaire §5.5.2 does; the bake passes
  jitter 0.5, so its offset is zero. Row striation on the reference crop
  dropped from 0.182 to 0.136; edge high-frequency energy from 0.72 to 0.64.
* **Cloud type** (stratus / cumulus / cumulonimbus). The mapping from WTHR
  classification flags is undetermined and must be measured. A single
  cumulus-band profile is used for every weather.
* ~~**Coverage mapping provenance.**~~ **Documented as an engine choice
  (W3.14 census, 2026-09-18).** Coverage is `SkyDome::weather_aurora.z`,
  derived in `env_translate::fog_coverage_from_weather` as a fixed
  0.86 / 0.80 / 0.70 / 0.40 / 0.55 per classification flag. The census
  (`crates/plugin/examples/weather_coverage_census.rs`) established that
  no authored scalar exists to derive it from: the 2D engines' coverage
  emerged from the cloud textures' own alpha channels multiplied by the
  JNAM per-layer brightness multipliers, which do not reduce to a scalar
  (32 of 84 `Skyrim.esm` weathers author no JNAM at all; the 16 authored
  `pleasant` records average a max-layer alpha of 0.99 — the one weak
  directional signal is their layer-0 mean of 0.78 vs 1.0 elsewhere).
  Retuning the constants against that signal would be a guess; the
  mapping stands as a defensible engine value like `SUN_SOUTH_TILT`,
  with the texture-alpha provenance recorded here as its citation.
* **Noise shape frequencies** (`0.00008` / `0.0009` per metre) predate the
  sourced pass and are not yet justified.
* **In-shader cloud-shape literals are uncited** (#4314, 2026-09-14 audit
  D18-03; re-confirmed 2026-09-23). New with the SKYAL volumetric pass and
  sourced nowhere — Schneider & Vos's height-gradient discussion states no
  breakpoints:
  * `cloud_height_gradient` remap breakpoints `0.15`/`0.06` (bottom end)
    and `0.55`/`0.78` (top start, fair/storm mix);
  * erosion strength `erosion * 0.45` and the erosion height blend
    `height_fraction * 5.0`;
  * detail-noise advection `wind_offset * 3.0`;
  * cloud-march wind scale `time * 0.00002`.
  These shape silhouettes and drift speed; treat them as engine-tuned
  values pending a citation, not as reference values from the paper. The
  carried-over 2D literals (tint `0.45`/`0.08`, alpha `0.78`/`0.96`,
  horizon fade `0.015/0.16`) ARE verbatim from the pre-volumetric
  `composite.frag` body and stay cited by that provenance.
* **Rendered-march acceptance is manual** (2026-09-19): unit tests pin the
  coverage → density source shape and `m-exteriors.sh cycle` only image-healths
  its PNGs — no gate checks a cloud invariant in pixels, so the cloudscape
  verdict is human-only (EXT-D7-2026-09-19-06 item 3).

### 2.4 Far-to-near composite, upper cloud plane, aerial perspective (2026-10-20)

**Order.** `sky_radiance` composites far to near, each cloud layer by its own
transmittance: the clear sky (gradient, Rayleigh/Mie structure, sun glow) →
the sun disc, stars, moon and aurora → the **upper plane** → the **volumetric
deck**. Before, the deck was drawn first and the four WTHR textures were
painted *over* it, so the games' high layers sat nearer than 1.5 km cumulus,
and the disc, glow and night sky were dimmed by fixed shares (0.94 / 0.82) of
an accumulated alpha. Those factors are gone (`sun_visibility`,
`glow_visibility`, `sky_visibility` are pinned absent by
`cloud_layers_composite_far_to_near_over_the_celestial_sky`).

**Upper plane.** The authored layers are drawn on a spherical shell at
`CLOUD_UPPER_PLANE_ALTITUDE` = 9 km — the midpoint of the WMO high étage
(cirrus, 5–13 km in temperate regions), above `CLOUD_LAYER_TOP`. The shell has
no projection singularity: toward the horizon its UVs converge to
√(2R/h) ≈ 38 tile units, and at the zenith they equal the old plane's, so
`tile_scale` keeps its authored meaning. The analytic mip is
`0.5·log2(distance / altitude)`, which equals the old `0.5·log2(1/elevation)`
above ~3°.

**Aerial perspective.** Air between the viewer and a layer dims it and adds
in-scattered sky light in front of it. Extinction is Hillaire 2020's Earth
atmosphere (Rayleigh (5.802, 13.558, 33.1)·10⁻⁶ m⁻¹, 8 km; Mie 3.996·10⁻⁶ +
4.40·10⁻⁶ absorption, 1.2 km); the in-scattered colour stays the authored WTHR
sky's, so palettes are kept. The column integral is the closed-form Chapman
integral over a curved ground, `√(πRH/2)·[erfcx(x₀) − e^(x₀²−x₁²)·erfcx(x₁)]`,
with erfcx from Abramowitz & Stegun 7.1.26 / 7.1.14. It is within 0.02% of a
numerical integral through the true sphere at every elevation and cloud
distance (`chapman_column_matches_numerical_integration`); two approximations
tried first were 15–17% off at low elevations and were not adopted. The share
of the haze in front of a layer is `(1 − T(d)) / (1 − T(∞))` (single
scattering, uniform source).

The deck gets full aerial perspective at its opacity-weighted mean depth. The
upper plane gets only the air **beyond its zenith path**: the authored
textures and PNAM tints were made to be composited straight onto the authored
sky, so the air above an overhead view is already in them; applying it again
washed the games' cloud art out. Overhead the authored look is exact; toward
the horizon the extra air fades it into the haze — what the old 0.12 fade did.

**World anchoring.** `SkyDome::cloud_origin` carries the absolute camera
position in metres. Both marches look the noise up at camera-relative
position + origin and the upper plane's UVs include it, so clouds parallax as
the viewer moves and the shadows they cast stay put
(`the_cloud_field_is_world_anchored`).

**Creation-era layer selection.** Skyrim, FO4 and FO76 author 32 cloud layers
and a `NAM1` disabled-layer mask. The four drawn slots used to be layers 0–3
verbatim, which Skyrim disables whenever they hold legacy references:
`SkyrimCloudy` names `OblivionCloudCloudyUpper01.dds` and
`WastelandCloudHorizon01.dds` there (not shipped with Skyrim SE), with
`NAM1 = 0xEF9EF0FF` disabling 0–7, and authors its live decks at 8–11, 16, 21,
22 and 28 — so **no Skyrim sky drew any authored cloud**. The slots are now
the first four enabled, textured layers, carrying their own PNAM colours, JNAM
alphas and RNAM/QNAM speeds (`select_creation_cloud_layers`,
`creation_cloud_slots_skip_layers_disabled_by_nam1`). All 71 FO4 weathers
author `NAM1` too.

FO4 sunsets are therefore *less* vivid than before, by design: `CommonwealthClear`
authors its big streaked deck (`CloudsLower04_d`, layer 2) at alpha 0, and the
old 4-slot read of the 1,024-byte JNAM gave that layer layer 1's 0.75. The
muted lavender is the weather as authored (`fo4_jnam_alphas_use_the_eight_slot_stride`).

### 2.4.1 The game's temperature and mood are the base (2026-10-20)

The per-world colour temperature — FO3/FNV and every DLC author their own: the
Pitt's grey-sepia, Point Lookout's teal, Big MT's lavender, Dead Money's orange —
comes from the worldspace `INAM` image space plus each weather's per-time-of-day
IMAD, and from the weather's own sky and cloud colours (`environment_temperature_grading`).
Everything SKYAL adds sits on that base and must not re-colour it:

* **Aerial perspective is chromatically neutral.** The atmosphere's extinction is
  averaged over the three channels (`sky_air_terms`). A per-channel Rayleigh share
  (blue scatters ~6x red) tilted every distant layer toward blue — the Pitt's
  sepia deck turned lavender at low elevations — by re-colouring a palette that is
  authored, not simulated.
* **Volumetric clouds take their chroma from the authored cloud colour**
  (`cloud_authored_colour`): each layer's mean texture colour (a heavily blurred
  mip of its sprite) times its tint, weighted by mean opacity, so a hidden layer
  contributes nothing. The Pitt's sepia is in its cloud *textures*, not its tints,
  which is why the tint alone was not enough. Physical lighting sets the puffs'
  brightness and shading; the authored colour sets their chroma
  (`CLOUD_MOOD_CHROMA_WEIGHT` 0.75, so sunset and storm lighting still read).
* **Overcast-class coverage becomes a stratiform deck** (`cloud_morphology`'s
  `stratiform` term, zero up to `CLOUD_STRATIFORM_START` 0.55 so scattered skies
  are unchanged): continuous and deep, lit by the same authored sun and ambient.

Verified per DLC at noon against the pre-change build (Pitt, Point Lookout, Zion,
Big MT, Divide, Dead Money): each world's sky tint matches its baseline to a few
8-bit levels (e.g. Dead Money 172,112,61 -> 177,119,68).

### 2.5 Cloud coverage per column, and cloud shadows (2026-10-20)

**Per-column coverage.** Coverage used to be subtracted from the 3D base noise
at every height. A 3.5 km column through 3D noise crosses a dense value
somewhere almost always, so the deck was a near-total sheet overhead at every
coverage (Skyrim and FNV "pleasant" 0.40 skies, measured straight up). Schneider
& Vos decide coverage per column from a 2D weather map; here that map is one
horizontal slice of the base volume (`CLOUD_WEATHER_SLICE_W`), thresholded at
the host-computed quantile that makes a `coverage` fraction of columns pass
(`cloud_column_threshold`, packed in `weather_aurora.w`). Within a column the
3D shape is remapped by the column's own coverage and scaled by it (Schneider,
GPU Pro 7: `base_cloud_with_coverage *= cloud_coverage`). `coverage` is now a
horizontal cloud fraction — the meteorological cloud amount.

**Cloud shadows.** `cloud_shadow.comp` bakes direct-sun transmittance through
the deck into a camera-centred, texel-snapped R16F 512² map over 64 km (scene
set 1 / binding 24), with 16 Beer–Lambert samples per column at the sourced
extinction. A texel is the column above the point where a sun ray crosses the
deck base, so receivers at any altitude look up their own crossing. The
directional upload (`compute_directional_upload`) is now the sun **above** the
clouds; every direct-sun site multiplies by the map
(`exteriorCloudSunTransmittance` — `lighting.glsl`'s two directional arms,
grass, the water glint), and passes that light with one sun value — the froxel
volumetrics, the interior portal sun, caustics — use the camera's own column,
read back from the bake after its fence (`camera_cloud_sun_transmittance`).
Beyond the map's edge there is no cloud data, so the lookup fades to
unshadowed across the outer tenth rather than letting the clamped sampler
smear the edge texel over distant terrain (found with a forced checkerboard
map, which also proved the map, binding and world addressing reach surfaces).
The clouds themselves are lit by the unattenuated sun, since their march
self-shadows. `every_direct_sun_site_applies_the_cloud_shadow` pins the sites.

**Overcast cover (2026-10-20).** `coverage` makes a `coverage` fraction of
weather-map columns eligible, but a column's density ramps linearly from zero at
the threshold, so only the densest columns are opaque: at the default 0.40 about
3% of the shadow map is shadowed, and a forced overcast-class weather (Skyrim
`SkyrimFog`, coverage 0.70) drew a few cumulus puffs over a mostly sunlit
ground. Three attempts to make coverage the cloudy sky fraction all failed on
look (a narrow value ramp gave giant flat-based slabs; a 4x finer weather map,
vertical-walled towers; a wide quantile ramp with column-scaled height still
read as towers from the ground) and were reverted. Instead the bake adds the
broad stratiform cover the discrete deck does not model: a uniform extinction
that starts at `CLOUD_OVERCAST_ONSET` (0.40, the pleasant weather's coverage, so
clear and pleasant skies are untouched) and reaches
`CLOUD_OVERCAST_OPTICAL_DEPTH` (2.5, the removed `exp(-2.5 * coverage)`'s full-deck
depth, ~8% of the sun) at full coverage; the deck's local shadows ride on top.
Measured on Skyrim at noon, direct-sun mean over the loaded terrain: pleasant
56.0; forced overcast 34.7 (64.0 before this term; 26.7 under the baseline's
blanket dimming). Cloud shadows themselves are confirmed on screen (FO4, and
Skyrim under an experimental denser deck: large soft-edged patches). What is
still missing is cloud *shape* variety at high coverage — a weather-map
height/type channel — so a high-coverage sky is still sparse puffs over an
authored overcast texture deck.

The authored upper plane casts **no** shadow. It is sky art, not a measured
medium: Skyrim draws one deck four times at different scrolls, and multiplying
those opacities blacked out the ground under a clear sky. The deck's coverage
already follows the weather classification, so overcast still shadows. The
plane does still occlude the sun disc in the sky, as the 2D path did.

---

## 3. Sequencing

| Step | State |
|---|---|
| Bloom bright-pass (the overdrive fix) | **DONE** `62a09fd9` |
| Shared `include/sky.glsl` + `SkyDome` guard | **DONE** `b7abdaa5` |
| `GpuImageDesc` cube support | **DONE** |
| Cubemap resource + `sky_cube.comp` bake | **DONE** |
| Construct + dispatch in `VulkanContext` / `draw_frame` | **DONE** — baked every frame before the geometry pass |
| Bind into scene set 1 / binding 20 + ready flag | **DONE** — a dedicated binding rather than the bindless array; see below |
| RT miss + bounded-path escape consume it | **DONE** |
| Volumetric cloud march into the cube | **DONE** |
| Background pass adopts the cloud march | **DONE** `564d0d2f` |
| Sourced, calibrated cloud lighting (Hillaire 2016) | **DONE** `564d0d2f` |
| Adaptive view march (Schneider & Vos 2015) | **DONE** `9ac8a929` |
| Geometric self-shadow march | **DONE** `5d5d6ddd` |
| Temporal jitter of shadow samples (striation) | **DONE** |
| Prefiltered mips for rough reflections | Implemented — GPU energy/lobe test and in-engine capture |
| Irradiance projection for ambient | **DONE** — nine-coefficient GPU SH projection at scene set 1 / binding 21 |
| Weather-driven cloud morphology | **DONE** — precipitation/thunder form broader, denser storm decks; explicit WTHR cloud taxonomy remains TODO |
| Palette-preserving Rayleigh/Mie sky structure | **DONE** — analytical bridge pending a full atmosphere LUT |
| Far-to-near composite, 9 km upper plane, aerial perspective | **DONE** 2026-10-20 (§2.4) |
| World-anchored cloud field | **DONE** 2026-10-20 (§2.4) |
| Creation-era `NAM1` cloud-layer selection | **DONE** 2026-10-20 (§2.4) |
| Per-column deck coverage (2D weather map) | **DONE** 2026-10-20 (§2.5) |
| Position-dependent cloud shadows | **DONE** 2026-10-20 (§2.5) |

### Reflection filtering (2026-09-14)

The 128² cube now allocates eight mip levels. `sky_prefilter.comp` computes
levels 1–7 with 256 deterministic GGX importance samples per texel, following
[Karis 2013, Pre-Filtered Environment Map](https://cdn2.unrealengine.com/Resources/files/2013SiggraphPresentationsNotes-26915738.pdf).
Every dispatch reads a **base-level-only** cube view and writes a disjoint
single-mip storage view. The base bake is made visible to compute; filtered
writes are made visible to fragment sampling. Resources remain per frame slot.
Storage rises from 786,432 to 1,048,560 bytes per slot.

Reflection misses translate the existing `roughness * 8` hit-texture bias back
to roughness and explicitly select the corresponding sky mip. Exterior
distance/RT-disabled reflection fallbacks use the same filtered sky. Diffuse
path escapes and clear glass retain mip zero. Explicit LOD avoids undefined
derivatives in divergent ray paths. This does not yet replace the surface
shader's historical roughness cutoff or its heuristic specular energy factor.

Verification: the isolated GPU test runs the actual shader against a constant
HDR cube and a bright +X face. It checks every mip/face/texel for constant
radiance preservation, bounds the directional result, and verifies the lobe
spreads to adjacent directions. It passed on the RTX 4070 Ti, including a run
with Khronos synchronization validation enabled and no reported API hazards.
Run it with:

```bash
cargo test -p byroredux-renderer \
  vulkan::sky_cube::filter::tests::gpu_filter_preserves_constant_radiance_and_broadens_a_lobe \
  --lib -- --ignored --exact --nocapture
```

The Skyrim sky capture exercised bake + filtering at about 0.16 ms total on
the RTX 4070 Ti. This is an observation, not a controlled performance delta.
The full-scene validation capture also reports water push-constant, texture
copy-size, and buffer-fill hazards outside this pass; it is not a clean
renderer-wide validation result.

### Diffuse exterior illumination (2026-09-14)

`sky_irradiance.comp` projects the freshly baked base cube into nine real,
order-two spherical-harmonic coefficients. It integrates exact cube-texel
solid angles and pre-applies the clamped-cosine bands, so the stored result is
Lambertian outgoing radiance (`E / pi`), not raw irradiance. The 144-byte
per-frame buffer is bound at scene set 1 / binding 21 together with the cube.

`exteriorSkyDiffuseOr` reconstructs the result at the shading normal. On an
exterior surface it replaces the old hand-authored ambient fallback; while the
path tracer is active it cross-fades only the untraced share, avoiding a second
copy of sky energy. This makes cloud cover and authored sky colour affect
terrain and geometry even when the indirect-ray budget fades out. The fallback
itself is unoccluded — nine SH coefficients carry no local-visibility data —
and that is fine because every consumer applies its own occlusion term
(W3.15 consumer verification, 2026-09-18): `triangle.frag`'s primary
diffuse ambient multiplies the SH result by `combinedAO`, and
`groundcover_blade.frag`'s ambient is gated by §12.1's dedicated
`skyVisibility` canopy-occlusion term. The remaining
`exteriorSkyRadianceOr` sites (reflection/path-escape misses in
`raytrace.glsl`, `lighting.glsl`, `water.frag`, `triangle.frag`) are
escaped-ray sites by construction — a traced ray that reaches the sky
genuinely sees it, so occlusion is structurally inapplicable there. The
`sky_visibility_pin` source test holds both diffuse gates so neither can
silently regress.

Verification uses the actual projection shader in the isolated Vulkan test.
A constant HDR cube must reconstruct exactly at three normals; a bright +X
cube face is checked against an independently integrated cosine reference over
124 normals. The latter permits the expected order-two ringing at its hard
edge, while still catching coefficient order, cube orientation, normalization,
and buffer visibility bugs. The test passed on the RTX 4070 Ti with Khronos
synchronization validation enabled.

### Joining weather clouds and explosion smoke

`include/medium_transport.glsl` now owns Henyey–Greenstein scattering and
homogeneous Beer–Lambert slab integration for both sky clouds and local
fog/combustion. The integral has a cancellation-safe vacuum limit, preserving
thin wisps and zero-extinction emission without over-brightening dense smoke.
Density generation, transport, albedo, and emission remain producer inputs:
cloud droplets must not inherit soot absorption or combustion temperature.

The local-volume path already transports fuel, temperature, soot, and velocity
with cooling, wind, buoyancy, and turbulence; see
[procedural volumetric fog](procedural-volumetric-fog.md). Cloud self-shadowing
and multiple scattering are not yet shared with that path. Next steps must
join the optical treatment while retaining the sky shell and local explosion
domains and their very different spatial scales.

### ~~Weather-driven direct-sun attenuation (2026-09-14)~~ — superseded by §2.5

The exterior directional upload used to apply a weather-wide
`exp(-2.5 * coverage)` after the time-of-day ramp. Clear weather was not
unchanged as intended: "pleasant" maps to coverage 0.40, so every clear sky
lost 63% of its sun (FNV noon illuminance 0.37), auto-exposure compensated for
the dim ground, and the sky blew out to white. The clouds were also lit by the
dimmed value although their march self-shadows. Replaced 2026-10-20 by the
position-dependent cloud-shadow map (§2.5).

### Sun-disc luminance floor (engine constant — record)

`shaders/include/sky.glsl` (the shared sky implementation, §2.1) lifts the
authored sun-disc term by a luminance ratio only, floored at
`local_sky_lum * 1.8 / disc_lum` (never attenuating — `max(1.0, …)`): the
authored `disc_color × sun_intensity × sun_glare` product reaches the horizon
glow's own scale only for bright-authored suns (FNV's [1.0, 0.49, 0.16] at
×3.5), and Skyrim's dim [0.33, 0.10, 0.0] slid under the horizon glow and
rendered no disc at all — a diffuse bright band with no sun in it. The 1.8
multiple is an engine calibration choice (documented only in the shader
comment before this entry): it shapes every golden-hour capture, so a retune
belongs beside this note. Hue is preserved (the ratio multiplies all
channels), and noon discs already dominate so the floor never fires there.

### The sunset hold (TOD key chain)

`weather.rs`'s time-of-day key chain holds `TOD_SUNSET` twice: the sun touches
the horizon at the authored `sunset_end`, and the key chain keeps the sunset
palette from `sunset_begin` through `sunset_hold_end = sunset_end` (clamped
inside `(sunset_begin + 0.05, night − 0.05)` so the chain stays strictly
increasing on broken-data extremes), before `TOD_NIGHT` takes over at
`night = (sunset_end + 2.0).max(sunset_begin + 0.1)`. Without the hold the
palette interpolates straight from sunset into night across the sun's own
descent, halving the golden hour the authored colours describe. Every
golden-hour capture rides this key shape.

### Weather-driven cloud morphology (2026-09-14)

The cloud shell now derives continuous morphology from the weather fields
already present in `SkyDome`: rain/snow precipitation and thunder frequency.
Fair weather retains the original broken-cumulus profile. Storm weather lowers
the cloud base, raises and broadens the top, reduces horizontal frequency, and
reduces erosion, yielding coherent deep decks instead of simply increasing
opacity. This affects the shared `cloud_march`, therefore the direct
background and baked reflection/GI sky cannot diverge.

This is deliberately not a guessed mapping from a cloud texture filename to
stratus/cumulonimbus. WTHR’s authored layers and per-TOD tints remain the
colour/detail authority; an explicit, evidence-backed taxonomy can refine the
morphology controls later.

### FO3/FNV weather evidence (2026-09-14)

Direct WTHR record inspection confirms that a filename is not a cloud-type
field. FNV `NVWastelandClear` uses `NVCloudlight.dds`; `NVWastelandHazy` uses
`NV_WastelandUpperSky2.dds`; `NVBlackMountainWeather` and
`NVSearchlightWeather` select different regional upper/overcast layers. FO3
similarly distinguishes `WastelandClear` (mostly `Alpha.dds` placeholders),
`WastelandEast` (`WastelandCloudCloudyUpper01.dds`), and `UrbanOvercast`
(`UrbanCloudOvercastUpper01.dds`). These assets occupy DNAM/CNAM/ANAM/BNAM
slots alongside non-cloud assets such as star fields and full-sky layers.

Therefore the procedural generator must retain the parsed layer slot and its
authored tint/opacity as data, then classify *role* (upper cloud, lower cloud,
horizon/full-sky, celestial) from the record/asset content. It must not infer
density or morphology directly from a path containing `cloud`; doing so would
turn starfield/horizon assets into false cloud cover and erase the distinct
wasteland climates the records actually author.

### Palette-preserving atmospheric structure (2026-09-14)

`sky_atmospheric_inscatter` augments the authored vertical WTHR gradient with
a low-amplitude Rayleigh angular phase and forward Mie haze toward the sun.
It derives its tints from the authored horizon, zenith, and sun colours rather
than imposing a generic Earth-blue palette, so desert, ash, snow, and modded
weather palettes remain recognizable. The cloud shell then composites its own
transmittance over the clear-air result; only `include/clouds.glsl` consumes
the canonical weather coverage lane.

This is a controlled bridge, not a replacement for a transmittance /
multiple-scattering LUT. It adds the previously missing sun-relative sky
variation while retaining the low cost and shared background/cubemap function.
The phase structure follows the atmosphere/cloud approach discussed in
[Hillaire’s SIGGRAPH 2016 Frostbite course](https://blog.selfshadow.com/publications/s2016-shading-course/).

The final deterministic Skyrim capture differed from the pre-extraction
capture by normalized mean absolute error `3.83e-8` (ImageMagick), confirming
that sharing the optical code preserved the current sky appearance. This is
a refactoring check, not evidence that the remaining sky realism work is done.

### FO3/FNV climate evidence (2026-09-14)

Read directly from the installed `Fallout3.esm` and `FalloutNV.esm` with
`dump_wthr_subs`. The following are authored inputs, not inferred cloud types:

| Weather | Classification | Authored layer assets (excluding alpha.dds) |
|---|---|---|
| FNV `NVWastelandClear` | Pleasant | `NVCloudlight.dds` |
| FNV `NVWastelandHazy` | Pleasant | `NV_WastelandUpperSky2.dds` |
| FNV `NVBlackMountainWeather` | Pleasant | `wastelandcloudcloudyupper01.dds` |
| FNV `NVSearchlightWeather` | Pleasant | `urbancloudovercastupper01.dds` |
| FNV `NVWastelandClearNight` | Pleasant | `NVWastelandStarfieldSky.dds` |
| FO3 `WastelandClear` | Pleasant | `WastelandCloudHorizon01.dds` |
| FO3 `WastelandEast` | Pleasant | cloudy upper + horizon 01 + cloudy lower |
| FO3 `UrbanOvercast` | Cloudy | overcast upper + horizon 02 + overcast lower |

Classification alone therefore cannot recover wasteland cloud coverage or
shape. Weather-layer slots can contain a starfield or a complete sky texture,
so treating every layer's alpha as cloud density would also be incorrect.
The climate adaptation must distinguish those asset roles, retain authored
layer motion/tint and time-of-day lighting, and translate the cloud inputs
into canonical generator parameters at EXAL. No cloud-type mapping is claimed
implemented by this evidence collection.

---

### The interior outdoor-sky lane (#4927, 2026-10-01)

Interiors carry **two skies**: the room's own XCLL lighting for surfaces,
and the worldspace-scoped outdoor palette (`SkyParamsRes` survives the cell
transition, #1199; an interior-only boot installs the canonical procedural
default, #4902) as the *portal palette* — what windows, apertures and
volumetrics see. `byroredux/src/render/sky.rs`'s `build_sky_params`
assembles the lane every frame; nothing is re-derived in the render loop
(#4902 removed that fallback).

The #2226/#3323 "interior sky is isolated" rule is **superseded in
practice** by this split: interior *surface* lighting stays isolated (XCLL
ambient/directional — `weather_system` never writes an interior's
`CellLightingRes`), while the outdoor palette is deliberately carried into
interiors for the portal, the aperture background and the portal sun. The
lane's pieces:

* **Interior ambient cube** — `interior_dalc_cube` reads the cell's XCLL
  `directional_ambient` (plus specular colour/fresnel power) into
  `SkyParams.dalc_cube` each frame. This is a per-frame read of authored
  data, not a bake; the weather-sampled `current_dalc_cube` on
  `SkyParamsRes` belongs to the exterior side of the lane.
* **`sky_lower.w` modes** — the composite packs the interior outdoor-sky
  mode into `sky_lower.w`: `0` = no outdoor sky, `1` = Show Sky
  (`InteriorSkyExposureRes`, from XCLL/Show Sky), `2` = bounded aperture.
  Night-sky details (stars, moon, aurora) gate on
  `depth_params.x <= 0.5 && sky_lower.w <= 0.5` so an interior that shows
  the sky keeps its night details even though the room's weather gate is
  off (#4908).
* **`jitter.w` = `is_exterior`** — the camera UBO's jitter vec4 carries the
  exterior flag in `w` (1.0 exterior, 0.0 interior / no exterior load). It
  gates `triangle.frag`'s half-sky reflection/refraction miss blend, the
  exterior-glass branch, and the ambient fallback: the
  `SkyParams::default()` clear-noon zenith must not bleed into interior
  glass (#1125 / REN-D9-NEW-01).
* **Portal-cloud lighting (#4839)** — the interior's portal palette calls
  `outdoor_sky_params` with `cell_directional = None`, so clouds seen
  through a window are lit by the exterior's own sunlight colour
  (`WeatherSkyState::sunlight_color`, HNAM-dimmed, cloud-transmittance
  scaled) instead of the room's XCLL key — matching the same weather
  outdoors.
* **`portal_sun` (#4909)** — direction is the outdoor `sun_direction`;
  radiance is the outdoor `sun_illuminance` (the same value that lights the
  exterior terrain — sunlight colour × dimmer × daylight fraction × cloud
  transmittance), black below the horizon. Volumetrics consume it only
  after a geometry ray proves an opening.
* **`exterior_zenith_color` (#3323)** — the window-portal escape colour for
  `triangle.frag`'s clear-depth portal pixels; pinning it to the default
  made every FNV interior window transmit clear-noon blue at 03:00.

| Piece | Consumer | Gate |
|---|---|---|
| `interior_dalc_cube` (XCLL cube) | scene UBO `dalc_cube` | `interior_xcll_cube_reaches_sky_params_without_exterior_resource` (`render/sky.rs`) |
| portal palette assembly | `portal_outdoor_sky`, `exterior_zenith_color` | `interior_only_session_bakes_procedural_outdoor_sky`, `interior_without_an_outdoor_environment_gets_no_portal_sky`, `stale_exterior_sky_params_res_does_not_leak_into_interior` |
| `sky_lower.w` 1/2 modes | `weather_sky_details` (include/sky.glsl) | `night_sky_details_follow_the_outdoor_palette_not_the_weather_flag` (`sky_dome.rs`), `interior_portal_sky_preserves_room_weather_gate` (`context/frame_params.rs`) |
| `jitter.w` is_exterior | `triangle.frag` glass/ambient branches | jitter-pin batch in `shader_constants.rs` (#1125) |
| portal-cloud lighting | cloud shell's `sun_illuminance` | `interior_portal_sky_clouds_are_lit_by_the_exterior_sunlight` |
| `portal_sun` direction/radiance | volumetrics light shafts | `interior_only_boot_uses_live_clock_for_portal_sun`, `the_portal_sun_is_the_sun_above_the_clouds` (dimmed renderer-side by the camera's cloud column, `volumetric_sun_uses_portal_lane_only_inside`), `interior_portal_sun_stays_dark_below_horizon` |
| Show Sky exposure | aperture background | `show_sky_interior_allows_open_sky_visibility_without_exterior_lighting` |

EXT-D4-01 (a stale `SkyParamsRes` misreading as a lighting regression)
follows from this gap: the lane above is the contract that turn of events
violates.

---

## 4. Harness

The measurements above are reproducible, and any change to this layer
should be checked the same way — `cargo test` cannot see any of it.

```bash
./target/release/byroredux --game skyrim_se --new-game \
  --camera-pos 5000,10500,-49000 --camera-forward 0.2,0.15,-1.0 \
  --bench-frames 180 --screenshot out.png
```

Framing is deliberate: above the terrain, so the frame is pure sky with
the below-horizon `SKY_LOWER` band visible for reference. Add
`--render-debug-mode composite_term` for the pre-bloom, pre-tonemap,
linear scene — that view is what separates a sky-assembly change from a
tone-mapping or bloom change.

Two traps, both hit while producing the numbers above:

* **Screenshots are sRGB-encoded.** The swapchain is `B8G8R8A8_SRGB`, so
  PNG values must be sRGB-decoded before being compared against authored
  linear colours.
* **A recompiled `.spv` does not reliably trigger a `cargo` rebuild.**
  `include_bytes!` dependency tracking did not pick up the regenerated
  SPIR-V, so captures silently ran a stale binary and produced a
  ~6.9/255 mean "regression" that did not exist. Always
  `touch crates/renderer/src/lib.rs` after `glslangValidator` and before
  `cargo build`, and establish the run-to-run noise floor (two captures
  of the same binary) before trusting any delta.
