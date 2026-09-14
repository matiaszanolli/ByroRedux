# SKYAL — Sky Abstraction Layer

**Status: PARTIAL (2026-09-14).** Shared sky evaluation, volumetric clouds,
the sky cubemap, and GGX-prefiltered reflection mips are implemented.
Clouds and local fog/combustion now share the phase-function and slab
integration implementation. Diffuse sky irradiance now comes from the baked
sky; weather-driven cloud morphology and a palette-preserving atmospheric
approximation are implemented, while a full atmospheric LUT and explicit
authored cloud taxonomy remain unfinished. See §3.

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
* **Clouds are UV-projected planes.** Up to four WTHR cloud textures are
  projected onto an infinite plane (`dir.xz / dir.y`). Near the horizon
  this stretches into visible streaks and needs a `smoothstep(0, 0.12,
  elevation)` fade to hide the singularity. Clouds have no depth, no
  self-shadowing, and no parallax.
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
`gpu_sky_cube` ~0.1 ms. The march scales with pixel count, so the worst
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
* **Coverage mapping provenance.** Coverage is `SkyDome::weather_aurora.z`,
  derived in `env_translate::fog_coverage_from_weather` as a fixed
  0.86 / 0.80 / 0.70 / 0.40 / 0.55 per classification flag. That mapping
  is pre-existing and uncited.
* **Noise shape frequencies** (`0.00008` / `0.0009` per metre) predate the
  sourced pass and are not yet justified.

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
remains deliberately unoccluded, so enclosed exterior-adjacent geometry still
needs the existing traced/AO terms rather than treating SH as local bounce
lighting.

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

### Weather-driven direct-sun attenuation (2026-09-14)

The exterior directional upload now consumes the same `WeatherSkyState`
`cloud_coverage` value as the cloud shell. It applies a conservative
Beer–Lambert world-scale transmission (`exp(-2.5 * coverage)`) after the
time-of-day sun ramp: clear weather is unchanged and a full deck retains about
8% of the direct key. This prevents the old contradiction where the rendered
sky was overcast but terrain remained clear-noon lit. Interior XCLL keys do
not use weather coverage.

This is intentionally a climate-scale approximation, not a cloud-shadow map:
it cannot cast moving, position-dependent cloud shadows. The next lighting
step must sample the same procedural density along each surface-to-sun column
without duplicating cloud generation in the material shader.

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
