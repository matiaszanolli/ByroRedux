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
| 1b | Local grid + sim pass, written but not yet drawn | no | **Done.** See "Stage 1b results" below |
| 1c | Local march + composite combine + froxel hand-off | **yes** | **Built; awaiting Matías's review.** See "Stage 1c results" below |

**Stage 1b results** (commit named in the history; release build, RTX 4070 Ti,
1280×720, ray-quality tier pinned with `--rt-test-ray-quality-tier 3`).

- *Froxel output unchanged.* Final frame byte-identical between the pre-stage-1a
  binary (`0d1b401ef`) and the stage 1b binary on s1, s2a and s3a, 3 runs each.
  Without the pin the adaptive RT tier flips between 2 and 3 from run to run and
  changes the whole frame (the unmodified binary does it too), so an unpinned
  pixel comparison proves nothing; `capture.sh` now takes
  `BYRO_CAPTURE_EXTRA` for the pin. (The stage 1a check above compared
  unpinned runs that happened to land on the same tier; the pinned runs cover
  1a and 1b together.)
- *Local grid state is deterministic.* The order-independent state hash on the
  bench line (`local_grid_hash`) was identical across 3 runs of s1
  (`a4a2acc2` after 300 frames), also with `BYRO_VALIDATION=1` (sync
  validation on: no hazard or VUID reported by the new pass).
- *Cost.* Median of 5 interleaved runs, ms. The sim pass is added on top of the
  existing passes until stage 1c moves the transport out of the froxels, and it
  scales with the cells that carry combustion, not with box size:

| scene | boxes | active cells | `volumetrics_local_sim` | `volumetrics_inject` base → 1b | `volumetrics` total base → 1b |
|---|---:|---:|---:|---|---|
| s1_close_flame | 1 | 7,228 | 0.081 | 0.583 → 0.572 | 0.613 → 0.682 |
| s2a_oil_blast_2s | 2 | 37,277 | 0.262 | 0.674 → 0.645 | 0.715 → 0.952 |
| s3a_nuclear_blast_2s | 2 | 272,145 | 1.252 | 0.728 → 0.692 | 0.769 → 2.014 |

  The nuclear scene is the case to watch: its cloud fills about half of both
  boxes, so the full advection / reaction / boundary-ray path runs for those
  cells. Stage 1c is expected to pay this back by having the froxel pass skip
  transport inside a box; if it does not, the coarse cell ladder or the box
  count is the knob. VRAM: 48 MiB resident (see `memory-budget.md`).

**Stage 1c results** (release build, RTX 4070 Ti, 1280×720, ray-quality tier
pinned to 3; before/after screenshots in
[`fire-volumetrics-baseline/phase1/`](fire-volumetrics-baseline/phase1/)). **This
stage is built and measured but NOT accepted:** the acceptance criteria that
need a human — close-range flame clearly better than `s1_close_flame`, distant
fires unchanged — are Matías's call, and the screenshots below have open issues
(see "Known problems").

**Review so far (Matías, 2026-11-01):** the close flame is an improvement; the
s3a blocks were to be traced before Phase 2 (done, below). The explosion look
and the cost are still open.

What it does: boxes of the local grid are simulated first each frame; the
froxel inject pass hands the fire inside a box off to them (it fills its own
combustion volumes with the mean of the box cells under each froxel, advances
nothing there, and leaves the fire's medium out of the fog it integrates);
composite ray-marches each box once per pixel, stopping at the opaque depth,
and composes the fog around it (`fog up to the box entry + fire + fog behind,
attenuated by the fire`, which reduces exactly to the old expression when no
box is on the ray). The fire is lit by an irradiance field the inject pass now
writes around each box (sun radiance x visibility, summed local-light radiance).
Boxes are created only within 20 m of the camera, held out to 25 m.

Cost, median of 5 runs, ms (base = the binary before stage 1a, `0d1b401ef`; sim, inject
and the `volumetrics` total are GPU brackets, composite is its own bracket and
now includes the march):

| scene | local sim | inject base → 1c | `volumetrics` total base → 1c | composite base → 1c | **total added** |
|---|---:|---|---|---|---:|
| s1_close_flame | 0.084 | 0.650 → 0.643 | 0.681 → 0.761 | 0.108 → 0.461 | **+0.43** |
| s2a_oil_blast_2s | 0.074 | 0.572 → 0.665 | 0.607 → 0.779 | 0.100 → 0.339 | **+0.41** |
| s3a_nuclear_blast_2s | 0.120 | 0.673 → 0.732 | 0.713 → 0.894 | 0.102 → 0.971 | **+1.05** |
| s5_skyrim_mare_hearth | 0.145 | 0.633 → 0.676 | 0.659 → 0.850 | 0.071 → 0.415 | **+0.53** |

(Measured after the follow-ups below: base and 1c interleaved run by run so GPU
clock drift hits both. The composite march skips empty occupancy bricks; the
nuclear scene's sim is now cheap because the flame no longer gets a second,
overlapping box during the blast.)

Where the cost is: the composite march, one sample per cell along every ray
through a box (s1's box covers about 60 % of the screen). Empty-space skipping
through coarse occupancy bricks (below) took it from about 0.5-0.7 ms to 0.3-0.5
ms on the sparse scenes; Phase 2's adaptive step size is the remaining lever.
On a box the gas fills (a dense blast) the per-step brick lookup is overhead
rather than a saving: that case measured slower with bricks than without in an
earlier run, and the interleaved numbers above are the ones to trust.

Checks run: renderer and workspace tests green apart from the unrelated
`ROADMAP.md` line ceiling; `check-shader-artifacts.sh` green; sync validation
clean on s1 and s5 (no VUID, no hazard); local grid state hash and the s1 final
frame identical across 3 runs.

Known problems (for review):

- **Explosions look different, not just sharper.** s2a and s3a show a larger,
  different fireball than the baseline at the same simulation time, and the
  room is lit differently by it. The source model (`applyCombustionSources`) is
  resolution dependent: it scales mass by how much of a cell the source covers,
  so a coarse froxel under-sources a thin shell that a fine cell captures in
  full. The baseline look was tuned against the coarse result.
- **The s3a orange blocks: traced and fixed.** They were not froxel gas and not
  a box face: with the close-range march off (`BYRO_LOCAL_GRID_DEBUG=no_march`)
  the froxels drew nothing at all there, so the blocks were local-grid
  content, frozen hot pockets that persisted unchanged between frames behind
  the blast. Turning BFECC off (`BYRO_BFECC=0`) removed them completely. The
  BFECC error correction is tuned for view-depth slabs of 0.6 m and more; on
  2.5-20 cm cells it is not needed (the semi-Lagrangian diffusion it cancels is
  already small) and it leaves those pockets. The local grid now runs without
  it (a seam in `combustion_transport.glsl`; the froxel pass keeps it). I did
  not find why BFECC misbehaves at this resolution; Phase 6 ("BFECC only where
  detail is still lost") is where that belongs.
- **Explosion boxes were too small.** An explosion's `FogVolume` is the swept
  bound of the authored effect (0.72 m for the lab's nuclear source) but the
  blast pushes its gas several metres; a box sized for the source cut the dome
  flat. Explosion boxes are now sized 16 source radii across (the 20 cm cell for
  the lab's two explosions) and centred vertically.
- **Flat faces remain** where a blast's gas crosses a box face into the froxels
  (visible at the right of the dome in the s3a sequence); one seam of the
  hand-off. `BYRO_LOCAL_GRID_DEBUG=no_march,no_handoff` bypasses the local grid
  entirely and is byte-identical to the pre-stage-1a binary on s1, s2a and s3a,
  so the froxel path is untouched.
- **The close flame is very bright and soft.** s1's core clips to white with a
  thin orange rim and little visible flame structure; the emission
  calibrations were tuned for the coarse field, which averages the peak
  temperature down.
- **Fire in a sunlit interior is unverified.** The march lights the fire from the irradiance field, whose sun term reuses the froxel pass's sun-visibility rule (open sky, architectural glass, authored apertures); none of the four scenes has a sunbeam on the fire, so that path has not been looked at.
- **Gas does not enter a box from the froxels** (open boundary: only outflow).
- Mean emission, not mean temperature, is what the coarse froxel state
  preserves (flame emission is exponential in temperature; a plain mean of the
  state radiates nothing and the room loses the fire's glow, which is what the
  first version did).

**Open items from the first review, worked through** (Matías asked for all three):

1. **Explosion look.** A reference render of the baseline binary with very fine
   froxels (`--froxel-xy-divisor 2 --froxel-z-slices 256`, 640×360×256, 22 ms for
   the inject pass alone, so only a yardstick) shows what the coarse baseline was
   approximating. For the oil blast (s2a) the local grid is much closer to that
   reference than the coarse baseline is: flame, smoke ball and the dimmer, less
   pink wall lighting all match it (`..._reference_fine_froxels.png`). So that
   difference is the old look being under-resolved, not a fault. For the nuclear
   blast (s3a) the reference keeps a thin grey smoke shell at the blast front
   that the local grid loses: plain semi-Lagrangian advection (BFECC is off, see
   above) at 20 cm cells smears a sheet that thin. Accepted for Phase 1 (Matías,
   2026-11-01) on the condition that the shell comes back later: tracked as a
   Phase 6 deliverable (see "Regain the nuclear smoke shell" there).
2. **Flat faces and the rectangular hole.** Two causes. (a) A rectangle cut out
   of the blast cloud was the flame's fine box nested inside the explosion's box:
   the finer box owns the shared space in the draw, its sim has never seen the
   blast's gas, so the cloud was missing there. Traced by giving only the
   explosion a box (`BYRO_LOCAL_GRID_DEBUG=only_explosions`: smooth round dome).
   Fixed by never nesting: an explosion box holds every source it covers and
   evicts any finer box it overlaps; the flame renders at the blast's cell size
   until the blast's box is gone, then gets its fine box back. A box also reads a
   covering box's field beyond its faces instead of clear air, for the overlaps
   that remain. (b) A genuine box face is now a cross-fade, not a cut: the march
   fades its density out and the froxel pass fades its share in over
   `LOCAL_GRID_FADE_CELLS` (8) cells.
3. **March cost.** The sim writes a brick of occupancy per 8^3 cells (this cell
   and its one-cell skirt); the march jumps over empty bricks instead of
   sampling 512 cells of clear air. The numbers are in the table above.

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

**Findings and work (2026-11-01).** Measured first, against a converged
reference: `BYRO_LOCAL_GRID_DEBUG=dense_march` takes eight samples per cell in
the composite march (pinned RT tier, final frame of s1/s2a/s3a/s5).

- The Phase 1 march (one jittered sample per cell, exact slab form) differs from
  that reference by a mean of 0.02-0.05 / 255 per pixel (largest single pixel 8
  on s1/s2a/s3a, 51 on s5). There is no stepping error in the local march to
  remove, and the reference costs 4-8x more. Energy-conserving integration was
  already in; jitter is resolved by TAA, which runs after composite, so no
  fire-specific history buffer was added.
- What remained was cost. An **adaptive step** now doubles the step after each
  quiet sample (clear gas, or less than 0.005 optical depth / HDR radiance per
  cell) up to 2 cells, and returns to 1 cell on the first busy sample or after
  an empty-brick jump. A 4-cell maximum was tried and rejected: mean error vs
  the reference 0.4-0.8 / 255, largest pixel 60-94. A 2-cell maximum with the
  0.05 busy threshold was worse than with 0.005.

| scene | mean error vs reference, before -> after | composite, before -> after (median of 7, interleaved) |
|---|---|---|
| s1 | 0.047 -> 0.173 | 0.416 -> 0.353 ms |
| s2a | 0.018 -> 0.126 | 0.415 -> 0.311 ms |
| s3a | 0.025 -> 0.046 | 1.115 -> 0.757 ms |
| s5 | 0.042 -> 0.047 | 0.414 -> 0.322 ms |

Error is small but larger than before on s1/s2a (99th percentile 1.7 and 3.0 /
255 in those scenes). Jitter keeps the estimate unbiased, so what it adds is
noise TAA integrates. **Reviewed by Matías (2026-11-01):** side-by-side
sequences of s1/s2a/s3a (Phase 1 march | adaptive | 8x reference, every 3rd
frame of a pan) "look good"; no flicker objection. Limit: the capture floor is
every 3rd frame (20 fps), so 60 Hz flicker was not covered.

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

### Phase 3 design note (2026-11-01)

Read before coding; sources were fetched from the primary documents, not
recalled.

**1. Multiple scattering: keep the closed-form series, soften its phase.**
Wrenninge's octave scheme (as summarized in Hillaire, *Frostbite sky and clouds*,
SIGGRAPH 2016 course notes, eqs. 19-20) sums octaves n = 0..N-1 with
sigma_s' = sigma_s a^n, sigma_e' = sigma_e b^n and phase eccentricity scaled by
c^n, and requires a <= b. The b^n term shortens the *light path* through the
medium. Our light terms never march through the smoke they light (the sun
visibility is a ray query against geometry, local lights get one transmittance
tap, and composite reads a prepared irradiance field), so b^n has nothing to
act on. The existing `1 + 0.6w + 0.3w^2` gain is the a^n part with a = albedo.
Decision: keep that gain, and give each octave its own phase, g_n = g * c^n with
c = 0.5 (Unreal's documented default for the same parameter; the Frostbite notes
give no value). Total scattered energy is unchanged (every phase integrates to
1); only its angular spread changes, so thick smoke loses the single peaked
lobe that makes it read flat.

**2. Species without a state or save change.** Steam and soot are today both the
`Smoke` profile; what already differs is the transported spectral albedo
(soot `[0.22, 0.16, 0.12]`, a white-tinted emitter up to 0.9, the no-colour
fallback 0.67). The albedo is advected with the gas, so a cell mixing steam and
soot gets an intermediate value for free. Decision: the species weight is a
function of the cell's mean albedo, `smoothstep(0.4, 0.9, albedo)`; no new
`FogProfile` variant (it would reach the save format) and no new field channel.
The catch: a mid-grey smoke at 0.67 lands halfway; the fallback albedo is a
candidate for revisiting if wood smoke reads too steamy.

**3. Phase per species.** Soot keeps the repo's existing dual lobe (forward
0.35, backward -0.12, mix 0.65), which is weaker-forward than fog. Steam uses
Jendersie & d'Eon 2023, *An Approximate Mie Scattering Function for Fog and
Cloud Rendering*, eqs. 2-7 (verified against the paper PDF):
`phi = (1 - w_D) HG(g_HG) + w_D Draine(g_D, alpha)` with, for droplet diameter
d in micrometres (fitted for 5 < d < 50),
g_HG = exp(-0.0990567 / (d - 1.67154)),
g_D = exp(-2.20679 / (d + 3.91029) - 0.428934),
alpha = exp(3.62489 - 8.29288 / (d + 5.52825)),
w_D = exp(-0.599085 / (d - 0.641583) - 0.665888).
The diameter is an artistic choice, not a measured one: 10 um, inside the fitted
range. It needs your eye, not a citation. The fit omits fogbow and glory.

**2 (revised, same day). Species is explicit, not albedo-derived.** Measured
before shipping: authored particle smoke is normalised to a peak albedo of 0.9
(`fog.rs`, `tint / peak * 0.9`), so an albedo threshold would have made all of it
steam. Steam is now `FogProfile::Steam` (GPU profile 8, `FogVolume` is not
saved, so no save-format change), whose sources write their extinction into the
optical texel's spare `w` lane; `w / sigma_t` is the steam share of a cell, and
the lane advects, dilutes and decays with the gas. `combustionSteamFraction`
reads it; `BYRO_LOCAL_GRID_DEBUG=no_species` forces the soot phase for A/B.
Authored (non-transported) media keep the weather / local-light lobes. Open
visual item: the `--combustion-lab-steam` scene shows the vent only faintly.

**Cost (interleaved medians of 7, composite pass, Phase 2 baseline -> now):**
s1 0.331 -> 0.389, s2a 0.308 -> 0.360, s3a 0.728 -> 0.861, s5 0.325 -> 0.549 ms
(s5 is noisy). A build with the sun-lit branch compiled out returned s1/s2a/s3a
to baseline, so the cost is the sun-lit code merely being present in the march
loop, with no sun anywhere in these scenes. Tried and not enough: gating on sun
visibility, per-ray octave lobes, lazy per-species evaluation, and splitting the
march into sun / sun-less functions (both end up inlined into one entry point,
so registers are sized for the heavier). **Fixed** with two specialised
composite pipelines (`LOCAL_FIRE_SUNLIT` specialization constant, picked each
frame from whether the local-grid table carries sun radiance). Interleaved
medians of 7, ms, Phase 2 baseline / one pipeline / two pipelines:
s1 0.372 / 0.413 / 0.356, s2a 0.292 / 0.337 / 0.306, s3a 0.716 / 0.897 / 0.713,
s5 0.347 / 0.388 / 0.378. Images are byte-identical to the one-pipeline build on
s1/s2a/s3a.

**Light direction for the march (2026-11-01).** The irradiance field carried no
direction, so the species phase only reached local lights in the froxel pass.
The field's image is now twice the froxel width: the second half holds the
radiance-weighted mean propagation direction of the lights (its length is their
coherence), written by the inject pass and blended with history like the
radiance. The march evaluates the species phase at that direction, falling back
to the isotropic phase where the lights cancel. Effect: the `no_species` A/B in
the steam lab now changes ~28,000 px (max 25/255), up from ~4,000 (max 11).
Cost, interleaved medians of 7, composite ms, Phase 2 baseline -> two pipelines
-> with direction: s1 0.355 -> 0.354 -> 0.511, s2a 0.309 -> 0.325 -> 0.401,
s3a 0.755 -> 0.791 -> 1.167, s5 0.466 -> 0.375 -> 0.440. The per-sample phase
(six pow(1.5) per soot sample) is the cost; a baked lookup over cos(theta)
would replace it.

**Steam lattice, found and fixed.** The lab steam showed a regular grid (one
froxel column, ~8 px). A froxel-only render (`no_march,no_handoff`) shows the
same grid with or without the species phase, so it is the froxel pass's own
jitter noise (per-froxel jittered sample position and shadow rays); the species
phase only made it visible, because steam's lobe is strongly angle-dependent
and soot's is nearly flat. Softening the lobe and pinning the steam share did
nothing. What worked: the light-direction field and the species phase's angle
are taken from the froxel's centre, not the jittered sample, and the direction
keeps a longer history (0.95; the radiance stays at 0.8). With those the
species render is as smooth as the soot-phase one. Steam still reads tan, not
white, in the lab: it is lit mostly by the flame's orange light and a frontlit
or off-axis steam cloud is dim by construction (its forward lobe is only a few
degrees wide). Not review-ready as a visual.

**Composite cost of the direction field, found and fixed.** The cost was
register pressure, not the phase arithmetic. `VK_KHR_pipeline_executable_properties`
on the RTX 4070 Ti: the sun-less composite pipeline needed 96 registers per
thread before the direction field and 126 after (fewer warps in flight for a
latency-bound march). Ablations, interleaved medians of 5, composite ms (s1 /
s3a): dropping the direction fetch and its phase 0.572 / 1.435 -> 0.426 / 0.861;
keeping the fetch but never taking the phase branch, 0.495 / 1.168 -> 0.482 /
1.097. So:
- A **baked phase table** (`combustion_phase_bake.comp`, 512 x 2 RGBA16F, rows
  soot / steam, x/y/z = octave 0/1/2, axis `((1 - cos) / 2)^(1/4)` so the steam
  forward peak spans ~35 texels) now replaces the per-sample lobes for both the
  local lights and the sun. It is evaluated once from the same functions the
  froxel pass uses, so the two cannot drift. On its own it was image-equivalent
  (max 1/255 on s1/s2a/s3a/steam) but saved little (s3a 1.31 -> 1.09 ms, the
  others within noise).
- Caching the two bracketing slices along the ray (one fetch per slice crossed)
  was tried and rejected: 128 registers with local-memory spill, no faster.
- **The fix:** the march took the steam share after the irradiance fetches,
  which kept the state and optics texels alive across them. Taken right after
  `combustionMedium`, the sun-less pipeline is back to 96 registers. The sun-lit
  one stays at 126 (119 before the direction field).

Result, HEAD (`588a923aa`) -> now, interleaved medians of 5, composite ms:
s1 0.600 -> 0.428, s2a 0.471 -> 0.356, s3a 1.163 -> 0.889, s5 0.439 -> 0.431,
steam lab 0.628 -> 0.483. That is the cost of the ablation without the
direction field, with the direction field kept. Final frames match HEAD within
1/255 on s1/s2a/s3a/steam (s5 differs run to run in either binary by the same
amount).

**4. Internal glow.** Not designed yet. First measure whether the derived fire
lights already light smoke from inside (A/B with them off), then decide.

*A/B attempt (2026-11-07, HEAD `2d6af0aeb`, release, tier 3, final frame of s1 /
s2a / s3a, `BYRO_LOCAL_GRID_DEBUG=no_derived_lights` against unset).* The switch
works but the whole-frame comparison cannot answer the question: the derived
lights also light the room, so 99.9 % of pixels change (mean brightness
122 -> 100 on s1, 106 -> 80 on s2a, 100 -> 74 on s3a; max channel delta 232 /
152 / 136). The change is the room losing the fire's glow, not smoke. The smoke
cannot be separated from the walls in these scenes without a view that shows the
volumetric contribution alone (none exists today: `RenderDebugMode` has no fog
or volumetrics view). Next step, before any 3b design: add that view, or A/B on a
scene whose backdrop is the sky. Code reading only: the derived lights join the
frame light list before upload, and the inject pass reads that list without
excluding them, so they feed the irradiance field the local march uses; whether
that is *visibly* internal glow is unmeasured.

**Stages.** 3a species phase + octave eccentricity, shared by the froxel inject
pass and the local march; 3b internal glow; 3c a steam-and-soot lab scene and
before/after captures for your review. Cost reported per stage.

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
- **Regain the nuclear smoke shell.** Phase 1 accepted losing the thin grey shell
  at the s3a blast front (plain semi-Lagrangian advection at 20 cm cells smears
  it). Candidates: BFECC with a limiter that works at fine cells (the Phase 1
  pockets were its failure mode), a finer explosion cell via a nested box, or
  burning-particle seeding for the front. Judge against
  `phase1/s3a_*_reference_fine_froxels.png`.

**Done when:** the same visual detail at a coarser grid, or more detail at the
same cost, measured against the Phase 0 baseline, and the s3a smoke shell is
back.

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
