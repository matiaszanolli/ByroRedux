# Cinematic Rendering R&D Plan

Status: draft · repo references ground-truthed 2026-09-21 · not yet scheduled on
[ROADMAP.md](ROADMAP.md)

Scope: take the renderer from "physically correct" to "looks like it was shot". Five
stages, ordered by impact per effort. Each stage has a research phase (read, prototype in
isolation, decide) and a development phase (integrate, gate, benchmark). All work is
RT-only, AMD-first, compute-shader implementable, no closed vendor binaries.

This plan rides on top of the runtime contract in
[`docs/engine/physical-lighting-backbone.md`](docs/engine/physical-lighting-backbone.md)
(units, emitters, visibility, transport, ray budget). Stage 2 in particular must extend —
not bypass — the `GpuRayBudget` tiers defined there.

## Global rules for every stage

- Every stage ends with a refreshed reference benchmark run (5 scenes × 5 configs × 3
  reps — the same scene × config × repetition shape as
  [`scripts/fsr-bench-matrix.sh`](scripts/fsr-bench-matrix.sh)) and a documented
  frame-time delta. No stage lands "unmeasured".
- Every new visual feature ships behind a config flag with the Cornell oracle extended to
  cover it. The oracle tops out at **L5** today (`CornellOracleRung`,
  `byroredux/src/cornell.rs`, hardware-gated by `byroredux/tests/cornell_rt_oracle.rs`);
  the L6/L7/L8 rungs named below are new rungs this plan adds.
- Per-game seams stay in the abstraction layers (NIFAL / EXAL / WATAL / SKYAL); nothing
  in this plan touches them except where noted.

## Ground truth as of 2026-09-21

What each stage builds on, verified against the tree:

| Topic | State at HEAD |
| --- | --- |
| Tonemap / exposure | Narkowicz ACES fit in `shaders/presentation.frag` (owned by `presentation.rs`); fixed 0.85× exposure — `vulkan/exposure.rs` states auto-exposure is explicitly absent |
| Grading | `ImageSpaceModifier` (blur/tint/fade/sat/brightness/contrast) in presentation; no LUT / OCIO |
| Bloom | mip-pyramid, applied post-composite (`vulkan/bloom.rs`, `bloom_{downsample,upsample,apply}.comp`) |
| Lens effects | none — grain / CA / halation / vignette all absent |
| Denoising | SVGF is the only denoiser (`vulkan/svgf.rs`, `svgf_temporal/atrous.comp`), working on the demodulated RT indirect signal; ReSTIR-DI soft shadows carry their own temporal/spatial reuse |
| DI sampling | ReSTIR-DI live: `vulkan/restir.rs` reservoir SSBOs, 16 reservoirs/fragment, W-clamp 64× |
| GI | in-shader bounded path: `MAX_DIFFUSE_BOUNCES = 2` diffuse events in `triangle.frag` (GGX/glass events redirect throughput without consuming the budget) under `GpuRayBudget` tiers; ambient fill = SH sky diffuse / DALC cube / legacy ambient (`AMBIENT_AO_FLOOR 0.3`) |
| Upscale / AA | TAA (`taa.comp`, Halton jitter) + full FSR 3 (`crates/fsr3-sys`, `upscaling.rs`, `frame_upscaler.rs`, `--upscaler taa` fallback); gbuffer motion vectors |
| Sky | analytic WTHR-palette approximation (`shaders/include/sky.glsl`); SKYAL bake = GGX/Karis-prefiltered cube + 9-coefficient SH irradiance (`sky_cube.rs`, `sky_prefilter.comp`, `sky_irradiance.comp`); atmosphere LUT listed as future work in [`docs/engine/skyal.md`](docs/engine/skyal.md) |
| Weather | `WeatherSkyState` (`byroredux/src/components.rs`) with `[rain, snow]` precipitation; TOD-driven sky (`systems/weather.rs`); `WeatherSurfaceState` already drives wetness/ripples (`crates/core/src/ecs/components/water.rs`) |
| Materials | `shaders/include/pbr.glsl` (GGX + anisotropic, Disney diffuse, fake SSS via Burley + Hanrahan-Krueger); `multiScatterEnergyCompensation` present (Fdez-Agüera 2019); POM live (`PARALLAX_ALPHA_HEIGHT_BIT`); no dedicated hair BSDF, no area lights / IES |
| `subsurface_rolloff` | live: `crates/core/src/ecs/components/material.rs` (BSVER 130+), gated by `SLSF2_Soft_Lighting` → `MAT_FLAG_SOFT_LIGHTING` |
| Debug UI | egui overlay exists (`crates/debug-ui`, F3, Metrics/Loader/Entities/Console/Settings panels) + byro-dbg TCP console with `screenshot` |
| Oracle / captures | Cornell oracle rungs L0–L5; golden-image harness (`tests/renderer_anchor.rs`, `scripts/png-stability.py`); 22 smoke tests under `docs/smoke-tests/` |

Corrections applied to the original draft of this plan:

- "current single-bounce/ambient fill" was wrong — the in-shader GI path is already a
  bounded **2-diffuse-bounce** loop. Stage 2 replaces the *mechanism* (per-pixel bounded
  loops → denoised, amortized, cache-backed ReSTIR-GI), it does not merely "add bounces".
- "Kulla-Conty compensation in `pbr.glsl` (small, immediate win)" — the equivalent
  already exists (`multiScatterEnergyCompensation`, Fdez-Agüera 2019). Stage 4's item is
  reframed as a coverage audit/extension.
- "S73" and "S85" do not exist in the repo. Real referents substituted: the
  `subsurface_rolloff` / `SLSF2_Soft_Lighting` flags themselves; SKYAL's
  prefilter + SH pipeline.
- "the R6a ground-cover gap" — R6a is the bench-of-record *staleness tracker*
  (`r6a_stale_*`). The real gap: the bench-of-record matrix has no exterior scene.
- Tonemapping lives in the **presentation** pass, not composite
  (`docs/COMPATIBILITY.md:129` is stale on this).
- `m-color.sh`, `m-characters.sh`, `m-sky.sh` do not exist yet — they are the new capture
  harnesses this plan adds; naming follows the existing `m-*.sh` convention.

Confirmed accurate from the draft: existing DI reservoirs, SVGF as current denoiser,
TAA/FSR motion vectors, `WeatherSkyState`, `radiometry` units
(`crates/core/src/radiometry.rs` + `lighting.rs`), NIFAL/EXAL/WATAL seams, NiLight
import (`import_nif_lights`).

---

## Stage 1 — Color pipeline and physical camera

**Goal:** frames go through a simulated lens and sensor, not a monitor. Highest impact,
lowest risk: pure post-process, no path tracer changes. All of it lands where ACES +
fixed exposure already live (the presentation pass).

**Research (1 session):**

- Tonemapping: AgX (Sobotka) vs ACES 1.3 RRT/ODT — replacing the current Narkowicz fit
  in `presentation.frag`. Compare on three scenes (Nordic interior, Mojave exterior,
  Commonwealth overcast). Decision criterion: highlight desaturation behaviour and skin
  tones.
- Physical exposure: EV100 derived from scene luminance via the existing `radiometry`
  units (`crates/core/src/radiometry.rs`, `lighting.rs` — Meters/ExtinctionPerMeter
  exist; EV/lux/candela types are part of this stage). Lagarde & de Rousiers 2014
  (Frostbite) as the reference.
- Grading: 3D LUT stage, OpenColorIO-compatible `.cube` files, one LUT per game/weather
  profile. Today grading is only `ImageSpaceModifier` (tint/fade/sat/contrast).
- Lens effects: physically based bloom from lens diffraction (Kawase multi-pass is the
  cheap baseline; FFT convolution bloom as the target — replacing the current
  mip-pyramid bloom), halation, vignetting, lateral chromatic aberration, film grain as
  luminance-dependent noise (Newson 2017 model).

**Development (1–2 sessions):**

1. Exposure and tonemapping stage in the presentation pass; per-game defaults,
   per-weather overrides via `WeatherSkyState`.
2. LUT stage + a `tools/lut-bake` utility so LUTs can be authored in Resolve/Nuke and
   dropped in (`tools/` already hosts byro-dbg, byro-detect, byro-launcher, nifskope,
   texture-upscale).
3. Lens stack as ordered, individually toggleable passes.
4. Debug: extend the existing egui overlay (`crates/debug-ui`, F3) with an exposure
   readout, histogram, false-color luminance panel; byro-dbg `screenshot` remains the
   capture path.

**Acceptance:** side-by-side capture harness (`m-color.sh`, following the
`docs/smoke-tests/` `--bench-hold` → byro-dbg-attach pattern) producing before/after
triptychs for the five bench scenes; Cornell oracle L6 verifies tonemapper is monotonic
and grey-balanced; frame-time delta < 0.5 ms at 1440p.

**Risks:** LUT + tonemapper order mistakes (grade in display space vs scene-linear).
Mitigation: write the pipeline order into the oracle.

---

## Stage 2 — Global illumination

**Goal:** real indirect light (multi-bounce), replacing the current in-shader bounded
2-diffuse-bounce loop + ambient fill (SH sky diffuse / DALC cube / legacy ambient). This
is the heavy engineering stage.

**Research (2 sessions):**

- ReSTIR-GI (Ouyang et al. 2021) as the first target; ReSTIR PT (Lin et al. 2022) as the
  follow-up once GI is stable. Start from the in-tree notes:
  `docs/research/restir_gi_notes.md`, `restir_overview_notes.md`,
  `volumetric_restir_notes.md`.
- World-space radiance cache to amortize bounces: Neural Radiance Cache (Müller et al.
  2021, paper is public) vs hash-grid radiance cache (Kaminsky/"Surfel"-style, e.g.
  GIBS / AMD's GI-1.0). Decision criterion: implementability in pure compute on RDNA and
  interaction with streaming exteriors.
- Prototype each in a standalone Cornell + Sponza harness before touching the engine
  (`--cornell` exists; `--combustion-lab` is the precedent for a dedicated harness
  scene).

**Development (3–4 sessions):**

1. ReSTIR-GI reservoirs alongside the existing ReSTIR-DI reservoirs (`vulkan/restir.rs`,
   16/fragment); shared temporal reprojection with the current TAA/FSR motion vectors.
2. Radiance cache (chosen variant) fed by the GI samples; cache invalidation tied to
   streaming cell load/unload and to time-of-day.
3. Retire the in-shader bounded loop + ambient fill to a fallback preset for low-end.
4. Emissive geometry and area lights integrated as GI sources. NiLight already imports
   (`import_nif_lights`) and emissive materials already contribute at GI hits
   (`ray_hit.glsl` `rayHitEmission`) — the gaps are registering emissive geometry as
   first-class `Emitter`s and adding analytic area lights (both absent today).

**Acceptance:** Cornell oracle L7 (energy conservation across N bounces vs reference
path trace at 4096 spp); temporal stability test (static camera, 300 frames, variance
below threshold); bench refresh with GI on/off per scene. Target: ≤ 3 ms GI cost at
1440p on the reference GPU.

**Risks:** ReSTIR bias and temporal artifacts in fast-moving scenes; cache leaks across
thin walls (already a known class in the volumetrics oracle). Mitigation: dedicated
stress scene with thin geometry.

---

## Stage 3 — Denoising and sampling

**Goal:** remove the last "noisy path tracer" tells. Depends on Stage 2 producing signals
worth denoising.

**Research (1 session):**

- NRD (NVIDIA Real-time Denoisers, MIT, vendor-agnostic): ReBLUR for diffuse/specular,
  ReLAX for ReSTIR outputs, SIGMA for shadows. Read the integration guide; verify it
  builds on our Vulkan/ash path without SDK deps. SVGF notes already in-tree
  (`docs/research/`).
- Spatio-temporal blue noise (Wolfe et al. 2022): sampling masks for every stochastic
  decision (lights, GI, DoF, volumetrics).
- Long-term watch: small neural denoisers runnable in compute (no vendor runtime).

**Development (2 sessions):**

1. STBN mask generation tool + replacement of all current random sequences.
2. NRD integration replacing SVGF (`vulkan/svgf.rs` — the only denoiser today); keep SVGF
   as fallback preset.
3. Shadow denoising via SIGMA for area-light penumbrae from Stage 2.

**Acceptance:** SVGF vs NRD comparison captures on the bench matrix; ghosting test scene
(moving light, moving occluder); bench refresh. Target: net frame-time neutral or better
vs SVGF.

**Risks:** NRD is C++; needs an ash-side binding or a clean-room port of the shaders
(shaders are HLSL/GLSL, MIT). Decide early which.

---

## Stage 4 — Materials the eye does not forgive

**Goal:** skin, hair, metals, wet and snowy surfaces. This is where "videogame" shows
most on Bethesda content.

**Research (1–2 sessions):**

- Skin: random-walk subsurface scattering in the path tracer (Christensen & Burley 2015;
  Wrenninge et al. 2017) vs screen-space SSS (Jimenez 2015) as a bridge. Today's fake
  SSS (Burley diffuse + Hanrahan-Krueger mix in `pbr.glsl`) is the baseline being
  replaced. Decision criterion: cost on FaceGen heads at typical dialogue-camera
  distance.
- Hair: Chiang et al. 2016 (Disney) hair BSDF adapted to Bethesda hair cards; Marschner
  2003 as baseline. Anisotropic GGX exists; no dedicated hair BSDF today (legacy `hair`
  flags + HairTint material kind only).
- Multi-scatter microfacet energy compensation: already present
  (`multiScatterEnergyCompensation`, Fdez-Agüera 2019, `pbr.glsl`) — research is a
  coverage audit (rough dielectrics, coloured metals, anisotropy) against Kulla & Conty
  2017, not a from-scope implementation.
- Weather-driven material layers: wetness and puddle accumulation (Lagarde 2012), snow
  accumulation by normal-up and exposure, as a layer on top of NIFAL's resolved PBR, not
  inside it. `WeatherSurfaceState` wetness/ripple groundwork already feeds
  `crates/core/src/ecs/components/water.rs`.

**Development (2–3 sessions):**

1. Multi-scatter compensation coverage audit + extension where the Fdez-Agüera fit falls
   short of Kulla-Conty (small, immediate win).
2. SSS: screen-space first behind a flag, path-traced random walk as the follow-up; the
   existing `subsurface_rolloff` / `SLSF2_Soft_Lighting` flags become inputs.
3. Hair BSDF with anisotropy derived from card tangents.
4. Weather material layer consuming `WeatherSkyState` precipitation +
   `WeatherSurfaceState` wetness.

**Acceptance:** Cornell oracle L8 (material energy conservation with multi-scatter); new
dialogue-camera capture in `m-characters.sh`; bench refresh.

**Risks:** Bethesda hair cards were authored for a specific shader; correct physics can
look worse. Mitigation: per-game tuning knobs at the NIFAL boundary, documented as
translation, not as engine behaviour.

---

## Stage 5 — Atmosphere, clouds, area lights

**Goal:** sky and air that behave like the real thing at every time of day.

**Research (1 session):**

- Hillaire 2020 ("A Scalable and Production Ready Sky and Atmosphere Rendering
  Technique"): transmittance / multi-scatter / sky-view LUTs — the upgrade the analytic
  WTHR-palette sky (`shaders/include/sky.glsl`) and `docs/engine/skyal.md` already
  anticipate; the volumetrics froxel pass already cites Hillaire 2015.
- Volumetric clouds: Schneider 2015/2017 (Horizon) and Hillaire 2016 (Frostbite)
  multi-scatter approximation; narrow-band spectral noise for undulatus and other banded
  cloud types (this is the missing altocumulus). `shaders/include/clouds.glsl` +
  `CloudSimState` + `cloud_noise.rs` are the baseline.
- Area lights and IES profiles: linearly transformed cosines (Heitz 2016) for analytic
  fallback; RT-sampled otherwise. Both absent today.

**Development (2 sessions):**

1. Atmosphere LUTs replacing the current sky lighting for the direct sky term; SKYAL's
   prefiltered GGX cube + 9-SH irradiance pipeline (`sky_cube.rs`, `sky_prefilter.comp`,
   `sky_irradiance.comp`) becomes downstream of it.
2. Cloud layer with per-weather profiles and shadowing onto terrain.
3. Area lights and IES in the DI light list.

**Acceptance:** sunrise-to-sunset timelapse capture (`m-sky.sh`) at fixed exposure;
volumetric oracle extended to atmosphere; bench refresh with an exterior scene finally
in the matrix (the bench-of-record matrix has none today; `m-exteriors.sh` and
`--bench-groundcover-sampling` are the nearest existing pieces).

**Risks:** cloud cost at 1440p. Mitigation: half-res with temporal reprojection from day
one.

---

## After the five stages: camera

Not a rendering stage but the one that decides whether it reads as film. Depth of field
driven by real lens parameters (focal length, f-stop, focus distance), object and camera
motion blur from velocity buffers, and a camera system for dialogue and traversal that
replaces Bethesda's. No paper resolves this; it is design work and belongs to the "new
way to play" track, after the vertical slice is playable.

---

## Sequencing summary

| Stage | Sessions | Depends on | Primary win |
| --- | --- | --- | --- |
| 1 Color & camera | 2–3 | — | Perceived quality of everything existing |
| 2 GI | 5–6 | — | Indirect light, room "air" |
| 3 Denoise & sampling | 3 | 2 | Clean signal, temporal stability |
| 4 Materials | 3–5 | 1 | Skin, hair, metals, weather |
| 5 Atmosphere | 3 | 1 | Sky, clouds, area lights |

Stages 1 and 2 can run in parallel threads; 3 waits for 2; 4 and 5 can interleave with
anything.
