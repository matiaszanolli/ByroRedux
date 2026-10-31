# Fire, smoke and explosions: quality and performance pass

Tracking: parent #6531. Sub-issues: Phase 0 #6532, Phase 1 #6533, Phase 2 #6534, Phase 3 #6535, Phase 4 #6536, Phase 5 #6537, Phase 6 #6538. Phase 0
findings and the baseline are in [fire-volumetrics-phase0.md](fire-volumetrics-phase0.md);
this file is the plan **as corrected by that survey**. Where the survey
contradicted the original plan the correction is marked **[corrected]**.

## Goal

Improve the look and cost of ByroRedux's volumetric fire, smoke and
explosions. The base already exists: a camera-aligned froxel grid with temporal
reprojection, and a transported combustion solver that runs *inside* that grid
(advection, reaction chemistry, soot, self-shadowing, overpressure). Its rates
live in `crates/core/src/combustion.rs`. Fire lighting on surfaces is reduced
from the transported field, not from analytic lights.

## Ground rules

- **Survey before changing.** Phase 0 is read-only apart from the report and
  the baseline captures.
- **Track the work.** Each sub-issue has explicit acceptance criteria; commits
  cite it with `Refs #N` — never a closing keyword, because GitHub closes any
  issue named next to one. A sub-issue closes only when every criterion
  is met in code.
- **Keep the GLSL mirror in sync.** **[corrected]** The combustion constants in
  `combustion.rs` are *generated* into GLSL, not hand-mirrored:
  `combustion.rs` → `crates/renderer/src/shader_constants_data.rs` →
  `crates/renderer/build.rs` → `shaders/include/shader_constants.glsl`.
  Rebuild `byroredux-renderer`, recompile the affected shaders, and run
  `scripts/check-shader-artifacts.sh` before any commit that touches shaders.
  Constants that exist only in `volumetrics_inject.comp`
  (`COMBUSTION_FUEL_VAPOUR_SIGMA_T`, `COMBUSTION_LIGHT_TRANSMITTANCE_STEPS`,
  `COMBUSTION_WALL_FRICTION_PER_SECOND`, `AMBIENT_TEMPERATURE_K`, …) are *not*
  generated; moving one into core is a prerequisite for tuning it.
- **Measure everything.** For every phase, report GPU time of each volumetric
  pass before and after, same scenes, same machine, naming the commit each
  number came from. No GPU → say so and stop. Do not estimate.
  The harness is `docs/engine/fire-volumetrics-baseline/capture.sh`.
- **Do not judge noise yourself.** Anything touching sampling, step count,
  jitter or denoising: capture screenshots and ask Matías.
- **No NVIDIA code or binaries.** NVIDIA Flow is reference reading only; read
  its licence first and do not copy code.
- **Plain language in reports.**
- **Stop and ask** before anything that changes the save format, the
  `GpuMaterial` / `GpuInstance` layout, or a public SDK interface.

## Phase 0: Survey and baseline — done in this change

Report, scene recipes, baseline timings and screenshots are committed. See the
phase-0 document.

## Phase 1: Separate fire rendering from the froxels

**[corrected]** The simulation currently lives *in* the froxel grid (view
aligned, 160×90×64 at 1280×720, 8×8 pixels per froxel, hybrid linear/exponential
Z). It is advected in world space (RK2 backtrace through the previous frame's
view-aligned volumes) but stored in view space. So the "if it is currently in
froxel space, move it" branch applies. No sparse-block or world-space grid code
exists.

The baseline screenshot `s1_close_flame` shows the cost of that choice: at
1.6 m the flame is a visible lattice of froxel-sized blocks.

- Add a world-space simulation grid around emitters (sparse blocks; the Flow
  32×16×16 allocation is reference reading only).
- Ray-march that grid directly, at higher resolution, where it is on screen.
- Inject only coarse density and emission into the froxels.
- Composite the local volume with depth awareness against opaque geometry.

Stop-and-ask check before starting: does the new grid need to be saved
(M45)? Today nothing in `byroredux/src/save_io*` or `crates/save` refers to the
combustion or froxel state, so the transported field is not saved.
**Answered (Matías, 2026-10-31): no.** The local grid is transient, like the
froxel field today: it is rebuilt from the `FogVolume` sources after a load. No
`FORMAT_MAJOR` bump and no schema-fingerprint change.

**Done when:** flame detail at close range clearly improves over the baseline
screenshots (confirmed by Matías), distant fires look unchanged, cost reported
against the baseline.

### Phase 1 design (proposal; stage 1a built, see the staging table)

**Why the froxels look blocky.** At 1.6 m a froxel is only a few centimetres
across the screen (the exact figure depends on the field of view) but 0.63 m deep (first 5 m are only 8 slices). The flame is sampled
coarsely along the view direction, which is the lattice seen in
`s1_close_flame`. A grid with equal-sized cubic cells removes that.

**What makes the port cheaper than it looks.** The transport is already written
in world space. Every read of the old field goes through one function,
`samplePreviousTransport(worldPos)`, which calls `reprojectHistory` (the only
place that turns a world position into a froxel coordinate). The sources
(`applyCombustionSources`) take a world position plus a segment. The finite
differences take three basis steps (`stepX/Y/Z`), which for a cubic grid are
just the three axes. So the chemistry, BFECC, curl noise, wind, pressure and
boundary tests can move into a shared include
(`include/combustion_transport.glsl`) used by both the froxel pass and a new
local-grid pass; only the sampler and the step vectors differ. The soot,
heat and fuel rates stay in `combustion.rs`.

**Pieces.**

1. **Local grid** (`volumetrics/local_grid.rs`, new). World-space, axis-aligned,
   cubic cells, three RGBA16F volumes (state, dynamics, optical) like the froxel
   field, ping-ponged per frame in flight. Anchored on the emitter clusters the
   16 m fog cluster grid already finds (`fog_clusters.rs`), so no new culling.
   Transient (see above).
2. **Local sim pass** (`volumetrics_local_sim.comp`, new). One thread per cell,
   same update as today but sampling the local grid instead of reprojecting.
3. **Local ray-march pass** (`volumetrics_local_march.comp`, new). Per screen
   pixel the ray is clipped to the grid box and to the opaque depth, then marched
   with the same exact slab integral as `volumetrics_integrate.comp`. Output:
   radiance and transmittance for the clipped segment.
4. **Hand-off to the froxels.** Inside a local grid the froxel pass stops
   simulating and instead fills its three combustion volumes by averaging the
   local cells under each froxel (coarse density and emission only, which is what
   the acceptance criterion asks for). Everything that reads those volumes today
   (fire-light moments, sun and light self-shadowing, the thermal-emission
   history) keeps working unchanged. Froxels inside the box are told to leave the
   transported medium out of the fog they integrate, so the fire is not drawn
   twice.
5. **Composite.** `composite.frag` already reads the integrated fog volume at the
   surface depth. With a local grid on screen it reads the fog at three depths
   (box entry, box exit, surface) and combines them: fog before the box, then the
   local fire, then fog after it. This is where the depth-aware part of the
   acceptance criterion lives.

**Decisions I need from Matías before building.**

- **Dense boxes or sparse bricks?** The issue says sparse blocks. A dense box per
  emitter cluster is much simpler (hardware trilinear filtering, no indirection)
  and is enough to prove the resolution win; bricks only save memory and
  dispatch time. Proposal: ship dense boxes first, with a fixed cell size chosen
  per box (about 2.5 cm for a candle or torch, 5 cm for a campfire, 20 cm and up
  for explosions), and add bricks only if the measured cost needs them. A 128³
  box with three RGBA16F volumes ping-ponged is about 100 MB, so the number of
  boxes has to be capped (proposal: 4).
- **Explosions that outgrow the largest box** (the nuclear profile spans tens of
  metres) stay on the froxels, as they do today. Proposal: accept that for
  Phase 1; Phase 6's burning-particle work is the real answer for large blasts.
- **Lighting inside the local march (the main open risk).** The froxel pass
  gets sun and light visibility from ray queries, which is too expensive per
  screen pixel. Proposal: the march uses the fire's own emission plus a lighting
  value read from the froxel pass at the same place, and a short march inside the
  local grid for self-shadowing. That needs one extra froxel-resolution volume
  holding visibility-weighted sun and light radiance without the phase function.
  I do not know yet whether that looks right for thick smoke; it is the first
  thing to check in screenshots.

**Staging, each commit measured on the GPU and `Refs #6533` only.**

| Stage | Change | Visible? | Check |
|---|---|---|---|
| 1a | Move transport into the shared include; froxel output byte-identical | no | **Done.** Final frame pixel-identical (PNG checksum) on s1, s2a, s3a across 3–5 runs; `check-shader-artifacts.sh` green; per-pass GPU time unchanged (s1 inject median 0.605 ms before, 0.611 ms after, 5 runs each, both built from `0d1b401ef` with and without the move) |
| 1b | Local grid + sim pass, written but not yet drawn | no | Hash of local grid over 5 runs identical; sim pass ms reported |
| 1c | Local march + composite combine + froxel hand-off | **yes** | s1 and s5 screenshots, per-pass ms, review by Matías |

**Risks to plan around.**

- Many tests read `volumetrics_inject.comp` as text and split on function names
  (35 references in `volumetrics.rs` alone, more in `gpu_timers.rs`, `reflect.rs`,
  `shader_constants*.rs` and others). Moving code into an include silently
  weakens them unless each is repointed in the same commit.
- `check-shader-artifacts.sh` has to pass, and shaders must be recompiled with
  plain `-V` (the reflection test needs `OpName`).
- No GPU, no measurement: if the machine is busy with the user's engine, wait.

## Phase 2: Integration quality

**[corrected]** Partly done already:

| Item | State at `e7f949ec1` |
|---|---|
| Energy-conserving slab integration (Hillaire 2015) | **Done** in `volumetrics_integrate.comp` (closed-form `(1−e^{−σΔ})/σ`, cancellation-safe series) |
| Blue-noise jitter + temporal reprojection | **Done for the froxel grid** (`froxelJitter`, R3 + blue-noise rank, history weight 0.92 per the design doc, shorter emissive history) |
| Depth-aware compositing | **Partly**: `composite.frag` uses a depth-weighted 2×2 column filter |
| Adaptive step sizes | **Not applicable to the froxel grid** (one sample per froxel, fixed slice distribution). Applies only to the Phase 1 local ray-march |

So Phase 2's real work is the same three items applied to the *Phase 1 local
ray-march*, plus the Kovalovs-style depth-discontinuity handling that the
current 2×2 filter only approximates. Do Phase 2 after Phase 1 or fold the
local-march part into it.

**Done when:** the "steppy" layering is gone at equal or lower sample counts and
Matías has reviewed a fly-through for flicker.

## Phase 3: Scattering appearance

**[corrected]** Partly done:

- Multiple scattering: a closed-form two-octave term `1 + 0.6ω + 0.3ω²` in the
  medium's spectral albedo is already in `volumetrics_inject.comp`. It is not
  Wrenninge's per-octave (a, b, c) attenuation with a phase and extinction
  falloff per octave. Decide whether to replace or keep it.
- Soot: spectral albedo `[0.22, 0.16, 0.12]` already; point/spot light phase is
  a fixed soot-like dual lobe (forward 0.35, backward −0.12, mix 0.65).
- **Not done:** steam and soot are not distinct species — both are the `Smoke`
  `FogProfile` with one transported albedo. No Draine blend anywhere. The sun
  path uses the authored-weather lobes, not a per-species phase.
- Internal glow: emission is transported and cools with the fluid. The ≤64
  derived fire lights are appended to the same per-frame light list the froxel
  inject reads, one frame-in-flight late (CPU readback), so they can light smoke
  from inside; whether they visibly do was not verified in Phase 0.

**Done when:** thick smoke no longer looks flat or dirty, and steam and soot
read as different materials in one scene.

## Phase 4: Fire colour and heat

**[corrected]**

- Planck→colour exists on the CPU with full CIE integration
  (`crates/core/src/radiometry.rs`). The GPU does **not** consume it: the inject
  shader has its own three-wavelength (610/550/460 nm) Planck approximation and
  a one-wavelength (555 nm) radiance ratio. So the task is a lookup table built
  from `radiometry` and sampled by the shader, which also removes the
  duplicated formulation. Unit conversion stays in the existing
  `lighting`/`radiometry` backbone.
- Soot absorption on emission is already present (`σ_a · L_e`,
  `σ_a = σ_t (1 − albedo)`).
- **Heat haze does not exist.** No distortion pass. The overpressure field
  exists but only the solver reads it (`combustion_dynamics.w`).

**Done when:** flame colour follows temperature without hand-tuned ramps and
the haze is visible but stable under camera motion.

## Phase 5: Fire as a light source in the ray-traced path

**[corrected]** Surface ReSTIR-DI already exists (temporal + spatial reservoirs,
36 B per pixel). Fire reaches it as up to 64 inverse-square point lights
decoded on the CPU from an 8×4×8 moment grid (fence-latent), scaled by an
art-directed `COMBUSTION_SURFACE_LIGHT_BOOST = 4.0`. So the plan's "feed
emissive volume regions in as candidates" means replacing that reduction with
volume-aware candidates, not adding ReSTIR. No equiangular sampling exists.

Known issue found in Phase 0: the lab trips the engine's own reach canary
(`max_cull_radius_bu=8960` against a 1024 BU threshold).

**Done when:** fire lights the room with correct shadows, nothing is lit twice
(the volume's own emission is composited separately from the derived lights),
and Matías has reviewed the noise.

## Phase 6: Simulation detail and efficiency

**[corrected]**

- MacCormack: **BFECC error correction (Selle et al. 2008) is already in** the
  dye-field advection. Velocity is deliberately not corrected. Remaining work is
  only wherever detail is still lost.
- Curl noise: **already in** (two eddy scales, 1.6 m and 0.48 m, in
  `combustion.rs`). Wavelet turbulence is not.
- Burning-particle explosions (Feldman 2003): not present.
- NanoVDB layout: not present, optional.

**Done when:** the same visual detail at a coarser grid, or more detail at the
same cost, measured against the Phase 0 baseline.

## Open questions for Matías

1. Real-content baseline scenes. **Waived for sequencing (Matías,
   2026-10-31):** a loose `--mesh` explosion NIF is acceptable as the explosion
   scene, so Phase 1 may start. The empty-sky frames from those NIFs still need
   diagnosing before they can serve as a baseline. Earlier status: **partly answered.** One exists (`s5`, Skyrim
   Whiterun Bannered Mare hearth). An exterior fire and a real explosion do not
   yet; see the phase-0 document §7 for what was tried. Still needed from
   Matías: whether a loose vanilla explosion NIF (`--mesh`) is acceptable as the
   explosion scene, or whether it has to be a placed one in a cell.
2. ~~Frame-sequence capture for the flicker review.~~ **Answered:** approved.
   `--bench-capture-every N --bench-capture-dir DIR` is implemented (documented
   in `game-loop.md`); it was added during Phase 0 as tooling, not as Phase 2
   work. The baseline sequences themselves are still to be captured.

## Reference list

Unchanged from the original brief (Wronski 2014; Hillaire 2015; Kovalovs 2020;
Schneider 2022/2023; Fong et al. 2017; Wrenninge et al. 2013; Jendersie &
d'Eon 2023; Nguyen/Fedkiw/Jensen 2002; Pegoraro & Parker 2006; Animal Logic
2023; Lin/Wyman/Yuksel 2021; Kulla & Fajardo 2012; NVIDIA Flow — reference
reading only; Selle et al. 2008; Kim et al. 2008; Bridson 2007; Feldman et al.
2003; Museth 2021).
