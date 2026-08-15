# RT Lighting and Material Correctness Recovery

**Baseline:** 2026-08-14, `main` at `c25f61e6` before the first recovery
edit. This plan is the execution contract for ray-traced lighting and material
correctness. `ROADMAP.md` remains the project-wide source of truth; this file
owns the detailed recovery sequence, evidence format, and exit gates.

## Outcome

The recovery is complete only when a fixed frame can answer, independently:

1. which authored light entered the renderer;
2. which lights survived SSBO and cluster assignment;
3. which light the reservoir selected;
4. which render instances entered the TLAS and why any did not;
5. whether the selected visibility ray hit and what it transmitted;
6. which material roles, colour spaces, and lobes were evaluated;
7. how direct, indirect, caustic, volumetric, water, bloom, and tone-map terms
   contributed to the final pixel.

The durable deliverable is not one good screenshot. It is a deterministic
capture harness, a machine-readable integrity snapshot, one debug view per
layer, and a redistributable scene ladder that fails at the first broken
contract.

## HEAD reconciliation

The original R0-R6 proposal predates several fixes now on `main`. Work must not
re-implement or revert them.

| Area | Already present at baseline | Remaining recovery scope |
|---|---|---|
| R0 measurement | Named benchmark modes; `renderer-stepped` owns a fixed 1/60 s delta; moving pan/orbit/dolly/cut cameras; scene fingerprints; five-scene stepped-camera bench at `34074b93` | Three-run verdict reproducibility; a true HEAD-vs-anchor visual predicate; separate performance and correctness thresholds |
| R1 ingestion | XCLL rotation fields renamed; the axis-invariant test is ignored as an explicit xfail; punctual per-light flat fill removed | Remove the XCLL type-3 `Lo` bypass; validate an XCLL-specific angle conversion; emit kind/direction/fade/provenance in `light.dump` |
| R2 TLAS | Missing TLAS instances split into skinned/rigid/SSBO causes; `patch_camera_rt_flag`; off-frustum occluders retained; AS publication and shrink synchronization fixed by `c25f61e6` | Persist counters beyond rate-limited logs; cluster overflow telemetry; four-scene runtime integrity captures; a forced-low-budget eviction test |
| R3 transport | Wächter-Binder-style `offsetRayOrigin`; shared material-aware shadow transport | Visibility-only and selected-light views; eliminate residual manual fixed offsets/tMin values; derive `RT_LOD_SCALE` from a sweep |
| R4 oracle | Redistributable Cornell scene; one cube golden test; same-revision upscaler SSIM test | Six-rung Cornell lighting/material ladder and CI comparison artifacts |
| R5 materials | `GpuMaterial` flags generated from Rust; semantic `MaterialTextureSet`; one NIF slot-to-role table; FO3/FNV TXST-to-NIF role permutation pinned | Material provenance dump; BGSM diffuse-lobe contract; complete REFR/BGSM role forwarding; lobe view; cross-game fixture matrix |
| R6 contracts | `lighting-from-cells.md` describes directional and ambient as separate controls | Reconcile renderer/shader/material docs with live code and pin critical GPU-layout claims |

The old `28155b79` benchmark and its issue #2367 are historical evidence, not
a current bisect predicate. Prospector and Dugout used incorrect/empty framing
on that harness. They must be remeasured with identical stepped-camera code on
both candidate and reference binaries before any performance conclusion is
carried forward.

## Execution status

- **R0 complete.** `check-bench-determinism.sh` produces three cold-process
  manifests, and `check-render-anchor.sh` compares explicit binaries over the
  static/pan/orbit/dolly/cut paths without mutating the caller's worktree.
  Three same-binary matrices and three `c25f61e6`-vs-`77b540d0` matrices
  passed; the bisect wrapper returns `0` for the clean control and `101` for a
  controlled 64x64 corruption. Raw stochastic metrics remain diagnostic while
  the fixed linear low-pass metric is the correctness gate.
- **R1 transport-facing work complete; provenance refinement remains.** XCLL
  directional colour emits a type-2 directional light with no type-3/flat-`Lo`
  bypass. The parser names azimuth/elevation explicitly and the dedicated
  `xcll_direction_yup` conversion is pinned by axis tests plus an ignored
  real-FNV census (388 XCLL cells, 252 active directionals, 96 at the authored
  `(0°, 270°)` overhead convention). `light.dump` reports every live light's
  kind, direction/position, photometric values, visibility and entity/FormID
  provenance. Exact translation source, GPU index, and assigned-cluster count
  are still not retained per light.
- **R2 complete for normal runtime pressure.** Persistent RT-integrity and
  fence-lagged cluster telemetry were captured on Dugout Inn, MedTek,
  Prospector, and Cydonia. Cydonia exposed two capacity failures: 656 live
  lights exceeded the old 512-light SSBO, and clusters reached 305 candidates,
  dropping 3,729 references at the old 128 cap. `MAX_LIGHTS = 1023` now leaves
  packed ReSTIR index 1023 as the invalid sentinel; the per-cluster cap is 512.
  Adaptive cold-start ray budgeting prevents the former Cydonia device loss.
  A forced-low-BLAS-budget eviction/rebuild recovery test is still required.
- **R3 core transport complete; measurement closure remains.** Selected-light,
  shadow-visibility, direct, raw-indirect, material-lobe, and RT-LOD views are
  available through the existing debug selectors. Every correctness view
  bypasses composite fog/caustics/bloom/dither, temporal FSR dispatch,
  presentation grading, exposure, ACES, underwater and fade processing. Every
  secondary-ray consumer now uses the shared representable-float origin offset
  with numerical `tMin = 0`. The first L0 capture also exposed and removed the
  shader's hard-coded no-light directional fallback, so zero submitted lights
  now leave direct transport at zero. A bounded pixel probe, a runtime
  enum/command, and the measured `RT_LOD_SCALE` sweep remain open.
- **R4 L0-L2 scene and manual runtime gate complete.** `--cornell-oracle l0|l1|l2`
  constructs the ladder from one manifest: a dark white plane, the same plane
  under one analytic directional source, then the same scene with one opaque
  blocker. CPU tests pin CLI selection, one-variable progression, source unit
  length/Lambert expectation, and shadow/control ray geometry. RTX 4070 Ti
  captures using the raw direct/direct/visibility selectors passed TLAS/light/
  cluster integrity: L0 is exactly black, L1 is spatially constant, and L2
  contains the predicted black blocker-shadow silhouette on white visibility.
  The ignored `cornell_rt_oracle` integration gate now captures those three
  frames, requires `rt-integrity verdict=PASS`, and checks analytic L0/L1 plus
  blocked/control L2 pixels. It passes locally on the RTX 4070 Ti and is ready
  for the RT-capable CI worker. CI scheduling/artifact publication remains;
  L3-L5 are not built.
- **R5 core role/flag observability complete; fixture breadth remains.** The
  PBR/translucency/model-space-normal bits are generated with the other shader
  constants, the FO3/FNV TXST↔NIF 2-5 permutation is explicit, `mat.dump`
  prints all semantic roles with path/source/binding/dimensionality/colour
  space, and the lobe view is raw. Vanilla BGSM stays on its authored legacy
  spec-gloss lobe unless a resolved template explicitly opts into PBR. Current
  provenance still coalesces inline NIF and external material-file roles as
  `MeshMaterial`; per-format source fields and the five-game fixture matrix
  remain open.
- **R6 partially complete.** `renderer.md` is reconciled with the live light,
  cluster, origin, debug-view, flag and dump contracts. This plan, ROADMAP, and
  the adjacent lighting/shader/material documents are being brought to the
  same checkpoint; CI enforcement for documentation drift remains future work.

## Recovery rules

- **Fix forward from current `main`.** A wholesale Session-63 revert would
  discard the deterministic bench mode, scale-aware origin offset, canonical
  role mapping, generated flags, and recent AS fixes. A narrow revert remains
  valid only when the new predicate identifies a single introducing commit.
- **Freeze new TLAS consumers.** No new ray-query feature or additional ray
  from an existing feature lands before R3 exits.
- **Freeze unrelated visual goldens.** Expected images authored before R3 may
  encode a bug. R4 oracle images are the exception because their terms and
  expected values are controlled explicitly.
- **Existing consumers are audit scope, not future freeze scope.** Volumetric
  visibility and water reflection/refraction/caustics are already live at
  HEAD. They stay enabled for the consumer inventory and are changed only to
  adopt a corrected shared transport contract.
- **Headless work may proceed.** Water resolution/submersion/current/coverage
  and gameplay/combat work that neither consumes the TLAS nor authors renderer
  goldens is independent.
- **No broad renderer issue sweep during recovery.** New findings arise from
  a failed named gate and include its artifact. This prevents another large
  write-only audit batch with no visual predicate.
- **One behavioral claim per commit.** Instrumentation precedes fixes, and a
  fix commit includes the regression test or fixture that would have caught it.

## Dependency ladder

```text
R0 deterministic predicate
   |
   +--> R2 membership/cluster proof ----+
   |                                    |
   +--> R1 authored-light proof --------+--> R3 selected ray + visibility
                                                |
                                                +--> R4 Cornell oracle
                                                |
                                                +--> R5 material/role closure
                                                          |
                                                          +--> R6 contract close
```

R1 and R2 can be developed independently after R0, but both must be green
before a shadow-transport result is interpreted.

## R0 - Restore a trustworthy predicate

### R0.1 Three-verdict determinism gate

Extend `scripts/check-bench-determinism.sh` so it runs each selected workload
three times, not twice:

- Cornell, `renderer-static`, static camera;
- Cornell, `renderer-stepped`, orbit camera;
- one real-content scene, `renderer-stepped`, its frozen camera path.

Each run writes a manifest containing engine commit, harness commit, binary
hash, scene fingerprint, mode, camera path, upscaler, resolution, warm-up
frames, captured frame numbers, driver/device identity, and relevant debug
mode. A verdict is identical only when all state fields and the comparison
classification match; byte-identical timing is not required.

**Tests:** benchmark-mode unit tests; parser tests for manifest mismatch;
intentional camera/fixed-step perturbations must fail the fingerprint gate.

**Exit:** three consecutive identical PASS/FAIL verdicts at current HEAD and
at the chosen anchor.

### R0.2 HEAD-vs-anchor capture runner

Add a runner that accepts two explicit binaries. It must never switch or
mutate the caller's worktree during capture. Build the anchor in a detached
worktree or provide it as `--reference-bin`; run the current checkout as
`--candidate-bin`.

Use five camera cases:

1. static, for stable single-frame diagnosis;
2. pan;
3. orbit;
4. dolly;
5. cut, for temporal reset/disocclusion.

Compare the same post-warm-up frame range in linear space. Store both raw and
fixed 5x5 low-pass SSIM, absolute error percentiles, outlier fraction, and image
dimensions. The low-pass metric is the gate: the renderer contains stochastic
RT sampling, so raw pixelwise agreement is diagnostic and can change when a
semantically irrelevant shader edit produces a different noise realization.
Correctness and
performance are separate predicates:

- correctness fails on manifest/fingerprint mismatch or an image threshold;
- performance fails only against the measured same-machine variability
  envelope, never a hard-coded percentage from an old harness.

The runner emits a single stable exit code suitable for `git bisect run` and a
directory containing both images, a diff heatmap, manifests, and metrics.

Implemented as `scripts/check-render-anchor.sh` plus the ignored
`renderer_anchor` integration test. The wrapper defaults to an owned Xvfb
display, accepts `BYROREDUX_ANCHOR_XVFB=0` for an existing display, and stages
the explicit binaries as stable hard links (copy fallback across filesystems)
before Cargo can rebuild anything. The committed correctness thresholds are
the three-run HEAD noise floor with margin; performance thresholds come from
the measured same-machine p50/p95 envelope above. `summary.json` exposes
`correctness_passed` and `performance_passed` independently as well as the
combined bisect verdict.

Reproduction:

```bash
scripts/check-render-anchor.sh /path/to/reference/byroredux \
  /path/to/candidate/byroredux target/renderer-anchor 60

# From an active bisect; this builds each checked-out candidate first.
git bisect run scripts/bisect-render-anchor.sh \
  /path/to/immutable/reference-byroredux target/renderer-anchor-bisect 60
```

**Exit:** three repeated baseline-vs-candidate runs PASS, the bisect wrapper
returns a stable good verdict, and a controlled visual fault returns a stable
bad verdict. A real regression range can now use the same command without
changing the predicate.

### R0.3 Issue/roadmap disposition

Treat #2367 as a superseded-harness measurement until the two historical
commits are rebuilt and captured through the same current runner. #2161 is
already closed and must not be reopened from old numbers. Record new results
as a new evidence artifact rather than editing old raw tables.

## R1 - Prove authored light ingestion

### R1.1 Separate ambient irradiance from the directional key

XCLL flat ambient and six-axis directional ambient remain on the camera/DALC
path. XCLL `directional_color + directional_azimuth/elevation + directional_fade`
emits one type-2 directional `GpuLight` with full structural visibility.

Remove the type-3 normal-independent branch from the main and GI shaders. No
directional colour may be added directly to `Lo` without N.L, BRDF, reservoir
selection, and visibility. Keep the existing 0.6 fallback only as the
pre-Skyrim missing-`directional_fade` source calibration.

**Tests:** CPU upload type/mask tests; shader-source canary forbidding the
type-3 bypass; generated SPIR-V rebuilt in the same commit.

### R1.2 XCLL-specific angle conversion

Do not change `core::math::coord::euler_zup_to_quat_yup`; it is the canonical
REFR/NIF Euler helper. XCLL carries two semantic angles, not a complete REFR
Euler triple. Introduce a named conversion at the cell-lighting translation
boundary, for example:

```rust
fn xcll_direction_yup(azimuth_cw: f32, elevation: f32) -> Vec3
```

Resolve its signs with this evidence order:

1. dump the full FNV directional population and range/histogram;
2. pin known cells with visibly directional authored lighting, including
   Prospector;
3. compare candidate vectors with the previous spherical path and current
   REFR-Euler path;
4. consult GECK/Creation Kit output only if the corpus and known cells remain
   ambiguous.

Tests cover zero, both signed quarter turns, unit length, and at least two
real XCLL records. Delete the xfail only when azimuth changes the result and
the real records pin the chosen convention.

### R1.3 `light.dump`

Preserve a CPU-side debug record for every uploaded light:

- GPU index and ECS entity, when any;
- source: `Xcll`, `Lgtm`, `WeatherSun`, `NifLight`, `Ligh`, `FireVolume`;
- canonical kind, input colour, resolved radiant colour;
- authored and effective range/source radius;
- authored XCLL angles and resolved Y-up direction;
- directional fade/sun scale;
- visibility mask and attenuation model;
- assigned cluster count and whether it was reservoir-eligible.

`light.dump [index]` returns structured JSON through the existing debug
protocol and a compact CLI view. Provenance is captured at translation time;
the command must not reverse-engineer it from `GpuLight` sentinels.

**Exit:** Prospector's XCLL source dumps as directional, its direction changes
with authored azimuth, and its separate ambient term is visible in the cell
lighting section rather than as a fake GPU light.

## R2 - Prove cluster and TLAS integrity

### R2.1 Persistent RT integrity snapshot

Promote the local/rate-limited acceleration counters into a per-frame
`RtIntegritySnapshot` exposed through renderer stats:

- RT requested, TLAS write succeeded, camera RT flag published;
- TLAS-eligible draws and emitted instances;
- missing skinned BLAS, rigid BLAS, and SSBO instance counts plus bounded
  offender samples;
- static/skinned BLAS bytes, configured budget, evictions and deferred
  destroys;
- TLAS build/refit mode and instance count;
- scene fingerprint and frame number.

Add `rt.integrity` to print the latest snapshot. Keep the existing warning,
but make the command and benchmark artifact authoritative.

### R2.2 Cluster overflow telemetry

Add a small per-frame `ClusterCullStats` storage buffer containing:

- number of clusters whose candidate count exceeded
  `MAX_LIGHTS_PER_CLUSTER` (512 after Cydonia proved the old 128 cap lossy);
- maximum candidate count observed;
- total dropped assignments;
- frame/light-count identity.

Clear before `cluster_cull.comp`, update with atomics when `sharedCount` exceeds
`MAX_LIGHTS_PER_CLUSTER`, copy to per-frame-in-flight host readback after the
dispatch, and consume only after the owning fence. Do not map/read the device
buffer synchronously in the current frame.

The descriptor binding and constants must come from the shared Rust/GLSL
contract, with a layout/offset test and a shader-source constant test. Export
the result through `rt.integrity` and benchmark manifests.

### R2.3 Reproducible eviction pressure

Add a test-only/diagnostic BLAS-budget override with an explicit byte value and
startup log. It must be rejected outside diagnostic builds or require an
unmistakable `--rt-test-blas-budget-mb` CLI flag. Use it to force eviction on
the development GPU and prove one of two contracts:

- visible/eligible rigid BLAS are pinned and never evicted; or
- an evicted eligible static BLAS is queued for on-command rebuild before the
  next TLAS publication.

The current state, where an evicted rigid BLAS can remain absent until cell
reload, is not an acceptable final contract.

### R2.4 Runtime matrix

Capture two windows—warm-up and steady-state—on:

- Dugout Inn;
- MedTek Research 01;
- Prospector Saloon;
- Cydonia.

Use renderer-stepped mode, fixed cameras, Vulkan validation, and the same
upscaler. Archive `rt.integrity`, `light.dump`, GPU timers, validation output,
and the captured frame manifest.

**Exit:** one named steady-state frame per scene has `rt_enabled = true`,
eligible instances equal emitted instances, missing rigid/SSBO counts zero,
and cluster overflow either zero or an explicitly tested overflow policy.
Transient skinned first-sight misses must reach zero after warm-up.

## R3 - Prove selected shadow transport

### R3.1 Structured debug mode, not more bit flags

The current `BYROREDUX_RENDER_DEBUG` bitset is nearly exhausted and only read
at process startup. Add a `RenderDebugMode` enum carried in an explicitly
reserved camera/debug field and settable by `render.debug <mode>`:

- `final`;
- `shadow_visibility`;
- `selected_light`;
- `direct_only`;
- `indirect_only`;
- `material_lobe`;
- `composite_term`.

Keep orthogonal low-level toggles as flags. A mutually exclusive visualization
belongs to the enum. Unknown values render magenta and log once.

### R3.2 Visibility-only view

After the final reservoir sample is revalidated, output its scalar/broadband
transmission directly as greyscale before SVGF, composite, exposure, bloom and
ACES. Encode invalid/no-candidate separately from a visible ray so black never
means both "occluded" and "nothing selected". Capture selected light index,
ray origin/direction/tMin/tMax, mask, committed-hit instance and distance in a
bounded pixel probe record.

### R3.3 Selected-light view

Hash the final selected `GpuLight` index to a stable false colour. Reserve
black for no candidate and magenta for an out-of-range index. The pixel probe
must also print the selected light's `light.dump` record, making a mismatch
between index and ray geometry observable in one round trip.

### R3.4 Shared origin/range contract

Inventory every `rayQueryInitializeEXT`. Classify each minimum distance as:

- numerical self-intersection avoidance;
- physical thickness/segment exclusion;
- LOD/range policy.

Numerical cases use `offsetRayOrigin` and zero/ULP-safe tMin. Replace remaining
manual `position +/- normal * 0.05` origins in water, glass, reflection,
refraction, caustic and GI paths. A non-zero semantic distance remains only as
a named generated constant with a test and units; no anonymous `0.05` remains
in a ray initializer.

### R3.5 RT LOD derivation

Sweep `RT_LOD_SCALE` over a declared range on Cornell plus the four real
scenes. Record traced/culled hits, visibility SSIM against the no-LOD reference,
and ray-query GPU time. Choose the smallest scale inside the visual threshold
and write the measured contract next to the generated constant. Correct the
comment in the same commit.

**Exit:** for a fixed pixel, selected-light colour, probe index, ray geometry,
visibility image and committed hit agree; visibility is stable across three
runs and across a large-coordinate scene.

## R4 - Build the Cornell L0-L5 oracle

Represent the ladder as data, not six ad-hoc scene constructors. Each rung has
a scene manifest, analytic expectations, capture camera, debug mode, and image
threshold.

| Rung | Adds exactly one variable | Primary assertion |
|---|---|---|
| L0 | albedo plane, no emitted/ambient light | radiance is zero before display encoding |
| L1 | one white directional, specular-disabled Lambert plane | constant N.L and energy match the analytic CPU result |
| L2 | one opaque blocker | binary visibility mask and penumbra-free hard edge match geometry |
| L3 | point/spot cluster plus reservoir selection | candidate list and selected-light identity are deterministic |
| L4 | one diffuse bounce and coloured Cornell walls | indirect-only channel has expected sign, colour bleed and bounded energy |
| L5 | dielectric, metal, glass and normal-role probes | lobe/role view selects the declared material contract |

L0-L2 enter CI first. Prefer scalar probe assertions and tolerant linear-image
metrics over exact PNG hashes. Store expected images only after their analytic
and debug-channel assertions pass. L3-L5 may run on the RT-capable integration
runner until CI has equivalent hardware.

Every renderer bug fixed after this plan must name the earliest rung that
would catch it; if none does, add a rung/variant before closing the bug.

**Exit:** each deliberate fault—wrong light type, missing TLAS instance,
flipped visibility, wrong material lobe—fails a different earliest rung.

## R5 - Close material and texture-role correctness

### R5.1 Keep one canonical role walk

The NIF `BSShaderTextureSet` slot table and ESM `TXST` role translation now
exist and the FO3/FNV 2-5 permutation is pinned. Finish the consolidation:

- replace `RefrTextureOverlay::fill_from_bgsm`'s hand-written subset (#2594);
- replace the supplemental-index hand walk (#2697);
- forward all BGSM/BGEM roles supported by `merge_external_material`, including
  lighting, flow, wrinkle, greyscale/LUT, inner and specular roles;
- add an explicit consumer or a documented unsupported diagnostic for the
  currently stranded inner/refractive role (#2713/#2627).

The shared helper yields `(MaterialTextureRole, path, source)` records. Each
consumer decides how to store the canonical role; it does not repeat source
slot numbers.

### R5.2 Make the FO4 diffuse-lobe contract explicit

Issue #2700 proves the present `pbr` bit gate selects Disney for 0 of 6,616
vanilla BGSMs. Replace the ambiguous `is_pbr` source-format inference with an
explicit canonical diffuse-lobe decision at material translation. Use three
fixture classes—painted dielectric, bare metal, skin/cloth—and capture
Lambert-vs-Disney direct-only/lobe views under identical lighting.

Default direction for the decision: a successfully resolved vanilla BGSM uses
the FO4/Creation material diffuse contract established by #1352; the wire
`pbr` bit may refine a material but cannot be the sole gate when it is absent
from the entire vanilla corpus. If measurement disproves that, document the
material-class rule explicitly. In either case, remove stale mirror tests and
add a real provider-backed merge test.

### R5.3 `mat.dump <entity>`

Print the translated material and one row per canonical role:

- role and resolved path;
- source: `NifTextureSet`, `TxstOverride`, `Bgsm`, `Bgem`, `Mat`, `Mswp`;
- source slot/field for diagnostics only;
- GPU material index and sampler binding;
- 2D/cube/LUT dimensionality;
- sRGB or linear view;
- present, placeholder, missing, or unsupported state;
- material flags, material kind, diffuse lobe and active BRDF lobes.

Capture provenance while translating/merging. Never infer `TxstOverride` by
comparing final paths after the fact.

### R5.4 Lobe/role visualization

`material_lobe` mode maps diffuse, GGX specular, sheen/subsurface,
transmission, emissive, clearcoat/environment and unsupported combinations to
stable colours. A sibling role mode shows the sampled normal model and the
source of the active diffuse/normal/environment textures.

### R5.5 Cross-game matrix

Pin at least one fixture from Oblivion, FNV, Skyrim SE, FO4 and Starfield, plus
synthetic tests. Assertions cover role paths, view colour space, cube-vs-2D,
normal-space flag, lobe, and placeholder count. FO4 must include a TXST 2-5
permutation case and an MNAM-only REFR overlay.

**Exit:** every sampled material texture on the matrix has a canonical role,
source and colour-space decision; no renderer branch asks which game/file
format produced it; direct-only/lobe captures are stable across three runs.

## R6 - Reconcile and pin the contracts

Update together:

- `docs/engine/renderer.md`;
- `docs/engine/shader-pipeline.md`;
- `docs/engine/lighting-from-cells.md`;
- `docs/engine/material-abstraction.md`;
- this plan and the active section of `ROADMAP.md`.

At minimum reconcile:

- `GpuLight` types and all four `params` fields;
- cluster and reservoir flow;
- current TLAS consumers and cull masks;
- origin/tMin and RT LOD policy;
- generated material flags and the 348-byte `GpuMaterial` layout;
- canonical texture roles and FO3/FNV TXST permutation;
- water and volumetric consumers already live at HEAD.

Add source-contract tests for GPU struct sizes/offsets, generated constants,
and descriptor bindings. Documentation should link those tests instead of
duplicating volatile counts where possible. `/session-close` must fail its
renderer-contract check when these files predate a GPU layout/consumer change.

**Exit:** a fresh codebase handoff can derive the same L0-L7 model from docs
and code, and the open documentation drift issues (#2781, #2917) are closed by
tests plus corrected text.

## Commit sequence

Keep the series bisectable:

1. `docs(renderer): baseline RT lighting and material recovery plan`
2. `fix(renderer): route XCLL directional lighting through visibility`
3. `test(bench): add three-verdict anchor visual predicate`
4. `feat(debug): expose persistent RT integrity snapshots`
5. `feat(renderer): count cluster light overflow`
6. `fix(lighting): translate XCLL directional angles explicitly`
7. `feat(debug): add light provenance and selected-ray views`
8. `fix(renderer): unify ray origin and minimum-distance policy`
9. `test(renderer): add Cornell L0-L5 correctness ladder`
10. `fix(materials): unify external-material role forwarding`
11. `fix(materials): pin FO4 diffuse-lobe translation`
12. `feat(debug): add material provenance and lobe views`
13. `docs(renderer): reconcile live RT and material contracts`

Instrumentation commits do not change rendered output. Behavioral commits
include before/after artifacts and the first regression gate that changes from
FAIL to PASS.

## Triage decision table

For a failed fixed frame, stop at the first failing row:

| Evidence | Conclusion | Next action |
|---|---|---|
| RT flag false or TLAS write failed | RT disabled, not a shading bug | fix publication/synchronization |
| missing rigid/SSBO instances non-zero | incomplete TLAS | fix pin/rebuild/instance mapping |
| cluster overflow non-zero | truncated candidate set | resize/tile/prioritize with an explicit policy |
| selected-light view wrong | cluster/reservoir identity bug | debug indices/weights/history before rays |
| selected light correct, visibility wrong | origin/range/mask/AS transport bug | use pixel probe and visibility view |
| visibility correct, direct-only wrong | BRDF/light-ingestion bug | inspect `light.dump` and lobe view |
| direct correct, indirect wrong | GI/SVGF bug | inspect indirect-only/history channels |
| lighting terms correct, final wrong | composite/exposure/bloom/fog bug | isolate composite terms |
| lobe correct, texture wrong | role/path/view/sampler bug | inspect `mat.dump` |

This table is the working meaning of "fix it once and for all": future
failures become a bounded layer diagnosis instead of another renderer-wide
theory sweep.
