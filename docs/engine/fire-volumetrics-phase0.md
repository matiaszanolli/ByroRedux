# Fire, smoke and explosions — Phase 0 survey and baseline

Tracking: parent #6531, this phase #6532. Plan (as corrected by this
survey): [fire-volumetrics-plan.md](fire-volumetrics-plan.md). Existing design
record: [procedural-volumetric-fog.md](procedural-volumetric-fog.md).

Surveyed at commit `e7f949ec1`. No engine code was changed in this phase.

## 1. How it works today, end to end

**The simulation grid is the froxel grid.** There is no separate world-space
fire grid. The transported combustion state lives in three view-aligned 3D
textures that are the same shape as the fog volume:

| Volume | Format | Holds |
|---|---|---|
| `combustion_state` | RGBA16F | fuel, temperature (K), soot extinction σ_t, visible-radiance calibration |
| `combustion_dynamics` | RGBA16F | world-space velocity xyz, specific overpressure w |
| `combustion_optical` | RGBA16F | spectral scattering coefficient σ_s (rgb) |

Plus the raw V-buffer (`lighting`, RGBA16F), an R32F thermal-emission history,
and the integrated volume — six volumes, 44 B per froxel per frame-in-flight.

- **Resolution:** one froxel per 8×8 render pixels, 64 Z slices, 128 m far
  plane. 160×90×64 at 1280×720 (confirmed by the `froxel_x/y/z` fields of the
  engine's `bench:` line). The first 5 m take 1/8 of Z linearly, the rest is
  exponential.
- **Advection is in world space; storage is in view space.** Each frame, each
  froxel reconstructs its world position, reprojects into the previous camera's
  grid, and does an RK2 backtrace plus BFECC error correction through the
  previous frame's fields (`transportCombustion`, `volumetrics_inject.comp`).
  Camera motion therefore does not become fake fluid velocity, but resolution is
  tied to the camera: detail is 8×8 pixels per cell in X/Y, and in depth the
  first 5 m are only 8 slices (≈ 0.63 m each), growing exponentially beyond.
- **How density, temperature and emission get in** (`volumetrics_inject.comp`,
  `applyCombustionSources`, `evaluateCombustionSource`): the engine converts
  particle emitters / cell objects to canonical `FogVolume` primitives
  (`byroredux/src/fog.rs`, `render/fog_volumes.rs`). Each frame the primitives
  are culled and binned into a camera-centred 16×16×16 world-space cluster grid
  (≤8 per cluster) and every froxel injects fuel, heat, soot and momentum from
  the primitives in its cluster, per `FogProfile` (flame, smoke, oil explosion,
  nuclear explosion).
- **Integration:** `volumetrics_integrate.comp`, one thread per XY column
  walking Z, exact Beer–Lambert slab integral `(1−e^{−σΔ})/σ`. Composite
  (`composite.frag::sampleVolumetricColumn`) reads one tap of the integrated
  volume with a depth-weighted 2×2 column filter. So fire is composited with
  fog, not as a separate object.
- **Self-shadowing:** `transportedCombustionTransmittance` — an 8-step,
  quadratically spaced march through the *previous* frame's soot field toward
  the sun and each local light, 2 m reach. Plus RT visibility rays against the
  scene TLAS for geometry shadows.
- **How fire light reaches surfaces:** every emissive froxel atomically
  accumulates fixed-point radiant moments into a camera-centred 8×4×8 grid. After
  that frame slot's fence the CPU decodes at most 64 inverse-square point lights
  from it (`append_combustion_surface_lights`), scales them by
  `COMBUSTION_SURFACE_LIGHT_BOOST = 4.0`, and merges them into the ordinary
  per-frame light list. Surface shading then treats them like any other light
  (clusters + ReSTIR-DI). One frame-in-flight of latency by design.
- **Cost driver:** the inject pass, not the transport. Its header documents up to
  22 ray-query traversals per froxel (1 opaque + 1 glass sun ray, 4 rim probes,
  up to 8 lights × 2 rays) and up to 9 more solid-boundary queries inside a
  combustion region.

## 2. Modules involved

**Rust**

| Path | Role |
|---|---|
| `crates/core/src/combustion.rs` | Canonical regimes, rates, ceilings (source of truth); generated into GLSL |
| `crates/core/src/radiometry.rs` | Blackbody → linear-sRGB chromaticity and visible radiance (full CIE integration) |
| `crates/core/src/lighting.rs` | Units backbone (`BETHESDA_UNITS_PER_METER`, emitter conversion) |
| `crates/core/src/ecs/components/fog_volume.rs`, `particle.rs` | `FogVolume` / `FogProfile`, particle→medium inputs |
| `byroredux/src/fog.rs` | NIF particle/mesh → canonical `FogVolume` at the translation boundary |
| `byroredux/src/render/fog_volumes.rs` | Collects `FogVolume`s and explosion ages into `GpuFogVolume`s per frame |
| `byroredux/src/cornell/combustion_lab.rs` | `--combustion-lab[-nuclear]` game-data-free test room |
| `crates/renderer/src/shader_constants_data.rs`, `build.rs` | Rust→GLSL constant generation (`shaders/include/shader_constants.glsl`) |
| `crates/renderer/src/vulkan/volumetrics.rs` | `VolumetricsPipeline`: volumes, dispatch, history, transport arming, readback |
| `…/volumetrics/combustion.rs` | Light-moment ABI, decode → `GpuLight`, reach canary |
| `…/volumetrics/fog_clusters.rs` | 16³ world-space cluster build, occupancy mask |
| `…/volumetrics/init.rs`, `noise.rs` | Pipeline/descriptor creation, Perlin–Worley density volumes |
| `…/context/assemble_camera_and_lights.rs` | Merges derived fire lights into the frame light list |
| `…/gpu_timers.rs` | GPU timestamp brackets incl. inject/integrate split |
| `…/restir.rs` | Surface ReSTIR-DI reservoirs (not volumetric) |

**Shaders** (`crates/renderer/shaders/`)

| File | Role |
|---|---|
| `volumetrics_inject.comp` (3359 lines) | Everything per froxel: fog density, sun/local-light in-scatter with RT visibility, combustion sources, transport (RK2 + BFECC + curl noise + wind + pressure + vorticity), thermal emission, light-moment reduction |
| `volumetrics_integrate.comp` | Z-column slab integration |
| `composite.frag` | Samples the integrated volume, depth-aware, adds height fog |
| `include/medium_transport.glsl`, `froxel_slices.glsl`, `blue_noise.glsl` | Shared slab math, Z distribution, jitter ranks |
| `include/shader_constants.glsl` | Generated constants (combustion rates among them) |
| `bloom_*.comp`, `taa.comp`, `svgf_*.comp` | Downstream consumers; fire has no motion vectors of its own |

## 3. Where the limits are

1. **Resolution is camera-bound and coarse.** `s1_close_flame` shows the flame as
   a lattice of ~8×8-pixel blocks at 1.6 m. That is the Phase 1 premise,
   confirmed by the baseline.
2. **The inject pass carries RT shadow work per froxel**, so any extra
   per-froxel cost multiplies a heavy base. Close to the fire inject rises
   (0.60 → 0.69 ms median, up to 1.06 ms).
3. **Fire light is a reduction to ≤64 points** with a hand-set ×4 gain and a
   one-frame readback delay. The lab logs its own canary at runtime:
   `OVERSIZED REACH … max_cull_radius_bu=8960 (canary threshold 1024 BU)`.
4. **Two blackbody formulations.** The CPU uses a full CIE integral; the shader
   uses a three-wavelength Planck approximation and a 555 nm ratio.
5. **Constants split two ways.** Combustion rates are generated into GLSL, but a
   handful (`COMBUSTION_FUEL_VAPOUR_SIGMA_T`, transmittance steps/reach, wall
   friction, ambient K) exist only in `volumetrics_inject.comp`.
6. **No heat haze, no species-specific phase functions, no equiangular light
   sampling, no wavelet turbulence, no burning particles.**

## 4. Which phases are already partly done

| Plan item | State |
|---|---|
| P1 separate world-space fire grid, sparse blocks | Not started. Sim is in froxel space |
| P2 energy-conserving integration | **Done** (`volumetrics_integrate.comp`) |
| P2 blue-noise jitter + temporal reprojection | **Done** for the froxel grid |
| P2 depth-discontinuity handling | Partial (2×2 depth-weighted filter) |
| P2 adaptive steps | N/A for froxels; belongs to the P1 ray-march |
| P3 multiple scattering | **Partial** — closed-form two-octave term, not Wrenninge octaves |
| P3 spectral soot albedo | **Done** `[0.22, 0.16, 0.12]` |
| P3 per-species phase (steam vs soot), Draine blend | Not started |
| P3 internal glow | Partial — emission transported; derived lights one frame late |
| P4 Planck colour | CPU **done**, GPU uses its own approximation, no LUT |
| P4 soot absorption on emission | **Done** (`σ_a · L_e`) |
| P4 heat haze | Not started |
| P5 ReSTIR-DI | Surface path **exists**; fire enters as ≤64 derived lights |
| P5 volume-aware candidates, equiangular | Not started |
| P6 BFECC / MacCormack | **Done** for dye fields (velocity deliberately uncorrected) |
| P6 curl noise | **Done** (1.6 m and 0.48 m eddies) |
| P6 wavelet turbulence, burning particles, NanoVDB | Not started |

## 5. Test scenes and method

All three requested scenes are covered, but **with a deviation**: they use the
game-data-free `--combustion-lab` (a 6×4×8 m Cornell room with a persistent
1850 K flame, a hood, and a one-shot explosion at t = 8 s) instead of real game
cells. It is deterministic and runs anywhere. It is an *interior* room, so the
requested "exterior with oil fire and nuclear explosion fog profiles" is
represented by the oil and nuclear profiles in the lab, not by an exterior cell.
Real-content scenes are an open question (plan, "Open questions").

| Scene | Run | What it shows |
|---|---|---|
| `s1_close_flame` | lab, camera 1.6 m from the flame, 300 frames (t = 5 s) | Close-range flame detail |
| `s2a_oil_blast_2s` / `s2b_oil_plume_7s` | lab, default camera, 600 / 900 frames (t = 10 / 15 s) | Oil explosion 2 s and 7 s after the blast |
| `s3a_nuclear_blast_2s` / `s3b_nuclear_cloud_7s` | `--combustion-lab-nuclear`, same frames | Nuclear profile |
| `s4_orbit_oil_lab` | lab, `--bench-camera orbit`, 900 frames | Moving camera through the plume (final frame only) |

Method: `renderer-stepped` (fixed dt = 1/60, frame-indexed camera), 1280×720,
TAA, headless under Xvfb, release build at `e7f949ec1`, RTX 4070 Ti, driver
580.178.04. Five runs per scene. The simulation-state hash was identical across
all five runs of every scene, so the scene content is reproducible. I used
`--bench-camera` paths and CLI flags rather than `byro-dbg`; `byro-dbg` is only
needed for `--bench-hold` sessions.

`renderer-static` was rejected: its clock is frozen, `transport_armed=0`, and
the flame is a seed point (probe run, not kept).

Reproduce: `docs/engine/fire-volumetrics-baseline/capture.sh OUT_DIR [REPS] [PREFIX]`.

## 6. Baseline: GPU time per pass

Milliseconds, median of 5 runs with (min–max). Each run contributes ONE frame's
timestamp reading (the last frame); there are no per-run averages. Before
numbers for every later phase come from this table, commit `e7f949ec1`.

| scene | sim t | inject | integrate | composite | main_render | svgf |
|---|---|---|---|---|---|---|
| s1_close_flame | 5.0 s | 0.689 (0.646–1.060) | 0.035 (0.029–0.459) | 0.114 (0.103–0.115) | 8.243 (7.579–9.012) | 1.484 (1.376–2.016) |
| s2a_oil_blast_2s | 10.0 s | 0.602 (0.531–0.640) | 0.035 (0.032–0.036) | 0.101 (0.099–0.106) | 5.045 (4.474–5.979) | 1.781 (1.469–2.086) |
| s2b_oil_plume_7s | 15.0 s | 0.647 (0.586–0.863) | 0.036 (0.034–0.042) | 0.103 (0.098–0.109) | 5.723 (5.473–6.547) | 1.825 (1.561–2.103) |
| s3a_nuclear_blast_2s | 10.0 s | 0.606 (0.530–0.907) | 0.035 (0.030–0.043) | 0.102 (0.098–0.587) | 4.590 (3.922–5.216) | 1.536 (1.380–2.251) |
| s3b_nuclear_cloud_7s | 15.0 s | 0.664 (0.597–0.746) | 0.036 (0.034–0.041) | 0.104 (0.100–0.106) | 5.635 (4.538–6.075) | 1.988 (1.610–3.369) |
| s4_orbit_oil_lab | 15.0 s | 0.601 (0.422–0.720) | 0.034 (0.024–0.039) | 0.103 (0.063–0.108) | 4.892 (3.589–6.096) | 1.642 (1.202–1.989) |

Full table (bloom, TAA, cluster cull, sky cube, shadow filter, presentation, and
the volumetrics total): [baseline.tsv](fire-volumetrics-baseline/baseline.tsv).

Reading it:

- The volumetric cost is **inject ≈ 0.6–0.7 ms, integrate ≈ 0.035 ms** at
  160×90×64. The design doc's target ranges (0.2–0.5 ms inject, 0.3–0.8 ms
  integrate) overstate integrate by roughly 9× (0.035 ms measured against a 0.3 ms
  floor), and inject (0.60–0.69 ms) is above its 0.5 ms ceiling.
- Inject is highest close to the flame (s1), and its spread (0.65–1.06) is the
  noisiest of the volumetric passes. The lone 0.459 ms integrate reading in s1
  is an outlier in one run.
- `main_render` and `svgf` vary much more between runs than the volumetric
  passes do, so treat differences smaller than about 1 ms there as noise.
- A single `composite` outlier (0.587 ms in `s3a`) is one run's frame.

Screenshots (final frame, run 1 of 5): [s1](fire-volumetrics-baseline/s1_close_flame.png),
[s2a](fire-volumetrics-baseline/s2a_oil_blast_2s.png),
[s2b](fire-volumetrics-baseline/s2b_oil_plume_7s.png),
[s3a](fire-volumetrics-baseline/s3a_nuclear_blast_2s.png),
[s3b](fire-volumetrics-baseline/s3b_nuclear_cloud_7s.png),
[s4](fire-volumetrics-baseline/s4_orbit_oil_lab.png).

## 7. Not done, and why

- **No fly-through clip.** A single screenshot per run is all the engine writes;
  video capture of an Xvfb window came out black (the swapchain is not visible
  to `x11grab`). Temporal stability therefore has not been captured, and I have
  not judged it. See the plan's open questions.
- **No real-game scenes** (see §5).
- **Not verified:** whether the derived fire lights visibly light smoke inside
  the froxel inject loop; the per-pass cost split inside `volumetrics_inject.comp`
  (transport vs RT visibility rays) — only the whole pass is timed.
