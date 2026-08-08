# Renderer Audit — 2026-08-07

**Scope: Full sweep, all 23 dimensions, depth=deep.**

- **Dimensions run**: 23
- **Dimensions producing findings**: 22
- **Dimensions returning clean**: 1 (Dimension 13 — TAA)
- **Raw findings filed by dimension agents**: 66
- **Unique findings after cross-dimension dedup**: 65 (one cross-dimension duplicate merged — see Executive Summary)

Repo: `/mnt/data/src/gamebyro-redux` · Branch: `main`

---

## Executive Summary

### Findings by severity

| Severity | Count | Notes |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 1 | AS build-scratch peak walk ignores live skinned BLAS |
| MEDIUM | 22 | — |
| LOW | 42 | 43 raw, minus one merged cross-dimension duplicate |
| **Total (unique)** | **65** | 66 raw findings across 23 dim files |

### Cross-dimension dedup applied

Exactly **one** cross-dimension duplicate was found and merged:

- **`REN-D3-2026-08-07-03`** (Dimension 3 — GPU-Struct Layout) and **`MAT-D7-2026-08-07-01`**
  (Dimension 7 — Material Table) both report the stale `300 B` `GpuMaterial` stride in the
  `MAX_MATERIALS` docstring at `crates/renderer/src/vulkan/scene_buffer/constants.rs`. D3's
  finding is broader (it also covers two `112 B` → `128 B` `GpuInstance` comment sites), D7's is
  narrower but carries its own evidence and a distinct fix suggestion. They are merged into a
  single LOW finding carrying **both** dimension tags and **both** original bodies verbatim.
  This is why the unique count is 65 against 66 raw `**Severity**:` entries in the dim files.

No other overlap survived checking. Specifically checked and found **not** duplicated:
dims 1/2/3 on acceleration-structure ↔ SSBO ↔ GPU-struct topics (D1 is AS-lifecycle, D2 is
shader-side ray-query semantics, D3 is host↔GLSL layout pinning — disjoint); dims 8/14/15 on
caustics/composite (D8 is the composite `is_sky` branch and composite doc-rot, D14 is the glass
caustic EMA/decay, D15 is the *water* caustic accumulator and its composite binding-8 fallback —
disjoint resources and disjoint code paths); dim 5 vs dim 1 on BLAS scratch (D5 verified the
deferred-destroy routing clean and filed nothing there; D1's HIGH is about the shrink *target*,
not the destroy *timing*); dim 7 vs dim 21 on the inert Disney scalars (D7 explicitly declined to
re-file it as already-documented; only D21 files it).

### Clean dimension

**Dimension 13 (TAA, M37.5) returned ZERO findings.** Every checklist item — Halton jitter
period/indexing, un-jittered `fragCurrClipPos`, per-FIF history slots, YCoCg variance clamp,
mesh-ID disocclusion with the bit-31 alpha marker masked, first-frame `should_force_history_reset`,
descriptor/pool derivation, and the disable path — traced to live code and passed. The dimension
agent additionally developed three candidate findings and then *falsified* all three rather than
shipping them as noise (MFIF≥3 prev-slot arithmetic, `recreate_on_resize` failure state, and the
infallible-`dispatch` dead fallback — the last already recorded in `AUDIT_RENDERER_2026-07-09.md`).
This is a genuinely clean dimension, recorded here explicitly so its absence from the Findings
section is not read as an omission.

### Pipeline areas affected

| Area | Dimensions | Findings | Peak severity |
|---|---|---|---|
| Acceleration structures (BLAS/TLAS) | 1 | 2 | HIGH |
| Ray queries / RT shading (`triangle.frag` + includes) | 2 | 3 | MEDIUM |
| GPU-struct ↔ GLSL layout pinning | 3 | 3 | MEDIUM |
| Synchronization & barriers | 4 | 3 | MEDIUM |
| GPU memory & resource lifecycle | 5 | 2 | LOW |
| NIFAL canonical material translation | 6 | 4 | MEDIUM |
| Material table / dedup | 7 | 3 | LOW |
| Denoiser (SVGF) & composite | 8 | 3 | MEDIUM |
| GPU skinning + skinned BLAS refit | 9 | 3 | MEDIUM |
| Camera-relative render origin / f32 precision | 10 | 2 | LOW |
| Pipeline state / render pass / G-buffer | 11 | 5 | LOW |
| Command-buffer recording | 12 | 3 | LOW |
| TAA | 13 | 0 | — |
| Caustic splat (glass/MLP) | 14 | 4 | MEDIUM |
| Water (M38) + water-side caustics | 15 | 3 | MEDIUM |
| Volumetrics (M55) & bloom (M58) | 16 | 2 | MEDIUM |
| Disney BSDF / PBR gating + soft shadows | 17 | 3 | MEDIUM |
| Sky / weather / exterior lighting | 18 | 2 | MEDIUM |
| Tangent space & normal maps | 19 | 2 | MEDIUM |
| Debug overlay & GPU telemetry | 20 | 3 | MEDIUM |
| Cornell-box RT harness | 21 | 3 | MEDIUM |
| Light animation canonical translation | 22 | 4 | MEDIUM |
| FSR 3.1 upscaler & presentation chain | 23 | 4 | MEDIUM |

### Headline

The single HIGH is a **GPU-memory-safety** defect in the shared BLAS build-scratch shrink policy
(`AS-D1-NEW-01`): the shrink target is computed from static BLAS only, so a live skinned entity's
next refit can submit an `UPDATE` whose scratch range runs past the reallocated buffer. It is
reachable on a plain window resize with NPCs on screen, and the prior `#1127` closeout that
dismissed the adjacent memory angle rests on a factually wrong premise (cell unload does *not*
drop skinned BLAS synchronously).

The MEDIUM band clusters into four themes:

1. **Shading-correctness asymmetries** — a normal, a normalization, or a coordinate convention
   applied on one branch and not its sibling: GI hemisphere axis vs. its own ray origin (D2),
   compute-vs-raster zero-weight skinning fallback (D9), view-flipped normal reused for a
   light-side computation (D15), `1/PI` applied to the DALC ambient arm but not its `sceneFlags.yzw`
   sibling (D17), and α-vs-α² in the specular-AA filter (D17).
2. **Unpinned or mis-pinned host↔GPU contracts** — `GpuTerrainTile` and `DalcCubeUBO` are
   hand-mirrored with no size/offset/lockstep pin (D3), a `SUBPASS_EXTERNAL` dependency whose
   `FRAGMENT_SHADER` limb loosens the swapchain layout transition below the acquire semaphore's
   scope (D4), and an `EguiPass` render pass that survives a swapchain *format* change (D20).
3. **Canonical-boundary drops** — `grayscale_to_palette_scale` parsed from both authoring sources
   and dead-ended before `Material` (D6), and pre-Skyrim LIGH flicker *parameters* decoded at
   Skyrim offsets so FNV/FO3/Oblivion torches provably cannot animate (D22).
4. **Temporal/history invalidation gaps** — the parked-camera caustic EMA has no dynamic-scene
   invalidation (D14), the integrated froxel volume is sampled half a slab deep (D16), and the
   composite `is_sky` branch discards alpha-blended geometry silhouetted against open sky (D8).

The LOW band is dominated by documentation/contract rot (roughly 20 of 42) — stale byte counts,
stale line citations, comments describing removed branches — plus a set of latent structural traps
(unconditional slot overwrite, infallible-`Result` escape hatches, `expect()` inside an open render
pass) that are unreachable today but have no type-level guard.

---

## RT Pipeline Assessment

*(Summarizing Dimensions 1, 2 and 8: BLAS/TLAS lifecycle, SSBO indexing, ray-query safety, denoiser
stability.)*

### BLAS / TLAS (Dimension 1)

The acceleration-structure layer is in good shape structurally, with one real safety hole.

**Correct and verified holding**: geometry format (`R32G32B32_SFLOAT` / `UINT32` / `OPAQUE`) is
uniform at all four triangle-geometry sites; `max_vertex` uses `vertex_count.saturating_sub(1)`
uniformly; the skinned path correctly strides by `SKIN_OUTPUT_STRIDE_BYTES` (position-only, `#2170`)
rather than `size_of::<Vertex>()` at both of its sites — the one place a copy-paste would silently
read garbage. Build-flag constants have not drifted, `built_flags` records BUILD-time flags and
`refit_skinned_blas` validates them *before* taking the mutable borrow (VUID-…-pInfos-03667). The
TLAS BUILD-vs-UPDATE decision is guarded on `built_primitive_count` for both grow and shrink, and
an empty TLAS is legal from frame 0. Every buffer whose device address is queried carries
`SHADER_DEVICE_ADDRESS`, and scratch alignment is enforced at allocation *and* at use at all five
scratch sites. Deferred BLAS destruction (`pending_destroy_blas` with a countdown) holds at every
eviction/drop site — there is no immediate `destroy_acceleration_structure` on a live path.

**The `instance_custom_index` ↔ SSBO contract holds end to end**: `build_instance_map` is computed
once with a predicate, and the SSBO builder loop applies the *identical* predicate in the *identical*
enumeration order, with no `mesh_registry` mutation in between. The 24-bit ceiling is pinned by a
compile-time assert plus a `debug_assert!` at the truncation site.

**The hole (`AS-D1-NEW-01`, HIGH)**: `shrink_blas_scratch_to_fit` derives its shrink target by
walking `self.blas_entries` only, while the buffer it shrinks is shared with `self.skinned_blas`.
`refit_skinned_blas` performs no size validation and nothing on the skinned path re-grows the buffer
for an entity that already has a BLAS. Two live call sites can fire with skinned BLAS resident:
`recreate_swapchain_core` (window resize with NPCs on screen — no cell transition, so *every* skinned
BLAS survives) and `finish_unload_batch`. The consequence is a build-scratch overrun: corrupted
neighbouring allocation or `VK_ERROR_DEVICE_LOST`.

**The latent gap (`AS-D1-NEW-02`, LOW)**: all three BLAS registration sites assign unconditionally,
so a re-registration would leak the previous `vk::AccelerationStructureKHR` (which has no `Drop`) and
permanently inflate the eviction budget. No live path reaches it today — caller discipline holds, but
non-uniformly (`build_global_blas_for_draws` guards; `build_blas_batched` does not).

### SSBO indexing & ray-query safety (Dimension 2)

**Clean**: every one of the six ray-hit sites uses `rayQueryGetIntersectionInstanceCustomIndexEXT` —
no `gl_InstanceID` anywhere. Raster-path `fragInstanceIndex` equals the SSBO row because `draw.rs`
emits `first_instance: instance_idx`, and the TLAS side packs the same compaction map, so
`traceReflection`'s self-skip and the refraction loop's self-terminus test are valid. `materials[]`
is uniformly indexed by `inst.materialId`, never by instance index. All vertex/index SSBO offsets go
through build.rs-generated stride constants — one source of truth, no hand-rolled strides — and
skinned instances correctly bypass `+ vOff` and the model matrix. Every `rayQueryInitializeEXT` is
reachable only under `sceneFlags.x > 0.5`. Noise is deterministic per-pixel-per-frame (PCG2D +
interleaved-gradient), so TAA/SVGF can converge. Both ReSTIR-DI regression guards hold, as does the
BC1 punch-through alpha guard.

**Three findings, all shading-quality rather than safety**: the GI hemisphere axis is built from the
*unflipped* `fragNormalEffective` while its ray origin is biased along the *viewer-flipped* `N_bias`
(MEDIUM — every cosine-weighted direction points through the surface for back-facing two-sided
draws); the glass refraction passthru loop drops `rayTMin` from `0.05` to `0.0` after iteration 1
with no stated rationale (MEDIUM); and that same loop never decrements its `tMax`, so effective reach
is 3× the documented 2000 units (LOW).

### Denoiser stability (Dimension 8)

The SVGF temporal → à-trous → composite chain **verifies clean against every checklist invariant**:
history ping-pong is correct and self-alias-proof; the motion-vector contract matches its producer
exactly (jitter is applied to `gl_Position` only, `fragCurrClipPos` stays un-jittered); mesh-ID
disocclusion masks bit 31 on both the bilinear and nearest paths and applies the 0.9 normal cone;
first-frame safety is per-FIF and only advances after submit success; the firefly clamp is correctly
hoisted *ahead* of `if (hasHistory)` so the disocclusion path is clamped too; à-trous parity gives
final slot 0 with no read-write alias. ACES correctly lives in `presentation.frag`, not composite, so
bloom is added upstream of the tone-map. The caustic double-count guard holds. All five shaders in
this dimension were recompiled and byte-compared against the committed `.spv` — identical.

**One substantive defect**: composite's `is_sky` branch *replaces* the pixel with
`compute_sky(dir)` and never reads `direct4.rgb`, while every blend-pipeline draw runs with
`depth_write_enable(false)`. Any translucent draw silhouetted purely against open sky is therefore
composed into the HDR attachment and then thrown away (MEDIUM). Notably the FSR reactive/transparency
masks are *not* lost, so FSR is told a transparent surface is present whose colour has been erased.
The two remaining D8 findings are doc-rot on dead `CompositeParams` fields.

---

## GPU-Struct & Memory Assessment

*(Summarizing Dimensions 3 and 5: layout pins, leaks, lifecycle/teardown.)*

### Layout pinning (Dimension 3)

The pinned contracts were verified three ways — Rust source, executed test asserts
(`cargo test -p byroredux-renderer --lib` → 525 passed, 0 failed), **and disassembly of the shipped
`.spv` artifacts** (`spirv-dis`, extracting `OpMemberDecorate … Offset` / `ArrayStride`), i.e.
validating the binaries the engine actually loads rather than the GLSL text.
`scripts/check-shader-artifacts.sh` reports 21/21 match against pinned glslang 11:16.2.0.

**Holding**: `GpuInstance` 128 B with byte-identical offsets across all five declaration sites
(including `surfaceId` @108, `skinnedVertexAddress` @112, `_reserved` @120); `GpuCamera` 336 B across
all six re-declarers; `GpuMaterial` 348 B with all 87 field names and offsets identical in both
`triangle.frag.spv` and `water.frag.spv`; `GpuLight` 64 B across four GLSL copies. Every
`GpuMaterial` field is a 4-byte scalar with zero implicit padding, which is what makes the byte-`Hash`
/ `Eq` path deterministic. Flag constants are single-sourced through `shader_constants_data.rs` — a
grep for hand-written `#define MAT_FLAG_|MATERIAL_KIND_|INSTANCE_FLAG_|DBG_` outside the generated
file returns nothing. The over-cap material intern path returns slot 0 with a one-shot warn, and
`upload_materials` carries a release-visible `assert!` plus a clamp, so the cap is airtight against a
GPU OOB read.

**The gap**: two hand-mirrored GPU structs have **no pin at all**. `GpuTerrainTile` (set 1 / binding
10 SSBO) has no `size_of` pin, no `offset_of!` pin, no GLSL↔Rust lockstep test and no `.spv`
reflection pin — the only thing coupling the two declarations is a comment, and the buffer is sized
from the Rust side while the shader indexes with the GLSL stride (MEDIUM). `DalcCubeUBO` is the same
shape, and worse: the `#1447` reflection tooling that exists precisely to catch this is already wired
for `CameraUBO` and the volumetrics UBOs, but `DalcCubeUBO` was never added to either list (MEDIUM).
Both are cheap to close. The third D3 finding is in-code comment rot quoting superseded 112 B / 300 B
sizes (LOW, merged with the D7 report of the same `MAX_MATERIALS` site).

### Memory, leaks and lifecycle (Dimension 5)

**No CRITICAL / HIGH / MEDIUM findings.** The severity floor for this dimension is "any per-frame
leak = HIGH", and none was found.

**Allocator lifecycle**: `AllocatorResource` removal precedes `VulkanContext` drop on *both* paths —
structurally in `App::drop` (so it holds on panic-unwind) and again in the `CloseRequested` arm, and
it is idempotent. The allocator is dropped before the device via `Arc::try_unwrap` → `into_inner`,
and the `Err(arc)` arm deliberately `return`s (leaking device+surface+instance) rather than falling
through to `destroy_device` — correct, since the alternative is a driver-side `vkFreeMemory` against a
dead device. Every `Arc<Mutex<Allocator>>` clone holder releases before that `try_unwrap`.

**Scratch and deferred destroy**: `shrink_blas_scratch_to_fit` routes the retired shared scratch
through `pending_destroy_scratch` on *both* exits; the TLAS resize does `device_wait_idle()` before
freeing; BLAS byte accounting is symmetric (`+=` at three sites, `saturating_sub` at three sites, no
drift path); the compaction query pool is destroyed on all three exits including the `?`-heavy phases.
A load-bearing coupling was identified and recorded: the end-of-frame TLAS shrink reads
`slot_to_shrink = self.current_frame` *after* the increment, naming the slot the *previous* frame
submitted on — it is safe **only** because `draw_frame` waits on **both** in-flight fences (`#282`).

**Teardown completeness**: every `VulkanContext` field was cross-checked against the `Drop` body; no
resource-owning field is missed, and ordering is correct (framebuffers before render pass, image views
before swapchain, meshes after pipelines, device last).

**The two LOWs** are host-RAM and latent-handle hygiene, not leaks: half the per-frame scratch cluster
(`previous_models_scratch` plus the two rigid-history `FxHashMap`s) is excluded from the peak-shrink
policy the other two members get, so a large-exterior peak pins ~16 MB + ~20 MB/map for the rest of
the session; and `GpuBuffer::destroy` leaves a dangling `self.buffer` handle in a `pub` field
(double-free is correctly prevented, but a *read* is not defended).

---

## Findings

Findings are grouped by severity and reproduced in their **original full format**, verbatim from the
source dimension files. Ordering within a severity band is by dimension number.

### CRITICAL

None.

---

### HIGH

#### AS-D1-NEW-01: `shrink_blas_scratch_to_fit` computes its peak from static BLAS only, ignoring live skinned BLAS that share the same scratch buffer
- **Severity**: HIGH
- **Dimension**: AS Correctness (Dim 1)
- **Location**: `crates/renderer/src/vulkan/acceleration/memory.rs:AccelerationManager::shrink_blas_scratch_to_fit` (consumers: `blas_skinned.rs:AccelerationManager::refit_skinned_blas`)
- **Status**: NEW (adjacent to `#1127` / PERF-DIM7-04, which was closed 2026-05-24 as "stale-premise" on the *memory* angle; the closeout's stated premise is factually wrong — see Evidence — and the *correctness* angle was never examined)
- **Description**: `blas_scratch_buffer` is a single allocation shared by **both** the static BLAS builders (`build_blas`, `build_blas_batched`) **and** the skinned BLAS builder/refitter (`build_skinned_blas_batched_on_cmd`, `refit_skinned_blas`). `shrink_blas_scratch_to_fit` derives its shrink target `peak` by walking `self.blas_entries` **only**:

  ```rust
  let peak: vk::DeviceSize = self
      .blas_entries          // static (mesh-keyed) BLAS ONLY
      .iter()
      .flatten()
      .map(|e| e.build_scratch_size)
      .max()
      .unwrap_or(0);
  ```

  `self.skinned_blas` is never consulted, even though every `BlasEntry` in it *does* carry a populated `build_scratch_size` (set in `build_skinned_blas_batched_on_cmd` Phase 4). If a live skinned entity's scratch requirement exceeds the surviving static peak, the shrink reallocates the shared buffer *below* what that entity's next `refit_skinned_blas` needs.

  `refit_skinned_blas` performs **no size validation** — it takes `self.blas_scratch_buffer.as_ref()`, reads its device address, and submits `mode = UPDATE` with that address. There is no `get_acceleration_structure_build_sizes` re-query and no comparison against `entry.build_scratch_size`. Nothing re-grows the buffer either: `build_skinned_blas_batched_on_cmd` (the only grow site on the skinned path) early-returns on `entities.is_empty()`, and an entity that already has a BLAS is never in that batch.
- **Evidence**: The two call sites that can fire while skinned BLAS are live:
  - `crates/renderer/src/vulkan/context/resize.rs` (`recreate_swapchain_core`) — a window resize with NPCs on screen. No cell transition, so **every** skinned BLAS survives.
  - `byroredux/src/cell_loader/unload.rs::finish_unload_batch` — the #1127 closeout claims "the static-survivors peak walk is a correct lower-bound **after the unload drops all skinned entries**". `unload_cell` does not drop skinned entries: `grep -rn drop_skinned_blas` shows the only callers are `context/skinned_blas_refit.rs` (LRU sweep + count/flag-mismatch paths) and `context/mod.rs` (shutdown). The unload path merely queues `pending_skin_unload_victims`, which `record_skinned_blas_refit` drains on a **later** frame (`skinned_blas_refit.rs`, `#1003` block). So at the moment `shrink_blas_scratch_to_fit` runs, the outgoing cell's skinned BLAS are still resident *and* still counted by nothing.

  Reachable shape: exterior cell grows the shared scratch to e.g. 40 MB on a large terrain/LOD BLAS → unload → interior cell whose static survivors peak at ~1 MB → `scratch_should_shrink(40 MB, 1 MB)` passes both the `2×` and the 16 MB-slack gate → buffer reallocated at 1 MB → a surviving NPC's `refit_skinned_blas` submits an UPDATE whose scratch range runs past the 1 MB allocation.

  The `peak == 0` arm is the degenerate version: with no static survivors the scratch is dropped entirely, and `refit_skinned_blas` then fails its `.context("blas_scratch_buffer absent — must be allocated by build_skinned_blas_batched_on_cmd first")?` on every skinned entity until one of them first-sights again. That arm at least fails loudly; the shrink-to-static-peak arm fails silently.
- **Impact**: AS build scratch overrun — the GPU writes build scratch past the end of the allocation. Consequences range from a corrupted neighbouring allocation to `VK_ERROR_DEVICE_LOST`. Blast radius is every skinned actor's RT presence (shadows/reflections/GI) plus whatever allocation follows the scratch buffer in the `gpu-allocator` slab. Invisible to `cargo test` (no live device); **needs RenderDoc / `BYRO_VALIDATION=1` verification** to confirm the driver actually faults rather than silently over-reserving — synchronization/BDA validation may or may not flag the scratch range depending on layer version.
- **Related**: `#1127` / PERF-DIM7-04 / REN-D2-NEW-01 (closed stale-premise, wrong premise); `AUDIT_PERFORMANCE_2026-05-19.md:88` (flagged the same peak-walk gap, framed as under-shrink/memory only); `#1782` (deferred scratch destroy — orthogonal, the *when* not the *how big*).
- **Suggested Fix**: Make the peak walk cover both maps: `chain` `self.skinned_blas.values()` into the `blas_entries` iterator when taking the `max()` of `build_scratch_size`, and apply the same union to the `peak == 0` early-drop arm. Pure CPU bookkeeping over an already-recorded field, unit-testable, no barrier/stage change. Optionally add a `debug_assert!(scratch_buffer.size >= entry.build_scratch_size)` in `refit_skinned_blas` so a future regression trips in debug instead of on the GPU.

---

### MEDIUM

#### REN-D2-2026-08-07-01: GI hemisphere axis is not viewer-flipped for two-sided back faces, while its ray origin is

- **Severity**: MEDIUM
- **Dimension**: Ray Queries (Dim 2)
- **Location**: `crates/renderer/shaders/triangle.frag` — one-bounce GI block (`N_geom` / `giDir` / `giOrigin`, guarded by `rtLOD < RT_LOD_GI`); interacts with the `gl_FrontFacing` flip applied to `N` near `terrainGeometryNormal` and with `N_bias`
- **Status**: NEW
- **Description**: The two-sided back-face flip is applied only to the shading normal `N`:

  ```glsl
  vec3 N = normalize(fragNormalEffective);
  if (!gl_FrontFacing) {
      N = -N;          // flips N only
  }
  vec3 terrainGeometryNormal = N;
  ...
  vec3 N_bias = dot(N, V) < 0.0 ? -N : N;   // always viewer-facing
  ```

  `fragNormalEffective` itself is never re-oriented. The GI path then builds its
  hemisphere around the *unflipped* value while biasing the origin along the
  *flipped* one:

  ```glsl
  vec3 N_geom  = normalize(fragNormalEffective);       // NOT viewer-flipped
  vec3 giDir   = cosineWeightedHemisphere(N_geom, n1, n2);
  vec3 giOrigin = fragWorldPos + N_bias * 0.1;         // viewer-flipped
  ```

  For a fragment of a two-sided draw (`mesh.two_sided` → cull-off pipeline:
  foliage/vine/grass cards, curtains, some architecture) rendered from its back
  side, `gl_FrontFacing == false`, so `N_bias` points toward the viewer while
  `N_geom` points away. Every cosine-weighted `giDir` therefore has a positive
  component *through* the surface plane, starting from an origin offset 0.1 units
  on the opposite side.

  The comment block immediately above `N_geom` argues (correctly) for using the
  geometric rather than the normal-mapped normal; the omission is the
  viewer-orientation half, which every *other* `fragNormalEffective` consumer in
  this shader does apply locally — the fire-refraction branch (`macroN`, flipped
  with `if (dot(macroN, V) < 0.0) macroN = -macroN;`) and the glass branch
  (`glassViewNormal`, same flip). The GI site is the one consumer that does not.
- **Evidence**: With tMin `0.05` and an origin `0.1` off the plane, the plane
  crossing occurs at `t ≈ 0.1 / dot(giDir, planeNormal)` — for cosine-weighted
  sampling typically `t ≈ 0.15`, i.e. comfortably inside `[tMin, 6000]`. The
  fragment's own triangle is in the TLAS (two-sided draws are not excluded from
  BLAS/TLAS), so it is a committable candidate:

  ```glsl
  rayQueryInitializeEXT(giRQ, topLevelAS, gl_RayFlagsOpaqueEXT, 0xFF,
                        pathOrigin, 0.05, pathDir, 6000.0);
  ```

  On commit the very first path segment sets
  `rtAO = mix(0.3, 1.0, smoothstep(60.0, 500.0, pathDistance))` with
  `pathDistance ≈ 0.15` → `rtAO` pinned at its 0.3 floor, and `pathRadiance`
  accumulates the back side of the same card instead of the room.
  Alpha-tested foliage often escapes via `rayHitHasCoverage` returning false
  (the `continue` arm), but opaque two-sided geometry commits.
- **Impact**: Back-facing fragments of two-sided draws get indirect light
  gathered from the wrong hemisphere and an AO term clamped near the 0.3 floor.
  Symptom class: darkened/AO-crushed back faces on vines, grass cards,
  curtains and two-sided architecture, and a front/back GI discontinuity on the
  same card. Blast radius is limited to two-sided draws, which is why this has
  survived: single-sided geometry is back-face culled so `gl_FrontFacing` is
  always true and the code path is a no-op there (as the flip's own comment
  notes). Magnitude on real cells **needs RenderDoc / visual verification** —
  the logic inconsistency is definite, the pixel-level severity is not measured.
- **Related**: Same normal-orientation family as #668 (RT-3, V-aligned flip on
  metal reflection), #733 (RT-11, hoisting `N_bias`), #821 / REN-D9-NEW-02
  (documented intentional asymmetry for the window-portal ray — that one *is*
  deliberate; this one is not documented as such). Prior audit
  `AUDIT_RENDERER_2026-05-19.md` verified `fragNormalEffective` flows through
  its six consumers for the `INSTANCE_FLAG_FLAT_SHADING` case but did not cover
  the two-sided orientation question.
- **Suggested Fix**: Orient the GI hemisphere axis toward the viewer at the GI
  site, mirroring the fire-refraction and glass branches:
  `vec3 N_geom = normalize(fragNormalEffective); if (dot(N_geom, V) < 0.0) N_geom = -N_geom;`
  (or hoist a single `fragNormalEffectiveView` next to `N_bias` and switch all
  four consumers to it). Leave the ReSTIR `geomN` / `rc.pad0` pair alone unless
  both sides change together — they are currently self-consistent.

#### REN-D2-2026-08-07-02: Glass refraction passthru loop drops `rayTMin` to 0.0 after the first iteration

- **Severity**: MEDIUM
- **Dimension**: Ray Queries (Dim 2)
- **Location**: `crates/renderer/shaders/triangle.frag` — IOR refraction block, `REFRACT_PASSTHRU_BUDGET` loop (`rayTMin` initialisation and its reassignment in the passthru-continue arm)
- **Status**: NEW
- **Description**: The loop initialises `float rayTMin = 0.05;` — matching the
  convention `raytrace.glsl` documents at length ("Same 0.05 tMin convention
  every other ray-query site in this shader uses (grep `rayQueryInitializeEXT`):
  window portal, refraction loop, cluster shadow, GI bounce"). But the
  passthru-continue arm silently resets it:

  ```glsl
  rayOrigin = exitPoint + refractDir * 0.05;
  rayTMin = 0.0;                       // <- no comment, no rationale
  accumulatedDist += hDist;
  continue;
  ```

  From iteration 2 onward the query runs with `tMin = 0.0` and only a 0.05
  origin nudge along the *newly refracted* direction. When the refracted
  direction is near-tangent to the interface just crossed (grazing exit, which
  is exactly the common case near total internal reflection), the 0.05 nudge
  projects to well under 0.05 of perpendicular clearance, and a `tMin` of 0.0
  makes the just-crossed triangle a committable candidate at `t ≈ 0`.
- **Evidence**: `raytrace.glsl`'s `traceReflection` documents the failure mode
  this convention exists to prevent — pre-#1017 it used tMin 0.01 against a 0.05
  bias, "which let perturbed-normal flips at grazing angles fire the ray back
  through the surface and self-hit, producing black speckle on metals." The
  refraction loop is cited in that same comment as one of the sites honouring
  0.05, but it only honours it on the first of up to three iterations. Note the
  loop's own downstream guards (`terminusOnSelf`, `terminusOnGlass`,
  `terminusOnFallback`) catch a self-*terminus*, but a mid-loop self-hit is
  consumed as a passthru and burns budget instead.
- **Impact**: Wasted passthru budget and, at grazing exits, a refraction ray
  that re-enters the surface it just left — the loop then terminates one
  interface early and the fragment falls to the ambient escape path.
  Symptom class: intermittent flat/ambient patches on curved glass at grazing
  angles. Bounded (max 3 iterations, always converges). Confined to
  `glassIORAllowed` fragments. **Needs RenderDoc verification** to confirm the
  self-hit actually commits on real content rather than being absorbed by the
  0.05 nudge.
- **Related**: #1017 (tMin normalisation on `traceReflection`), #789 (the
  passthru loop's origin), #820 (Frisvad basis at the same site — verified
  intact, see below).
- **Suggested Fix**: Keep `rayTMin = 0.05` for every iteration, or — if the 0.0
  is deliberate to avoid skipping genuinely thin stacked panes — document that
  rationale inline so the next audit does not re-flag it.

#### REN-D3-2026-08-07-01: `GpuTerrainTile` is a hand-mirrored GPU struct with no size, offset, or lockstep pin
- **Severity**: MEDIUM
- **Dimension**: GPU-Struct Layout (Dim 3)
- **Location**: `/mnt/data/src/gamebyro-redux/crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:9` (`GpuTerrainTile`) ↔ `/mnt/data/src/gamebyro-redux/crates/renderer/shaders/include/bindings.glsl:322` (`struct GpuTerrainTile`)
- **Status**: NEW
- **Description**: `GpuTerrainTile` is a `#[repr(C)]` struct uploaded to the
  set 1 / binding 10 SSBO and hand-mirrored in GLSL, exactly like `GpuInstance`,
  `GpuMaterial`, `GpuLight` and (since REN-D3-01 was fixed) `GpuFogVolume`. Unlike
  all of those it has **no** `size_of` pin, **no** `offset_of!` pin, **no**
  GLSL↔Rust lockstep test, and **no** `.spv` reflection pin. `grep -rn
  "GpuTerrainTile" crates/renderer/src` returns only use sites — buffer sizing,
  upload, and a debug scratch row — never an assertion. The only thing coupling
  the two declarations today is a comment.
- **Evidence**: Rust `[u32;8] × 3` = 96 B; shipped `triangle.frag.spv` carries
  `OpDecorate %_runtimearr_GpuTerrainTile_0 ArrayStride 96` with members at
  0 / 32 / 64 — currently correct. The buffer is sized from the **Rust** side
  (`buffers.rs:456  size_of::<GpuTerrainTile>() * MAX_TERRAIN_TILES`) and the
  upload memcpy uses the Rust stride (`upload.rs:786`), while the shader indexes
  with the **GLSL** stride. Adding a 4th layer role (e.g. `layer_glow_index:
  [u32;8]`, a natural next step for LAND splatting) on the Rust side alone makes
  the two strides 128 vs 96 and every tile from index 1 onward reads misaligned
  bindless texture indices.
- **Impact**: Silent per-tile corruption of terrain splat texture indices across
  every exterior cell (wrong/garbage diffuse-normal-specular layers, or index-0
  placeholder). Fails no test, fails no validation layer — the SSBO byte count is
  legal either way. Blast radius = all outdoor rendering.
- **Related**: Same defect class as REN-D3-01 (`GpuFogVolume`, audit 2026-08-02,
  since fixed) and #1657 / SF-D8-01 (`GpuMaterial` order guard).
- **Suggested Fix**: Add `size_of::<GpuTerrainTile>() == 96` +
  `offset_of!` pins in `gpu_instance_layout_tests.rs`, and extend the existing
  `strip_struct_body`/`extract_struct_body` helpers already in that file to
  cross-check the GLSL `struct GpuTerrainTile` field list against the Rust one.

#### REN-D3-2026-08-07-02: `DalcCubeUBO` block size is unpinned despite the #1447 reflection tooling existing
- **Severity**: MEDIUM
- **Dimension**: GPU-Struct Layout (Dim 3)
- **Location**: `/mnt/data/src/gamebyro-redux/crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:410` (`GpuDalcCube`) ↔ `/mnt/data/src/gamebyro-redux/crates/renderer/shaders/include/bindings.glsl:344` (`uniform DalcCubeUBO`)
- **Status**: NEW
- **Description**: `GpuDalcCube` (8 × `[f32;4]` = 128 B) is uploaded to set 1 /
  binding 14 as a UBO whose GLSL mirror is a hand-written inline block. #1447
  established `reflect::uniform_block_size_by_name` precisely to catch a
  `#[repr(C)]` UBO struct growing on the Rust side without the GLSL block, and
  `reflect.rs` uses it for `CameraUBO`
  (`camera_ubo_size_matches_gpu_camera_in_every_shader`) and for the volumetrics
  UBOs (`volumetrics_ubo_sizes_match_host_structs_in_every_shader`).
  `DalcCubeUBO` was never added to either list, and there is no Rust-side
  `size_of::<GpuDalcCube>()` pin either.
- **Evidence**: `spirv-dis triangle.frag.spv` → `DalcCubeUBO` members at
  0/16/32/48/64/80/96/112 (128 B total) — currently correct.
  `grep -rn "GpuDalcCube" crates/renderer/src` yields only `buffers.rs:440`
  (`size_of::<GpuDalcCube>()` for allocation), `upload.rs:183`, and a
  `Default` construction. `grep "DalcCube" crates/renderer/src/vulkan/reflect.rs`
  → nothing.
- **Impact**: A Rust-side append (the doc comment already earmarks
  `specular_fresnel` as "reserved for future per-cell specular tint plumbing",
  i.e. a field that is *expected* to grow a consumer) that misses the GLSL block
  silently shifts every ambient-cube axis the fragment shader reads, mis-tinting
  interior ambient on all Skyrim WTHR.DALC cells. Cheaper to catch than
  REN-D3-01 was: the tooling already exists.
- **Related**: #1447 (`CameraUBO` size hazard), REN-D3-01 (`GpuFogVolume`).
- **Suggested Fix**: Add `"DalcCubeUBO" → size_of::<GpuDalcCube>()` to the
  existing `.spv`-reflection size table in `reflect.rs::tests` alongside the
  `CameraUBO` / volumetrics entries — a few lines, no new machinery.

#### REN-D4-2026-08-07-01: Swapchain image's `UNDEFINED →COLOR_ATTACHMENT_OPTIMAL` layout transition is not provably ordered after the acquire semaphore
- **Severity**: MEDIUM
- **Dimension**: Sync/Barriers (Dim 4)
- **Location**: `crates/renderer/src/vulkan/presentation.rs:PresentationPipeline::recreate`/`create_render_pass` (the `incoming` `vk::SubpassDependency`), against `crates/renderer/src/vulkan/context/draw.rs:draw_frame` (`let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];`)
- **Status**: NEW (adjacent to the invariant note in `docs/audits/AUDIT_RENDERER_2026-04-25.md`, which recorded `(initial_layout=UNDEFINED, image_available wait at COLOR_ATTACHMENT_OUTPUT)` as "the load-bearing invariant" back when *composite* was the swapchain writer; the writer has since moved to `presentation.rs` with a wider dependency and the invariant was not re-checked)
- **Description**: The submit waits on `image_available[frame]` with `wait_dst_stage_mask = COLOR_ATTACHMENT_OUTPUT` only. The presentation render pass is the pass that writes the acquired swapchain image; its color attachment is declared `initial_layout(UNDEFINED)` → `final_layout(PRESENT_SRC_KHR)`, so the pass performs an `UNDEFINED → COLOR_ATTACHMENT_OPTIMAL` layout transition. Per spec, a render pass's automatic layout transition is ordered between the first and second synchronization scopes of the relevant `SUBPASS_EXTERNAL` dependency. That dependency's `dst_stage_mask` is `FRAGMENT_SHADER | COLOR_ATTACHMENT_OUTPUT` — and `FRAGMENT_SHADER` is *logically earlier* than `COLOR_ATTACHMENT_OUTPUT`, so it is **not** gated by a semaphore wait scoped to `COLOR_ATTACHMENT_OUTPUT`. The transition (a write, for hazard purposes) therefore has a window in which it can execute before the presentation engine has released the image.
- **Evidence**:
  - `draw.rs`: `let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];`
  - `presentation.rs`, attachment: `.load_op(vk::AttachmentLoadOp::DONT_CARE).initial_layout(vk::ImageLayout::UNDEFINED).final_layout(vk::ImageLayout::PRESENT_SRC_KHR)`
  - `presentation.rs`, `incoming`: `.dst_stage_mask(vk::PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)` — the `FRAGMENT_SHADER` limb exists to cover the *upscaled-image sampler read* (a different resource), but subpass-dependency scopes are pass-wide, so the swapchain attachment's transition inherits the looser ordering.
- **Impact**: Theoretical corruption / tearing of the presented image, and a potential WSI VUID hit under sync-validation. In practice `UNDEFINED` discards contents so a premature transition is mostly benign on current drivers, which is exactly why this is invisible to `cargo test` and to normal play. Blast radius is every frame, on every platform, if a driver ever schedules the transition early.
- **Related**: `docs/audits/AUDIT_RENDERER_2026-04-25.md` (the original invariant note); `docs/audits/AUDIT_RENDERER_2026-04-22.md` (same class of "pass-wide dependency masks a per-resource need" observation on composite); #2143 (which repaired the *outgoing* half of this same dependency pair).
- **Suggested Fix**: **needs RenderDoc / sync-validation verification.** Run with `BYRO_VALIDATION=1` plus synchronization validation enabled and confirm whether the layer reports an acquire-ordering hazard on the swapchain image before changing anything. Do not blind-fix.

#### NIFAL-D6-2026-08-07-01: `grayscale_to_palette_scale` is dropped at the canonical boundary — the palette remap it modulates is applied at full strength regardless of authoring
- **Severity**: MEDIUM
- **Dimension**: NIFAL Material (Dim 6)
- **Location**: `/mnt/data/src/gamebyro-redux/byroredux/src/material_translate.rs:translate_material` (missing copy); source at `/mnt/data/src/gamebyro-redux/crates/nif/src/import/types.rs:ImportedMaterial::grayscale_to_palette_scale`; consumer gap at `/mnt/data/src/gamebyro-redux/crates/renderer/shaders/triangle.frag:984`
- **Status**: NEW (acknowledged in two source comments, but no open tracker — keyword scan of the 73 open issues in `/tmp/audit/renderer/issues.json` returns nothing; structurally identical to #2284 / MAT-D1-NEW-04, which *was* tracked and fixed 2026-08-05)
- **Description**: `grayscale_to_palette_scale` is parsed from **both** authoring sources — the inline `BSEffectShaderProperty`/`BSLightingShaderProperty` tail (`crates/nif/src/blocks/shader.rs:724`, pinned on FO76 content by `blocks/shader_tests/fo76.rs:25` at `0.4`) and BGSM (`crates/bgsm/src/bgsm.rs:135,308`, with template inheritance) — and merged onto `ImportedMaterial` at `byroredux/src/asset_provider/material.rs:1066`. It then **dead-ends there**: there is no `Material` field, so `translate_material` has nothing to copy into, no `GpuMaterial` field, and no shader consumer. This is the exact failure shape #2284 documented ("captured at the NIF importer boundary … but dead-ended there with zero consumers: no field existed here, so `translate_material` had nothing to copy into").

  What makes this worse than a plain unplumbed-scalar gap: the *enable* half of the same feature **is** fully plumbed. `pack_imported_material_flags` (`cell_loader.rs:231-270`) sets `EFFECT_PALETTE_COLOR`/`EFFECT_PALETTE_ALPHA` from `bgsm_greyscale_lut_enabled`, and `triangle.frag` acts on it. So the remap fires; only its authored strength is discarded.
- **Evidence**: The shader performs an unmodulated LUT lookup and says so:

  ```glsl
  // The grayscale_to_palette_scale modulator is not yet
  // plumbed to GpuMaterial — direct lookup for now.
  if ((mat.materialFlags & MAT_FLAG_EFFECT_PALETTE_COLOR) != 0u
      && mat.greyscaleLutIndex != 0u) {
      float gsIndex = dot(texColor.rgb, vec3(0.2126, 0.7152, 0.0722));
      texColor.rgb = texture(
          textures[nonuniformEXT(mat.greyscaleLutIndex)],
          vec2(gsIndex, 0.5)).rgb;
  }
  ```
  and `crates/core/src/ecs/components/material.rs:258` cites it as the
  *precedent* for landing captured-but-unshaded scalars — while the field it
  is a precedent for now exists on `Material` and this one still does not.
  `grep -rn grayscale_to_palette_scale` over the tree shows the last
  assignment is `asset_provider/material.rs:1066` (raw tier); nothing reads it.
- **Impact**: FO4 / FO76 / Starfield content that authors a non-default
  palette scale (the FO76 fixture authors `0.4`) renders its greyscale-to-palette
  remap at an effective `1.0`. Content-visible on palette-remapped lit BGSM
  surfaces and FX atlases; silent (no warning, no `mat.*` visibility, since the
  value never reaches an inspectable ECS component). Blast radius is bounded to
  the palette-remap population, but it is a genuine canonical-boundary drop —
  the class of bug NIFAL exists to make impossible.
- **Related**: #2284 / MAT-D1-NEW-04 (same shape, fixed); #1353 / FO4-D8-07 (the
  remap itself); #1580, #2108 / SF-D9-01 (the enable-bit gating);
  `docs/engine/nifal.md` §2 "Materials — converged".
- **Suggested Fix**: Add `grayscale_to_palette_scale: f32` (default `1.0`) to
  `Material`, copy it in `translate_material`, and extend
  `translate_material_copies_every_canonical_field` with a distinctive value —
  mirroring the #2284 landing exactly (captured now, `GpuMaterial`/`triangle.frag`
  consumer as an independently-reviewable follow-up). Open a tracker for the
  GPU half so the shader TODO stops being the only record.

#### REN-D8-N01: Composite `is_sky` branch discards alpha-blended geometry drawn against the sky
- **Severity**: MEDIUM
- **Dimension**: Denoiser/Composite (Dim 8)
- **Location**: `crates/renderer/shaders/composite.frag:406` (`if (is_sky) { ... combined = compute_sky(dir); }` in `main`), against `crates/renderer/src/vulkan/pipeline.rs:663` (`create_blend_pipeline` → `.depth_write_enable(false)`)
- **Status**: NEW
- **Description**: Composite classifies a pixel as sky purely from the depth
  attachment (`bool has_surface = depth < 1.0; bool is_sky = !has_surface &&
  (params.depth_params.x > 0.5);`). The sky branch then *replaces* the pixel:
  `combined = compute_sky(dir);` — `direct4.rgb` (the main pass's HDR colour
  attachment) is never read into `combined` on that path. But every draw that
  goes through `create_blend_pipeline` runs with `depth_write_enable(false)`,
  so an alpha-blended fragment with nothing opaque behind it leaves depth at
  the cleared `1.0`. Its HDR contribution was blended into attachment 0 in the
  main pass and is then thrown away in composite and overpainted with the
  procedural sky. The prior audits (`2026-08-02` / `2026-08-03`, REN-D8-02 /
  REN-D16-02) covered a *different* gap in this same branch — missing bloom
  and volumetric fog — and that one is now **fixed** (`combined` is built by
  either branch and both terms are applied after). The discarded-`direct`
  problem is a separate, still-live defect in the restructured code.
- **Evidence**:
  ```glsl
  vec3 combined;
  if (is_sky) {
      vec3 dir = screen_to_world_dir(fragUV);
      combined = compute_sky(dir);      // direct4.rgb dropped entirely
  } else {
      ...
      combined = direct + indirect * albedo + caustic;
  }
  ```
  `pipeline.rs::create_blend_pipeline`: `.depth_test_enable(true)
  .depth_write_enable(false)` — "Transparent surfaces never write depth".
  Main-pass clear (`draw.rs:1470`) leaves attachment 0 at `clear_color` and
  depth at `1.0`, so the only content at a `depth == 1.0` exterior pixel is
  exactly the transparent geometry that was blended over the clear.
- **Impact**: Exterior only (`depth_params.x > 0.5`). Any translucent draw
  silhouetted purely against the sky vanishes: smoke / steam / magic particle
  billboards, alpha-blended banners and glass panes seen against open sky,
  and any `AlphaBlend`-flagged mesh on a skyline. Geometry-backed transparents
  are unaffected (the opaque behind them wrote depth). Note the FSR masks are
  *not* lost — `outReactive` / `outTransparency` MAX-blend correctly — so FSR
  is told a transparent surface is there while its colour has already been
  erased, which is its own mild inconsistency.
- **Related**: REN-D8-02 / REN-D16-02 (`AUDIT_RENDERER_2026-08-02.md`,
  `2026-08-03.md`) — same branch, bloom/fog half, now fixed. `DEN-11` /
  `#676` (the `direct4.a` alpha-marker pass-through, which is already
  forwarded symmetrically from both branches).
- **Suggested Fix**: Composite the sky *behind* the main pass result rather
  than instead of it — e.g. `combined = compute_sky(dir) * (1.0 - coverage) +
  direct;` where `coverage` comes from a real accumulated-coverage lane. The
  cheapest correct-for-one-layer version uses the existing `direct4.a` (the
  HDR attachment's alpha blend factors are `src_alpha = ONE, dst_alpha =
  ZERO`, so it carries the last transparent fragment's alpha); a fully correct
  version wants an accumulated `ONE_MINUS_SRC_ALPHA` coverage lane on the HDR
  attachment's alpha. Worth confirming the intended layering with a RenderDoc
  capture of an exterior particle-over-sky frame before shipping.

#### REN-D9-NEW-01: Zero-weight fallback diverges between `skin_vertices.comp` (identity) and `triangle.vert` (`inst.model`)
- **Severity**: MEDIUM
- **Dimension**: Skinning (Dim 9)
- **Location**: `crates/renderer/shaders/skin_vertices.comp:131-134` vs `crates/renderer/shaders/triangle.vert:146-151`
- **Status**: NEW
- **Description**: Both shaders take a defensive branch when a vertex's four bone weights sum to ~0. The raster path substitutes the instance's world matrix (`xform = inst.model`, already render-origin-rebased on the CPU). The compute path substitutes `mat4(1.0)`. Because the skinned BLAS is instanced into the TLAS with an **IDENTITY** transform (`acceleration/tlas.rs:540-548` — "skinned draws get IDENTITY because their BLAS already holds absolute world-space vertices"), the identity fallback writes the raw bind-pose/NIF-local coordinate into what the TLAS reads as absolute world space. The in-shader comment claims the branch "mirrors triangle.vert:153" and that "the rigid `inst.model` path doesn't apply here" — the second half is an assertion, not a derivation, and it is wrong for a skinned actor standing anywhere other than the world origin.
- **Evidence**:
  ```glsl
  // skin_vertices.comp:131-134
  float wsum = boneW.x + boneW.y + boneW.z + boneW.w;
  mat4 xform;
  if (wsum < 0.001) {
      xform = mat4(1.0);        // → raw local position, written into an ABSOLUTE-space BLAS
  ```
  ```glsl
  // triangle.vert:146-151
  float wsum = inBoneWeights.x + ... ;
  if (wsum < 0.001) {
      xform = inst.model;       // → correct world placement for raster
  ```
  Reachability: the classic `densify_sparse_weights` importer path cannot produce a zero quad (`crates/nif/src/import/mesh/skin.rs:529`, `:594` — unweighted vertices fall back to bone 0 @ 1.0). The Skyrim SE / FO4 packed-half path (`crates/nif/src/import/mesh/skin.rs:112-125`) is documented as **pass-through, no renormalisation, no zero fallback**, so a decoded all-zero weight quad reaches the Vertex struct unmodified.
- **Impact**: A single zero-weight vertex inside a skinned mesh drags that entity's BLAS AABB from the actor's bounding box out to the world origin. In an exterior cell that is a 10^5-unit-wide box instanced into the TLAS with identity — every shadow / reflection / GI ray that enters that volume pays triangle-intersection cost on a degenerate sliver, and can register spurious hits (the "long thin ribbon" class of artifact already described for the unrelated IDENTITY-`bone_world` dropout at `byroredux/src/render/skinned.rs:31-38`). Raster shows nothing wrong, so the symptom presents as an unexplained RT perf cliff / shadow streak on specific SSE-family actors.
- **Related**: `byroredux/src/render/skinned.rs:31-38` (IDENTITY bone-world dropout — different cause, same visual signature); `#651` / SH-6 (the sibling bone-index clamp, which *was* correctly mirrored across the two shaders).
- **Suggested Fix**: Make the compute fallback match the raster one — bind the instance model matrix into the skin dispatch (push constant or an extra SSBO read) and use it for the `wsum < 0.001` branch; or, cheaper and equally correct, make the invariant real at import time by renormalising / zero-filling in the SSE packed-half path so `wsum == 0` is unreachable, and turn the shader branch into a `debug`-only assert. Do not "fix" it by removing the branch.

#### REN-D14-2026-08-07-01: Parked-camera caustic EMA has no dynamic-scene invalidation
- **Severity**: MEDIUM
- **Dimension**: Caustics (Dim 14)
- **Location**: `crates/renderer/src/vulkan/caustic.rs::CausticPipeline::dispatch` (the `camera_static` branch) / `crates/renderer/src/vulkan/context/draw.rs:1740` (`camera_static` derivation)
- **Status**: NEW
- **Description**: The temporal-EMA path that replaced the per-frame clear is gated on a
  single global boolean derived **only** from the jitter-free view-projection matrix.
  When that matrix is unchanged, the accumulator is not cleared; instead it is scaled by
  `decay = parked_frames/(parked_frames+1)` (capped at `CAUSTIC_DECAY_MAX = 0.995`) and
  this frame's splat contributes only `emaWeight = 1 - decay`. There is no per-pixel
  motion-vector, mesh-ID, normal-consistency or light-change invalidation anywhere in the
  path — unlike `svgf_temporal.comp` and `taa.comp`, both of which reject history per-pixel.
  A parked camera with a *moving scene* therefore keeps up to `1/(1-0.995) = 200` frames of
  stale pool: a swinging/carried lantern, a walking NPC with a torch, an occluder crossing
  between the light and the glass, physics clutter settling, or a glass door opening all
  change every landing point while the accumulator still holds the old pool at up to 99.5 %
  weight.
- **Evidence**:
  `draw.rs:1740`
  ```rust
  let camera_static = vp
      .iter()
      .zip(self.prev_view_proj.iter())
      .all(|(a, b)| (a - b).abs() < 1.0e-6);
  ```
  `caustic.rs::dispatch` — the only consumer:
  ```rust
  if camera_static { self.parked_frames = self.parked_frames.saturating_add(1); }
  else { self.parked_frames = 0; }
  ...
  let decay_factor = if camera_static { (n / (n + 1.0)).min(CAUSTIC_DECAY_MAX) } else { 0.0 };
  ```
  The clear (`cmd_clear_color_image`) is in the `else` (moving-camera) arm only.
  `caustic_splat.comp` consumes it as `float emaWeight = 1.0 - pc.decayFactor;` and the
  decay dispatch (`pc.decayOnly == 1u`) scales every texel unconditionally with no
  validity test.
- **Impact**: Visual only, but directly visible: a caustic ghost/trail that persists for
  ~3 s at 60 fps whenever the player stands still and something in the scene moves. Worst
  in exactly the content the feature targets — FNV/Skyrim interiors with chem glass and
  bottles lit by carried torches and patrolling NPCs. Blast radius is limited to the
  caustic term (composited additively over `direct`), so no correctness/stability risk.
- **Related**: #2239 (the other half of the EMA correctness work); the module doc's
  "On camera motion the host clears (decayOnly never set) so a stale, mis-registered pool
  can't smear" comment describes camera motion only. Same class as the SVGF/TAA
  disocclusion tests that already exist for the other temporal passes.
- **Suggested Fix**: Either (a) reset `parked_frames` when the scene changes as well as when
  the camera does — e.g. thread a "scene dirty" signal (moved light / moved caustic-source
  instance count-or-transform hash) into `dispatch` alongside `camera_static`, or (b) hard-cap
  `CAUSTIC_DECAY_MAX` far lower (e.g. 0.9, ≈10-frame memory) so a stale pool decays within a
  few frames while still killing the jitter stipple.

#### REN-D15-NEW-01: Water-side caustics are fully suppressed (and refract upward) whenever the water surface is viewed from below
- **Severity**: MEDIUM
- **Dimension**: Water (Dim 15)
- **Location**: `crates/renderer/shaders/water.frag:486-490` (`viewFromPositiveSide` flip) → `:701` / `:753` (the `#1256` caustic block)
- **Status**: Regression of #2223 (commit `3d967d95`, "water caustics refract through the wave-perturbed normal")
- **Description**: `main()` flips the *shading* normal to the viewer side before the caustic block runs:
  `bool viewFromPositiveSide = dot(Nsurface, V) >= 0.0; ... if (!viewFromPositiveSide) { Nperturbed = -Nperturbed; }`.
  The caustic splat is a **light-side** computation — the sun is always above the surface, independent of where the camera is — but #2223 switched it from the never-flipped `Nsurface` to the view-flipped `Nperturbed` without re-anchoring the flip. When the camera is underwater, every visible water fragment has `viewFromPositiveSide == false`, so `Nperturbed` points *down* into the water, and:
  1. `refract(-sunDir, Nperturbed, 1.0/1.33)` with a normal on the same side as the incident propagation direction returns a ray pointing **upward** (for a straight-overhead sun the exact result is `(0,+1,0)`), not down toward the floor;
  2. `float NdotSun = max(dot(Nperturbed, sunDir), 0.0)` evaluates to `0.0` (down-facing normal vs. to-sun direction), so `contrib == 0`, `fixed_val == 0`, and the `imageAtomicAdd` is skipped entirely.
  Net effect: **zero water caustics whenever the camera is submerged** — precisely the viewing condition where caustics are the signature effect. The origin bias and shadow ray on the same path correctly stayed on the unflipped `Nsurface` (`vWorldPos + Nsurface * 0.05` / `vWorldPos - Nsurface * 0.05`), which is what makes the `Nperturbed` half inconsistent rather than merely unconventional.
- **Evidence**:
  ```glsl
  // :486-490 — view-side flip (correct for reflect/refract of the VIEW ray)
  bool viewFromPositiveSide = dot(Nsurface, V) >= 0.0;
  vec3 N = viewFromPositiveSide ? Nsurface : -Nsurface;
  if (!viewFromPositiveSide) { Nperturbed = -Nperturbed; }
  ...
  // :677-682 — light-side shadow ray, correctly anchored to Nsurface
  traceShadowTransmittance(vWorldPos + Nsurface * 0.05, sunDir, ...)
  // :701 — but the light-side REFRACTION uses the view-flipped normal
  vec3 refractDir = refract(-sunDir, Nperturbed, 1.0 / 1.33);
  // :720 — origin still on the unflipped geometric normal
  vWorldPos - Nsurface * 0.05, 0.05, refractDir, 5000.0
  // :753 — light-side cosine also on the view-flipped normal → 0 underwater
  float NdotSun = max(dot(Nperturbed, sunDir), 0.0);
  ```
  Pre-#2223 the block used `Nsurface` throughout, which is never flipped — underwater caustics worked (structureless, but present). The regression is a side effect of an otherwise-correct fix.
- **Impact**: Underwater/submerged camera loses all water-side caustic contribution to `combined` (composite binding 8 stays all-zero for the water half). Visual-only, no crash, no wasted GPU work beyond the already-traced shadow ray. Blast radius: any cell with a swimmable water volume + exterior sun; also affects any camera that dips below a waterfall/river plane. Interacts with `submersion_system`'s `head_submerged` FX path, which is otherwise correct.
- **Related**: #2223 (`3d967d95`); prior `AUDIT_RENDERER_2026-08-02.md` REN-D15-01 (the fix this regressed out of); `AUDIT_RENDERER_2026-08-03.md` closeout table row for REN-D15-01.
- **Suggested Fix**: Compute a light-side normal for the caustic block that is independent of the view side — e.g. `vec3 NperturbedLight = viewFromPositiveSide ? Nperturbed : -Nperturbed;` (i.e. undo the flip) and use it for both `refract()` and `NdotSun`, keeping the `Nsurface`-based origin bias as-is. Add a shader-source assertion/comment tying the two together so the next `Nsurface`↔`Nperturbed` swap doesn't re-couple them.

#### REN-D15-NEW-02: `foamFlowStreaks` still hashes absolute world coordinates — the #1502/#1997 precision rebase covered only `sampleScrollingNormal`
- **Severity**: MEDIUM
- **Dimension**: Water (Dim 15)
- **Location**: `crates/renderer/shaders/water.frag:376-390` (`foamFlowStreaks`), called at `:606` / `:609` / `:616`; contrast with `sampleScrollingNormal` (`:179-227`) and the `uvOrigin` plumbing at `:434-449`
- **Status**: NEW (sibling gap left by the #1997 fix for #1502)
- **Description**: #1997 fixed the documented #1502 precision bound by threading a render-origin offset (`uvOrigin`) into `sampleScrollingNormal` and subtracting it before the `hash21` lattice: `vec2 uv = (uvBase - originOffset) * scale + scroll * time;`. The *other* absolute-world consumer of the same `valueNoise`/`hash21` lattice — `foamFlowStreaks` — was not rebased. It is called with the raw absolute `vWorldPos` and projects it onto the flow axis:
  ```glsl
  float u = dot(worldPos, flowDir) - speed * time;   // worldPos ABSOLUTE
  float v = dot(worldPos, perp);
  float streak = valueNoise(vec2(u * 0.04, v * 0.18));
  ```
  `hash21` does `p = fract(p * vec2(443.897, 441.423))`. At Tamriel-scale coordinates (~±233k units) `u * 0.04 ≈ 9.3e3`, so `p.x * 443.897 ≈ 4.1e6` — beyond fp32's 24-bit mantissa for a meaningful fractional part (resolution ≈ 0.25), and `fract()` collapses to a handful of discrete values. At FNV Mojave far cells (`grid * 4096`, up to ~57k) the product is ~1.0e6, resolution ≈ 0.06 — already visibly degraded. The result is a frozen, banded, or near-constant streak mask instead of animated whitewater. The `uvOrigin` value needed for the fix is already computed in `main()` at `:434-449` for both the flat-plane and waterfall branches; it simply isn't passed to `foamFlowStreaks`.
- **Evidence**: `foamFlowStreaks(vec3 worldPos, float time)` takes no origin parameter; all three call sites pass `vWorldPos` (absolute, set by `water.vert:107` as `worldPos.xyz + renderOrigin.xyz`). Reachability is real, not theoretical: `byroredux/src/env_translate.rs:140/147` assigns `WaterKind::Rapids` and `WaterKind::River` from WATR classification, and both branches call `foamFlowStreaks` (`:606`, `:609`); the waterfall branch (`:616`) calls it too.
- **Impact**: Visual-only — rivers/rapids/waterfalls in distant exterior cells lose their animated whitewater streaks (frozen or blocky mask). No NaN, no crash, no CPU-side effect. Reachable in exactly the worldspaces where rivers are most common, and it directly contradicts this dimension's "procedural-noise precision bound marked for absolute-world UVs (regression guard, #1502)" checklist item.
- **Related**: #1502, #1997; `AUDIT_RENDERER_2026-07-15_DIM15.md` REN-D15-01 (the finding that produced the `sampleScrollingNormal` half of the fix).
- **Suggested Fix**: Add an `originOffset` (or `vec3 renderOriginXYZ`) parameter to `foamFlowStreaks` and subtract it from `worldPos` before the `dot(...)` projections — i.e. `float u = dot(worldPos - renderOrigin.xyz, flowDir) - speed * time;`. Same one-line pattern as the `sampleScrollingNormal` rebase; the origin is already in the camera UBO, so no new plumbing is needed.

#### REN-D16-2026-08-07-01: Integrated froxel volume stores slab-BACK-face cumulative state but composite samples it at texel CENTER — half-slab forward fog bias
- **Severity**: MEDIUM
- **Dimension**: Volumetrics (Dim 16)
- **Location**: `/mnt/data/src/gamebyro-redux/crates/renderer/shaders/volumetrics_integrate.comp:main` / `/mnt/data/src/gamebyro-redux/crates/renderer/shaders/composite.frag:hybridSliceCoordinate`
- **Status**: NEW (adjacent to, but distinct from, #1462 — that fix moved the *injection* sample from slice-CENTER to slice-FRONT-EDGE; the integrate→composite texel-center mapping was not touched)
- **Description**: `volumetrics_integrate.comp` accumulates a slab and then writes the post-slab cumulative state into texel index `slice`:
  `inscatter_total += inscatter * trans_cumulative * dt; trans_cumulative *= exp(-extinction*dt); imageStore(integrated, ivec3(col, slice), vec4(inscatter_total, trans_cumulative));`
  The stored value therefore physically lives at normalized depth `u = (slice+1)/N` (the slab's back face — the shader comment says so explicitly: *"store cumulative state at its back face"*). But `composite.frag` fetches with `texture(volumetricFroxel, vec3(fragUV, slice))` where `slice = hybridSliceCoordinate(min(worldDist, gridFar))` returns a **plain normalized [0,1] depth**, and a `sampler3D` places texel `k` at `u = (k+0.5)/N`. The lookup for a fragment truly at `u = (k+1)/N` lands halfway between texel `k` and `k+1`, i.e. it returns roughly the cumulative state of a point **half a slab deeper** than the fragment. There is no `-0.5/N` (or `+0.5/N`) texel-center correction on either side.
- **Evidence**:
  - `volumetrics_integrate.comp:101-119` — `sliceFront = slice/size.z`, `sliceBack = (slice+1)/size.z`, `dt = sliceDistance(sliceBack) - sliceDistance(sliceFront)`, store at `ivec3(col, slice)`.
  - `composite.frag:492-493` — `float slice = hybridSliceCoordinate(min(worldDist, gridFar)); vec4 vol = texture(volumetricFroxel, vec3(fragUV, slice));`
  - `composite.frag:99-109` — `hybridSliceCoordinate` returns exactly the normalized-depth `u` (inverse of `sliceDistance`), with no `(u*N - 0.5)/N` re-centering.
- **Impact**: Systematic over-application of fog: every fragment is attenuated by (and receives inscatter from) roughly half a slab of extra medium. Worst in the near field, where the hybrid-Z distribution is *linear* and the first `LINEAR_SLICE_FRACTION = 0.125` of slices cover `LINEAR_DEPTH = 350` world units — with 64 slices that is 8 linear slices of ~44 world units each, so near-camera fragments are biased by ~22 world units of medium. Also softens/advances god-shaft boundaries by half a slab along the view ray, partially defeating the crisp-boundary goal M55 states. Blast radius: every fog-bearing cell (interior and exterior); it is a bias, not a crash, so it is invisible to `cargo test`.
- **Related**: #1462 (inject slice-center → front-edge reconciliation), REN-D16-01/#2225 (height-fog datum), #928 (`VOLUMETRIC_OUTPUT_CONSUMED`).
- **Suggested Fix**: Pick one convention and apply it on both ends. Cheapest: in `composite.frag`, convert the normalized depth to a texel-aligned coordinate for the back-face convention — `slice_tc = clamp((u * N - 0.5) / N, 0.0, 1.0)` with `N = params.volume_params.w` (already plumbed as the slice count into `IntegrationParams.grid.w`; expose the same to composite). Alternatively have `integrate` store at the slab **center** (`sliceDistance((slice+0.5)/N)` semantics) so the texel-center fetch is already correct — but that then needs `inject`'s front-edge convention (#1462) re-reconciled.

#### REN-D17-NEW-01: Kaplanyan-Hoffman specular AA filters α instead of α² — under-filters exactly the smooth surfaces it exists for
- **Severity**: MEDIUM
- **Dimension**: Disney BSDF (Dim 17)
- **Location**: `/mnt/data/src/gamebyro-redux/crates/renderer/shaders/include/pbr.glsl:specularAaRoughness` (lines 210-217)
- **Status**: NEW
- **Description**: The published filter (Kaplanyan & Hoffman 2016; Filament
  `normalFiltering()`, §4.10.1) widens the GGX **α²** by the kernel variance:
  `α²_filtered = α² + 2σ²`. This shader's documented convention is
  `α = roughness²` — stated explicitly by the sibling helper
  (`deriveAxAy`: *"shader convention: α = roughness²"*) and confirmed by
  `distributionGGX`, whose local `a = roughness*roughness` is α and `a2 = a*a`
  is α². `specularAaRoughness` instead computes `roughness2 = roughness *
  roughness` — which is **α, not α²** — adds `2 * kernelVariance` to that, and
  `sqrt`s back. The caller squares the return, so the effective result is
  `α_filtered = α + 2σ²` rather than `α_filtered = sqrt(α² + 2σ²)`.
- **Evidence**:
  ```glsl
  // pbr.glsl:210
  float specularAaRoughness(vec3 N, float roughness) {
      vec3 dNdx = dFdx(N);
      vec3 dNdy = dFdy(N);
      float kernelVariance = 0.25 * (dot(dNdx, dNdx) + dot(dNdy, dNdy));
      float roughness2 = roughness * roughness;      // == α, NOT α²
      float filteredR2 = clamp(roughness2 + 2.0 * kernelVariance, 0.025 * 0.025, 1.0);
      return sqrt(filteredR2);                       // caller squares → α + 2σ²
  }
  ```
  Call path: `triangle.frag:2269` and `lighting.glsl:119` →
  `distributionGGX(NdotH, aaRoughness)` / `deriveAxAy(aaRoughness, …)`, both of
  which square the argument to get α.
  Numeric gap at perceptual roughness `p = 0.1` (α = 0.01), σ² = 1e-3:
  current form gives α_f = 0.012; the published form gives
  `sqrt(1e-4 + 2e-3) = 0.0458` — roughly **4× narrower** filtering than intended.
  The two forms only converge as `p → 1`.
- **Impact**: Specular aliasing is under-suppressed on low-roughness normal-mapped
  surfaces at distance — corrugated metal, brick mortar, fence cutouts, polished
  trim. That is the exact regression class the helper's own docstring cites
  (Nellis Museum / Quonset interiors). The lobe is widened by a constant
  `2σ²` regardless of base roughness, so smooth surfaces (where aliasing is
  worst, because the lobe is narrowest) get proportionally the least help.
  Blast radius: every raster fragment with `DBG_DISABLE_SPECULAR_AA` clear —
  both the no-cluster fallback directional and the clustered
  `shadowableLightRadiance`; both isotropic and anisotropic NDF branches.
- **Related**: `deriveAxAy`'s "0.025 floor mirrors specularAaRoughness's
  filteredR² ≥ 0.025² clamp" comment inherits the same mis-scaling.
  `AUDIT_RENDERER_2026-05-07.md:39` marks this helper "verified correct" —
  that verification did not trace the α-vs-α² convention through the caller.
- **Suggested Fix**: Square once more before adding the variance and take the
  fourth root on the way out, i.e. `filteredA2 = clamp(roughness2*roughness2 +
  2*kernelVariance, …); return sqrt(sqrt(filteredA2));` — matching Filament's
  `perceptualRoughnessToRoughness` / `roughnessToPerceptualRoughness` round-trip.
  Re-check the `0.025²` floor's meaning under the corrected units and recompile
  `triangle.frag.spv` + `water.frag.spv`.

#### REN-D17-NEW-02: `pathEnvironmentRadiance` converts the DALC arm to radiance but not its `sceneFlags.yzw` siblings — ~π step between Skyrim and FO3/FNV cells
- **Severity**: MEDIUM
- **Dimension**: Disney BSDF (Dim 17)
- **Location**: `/mnt/data/src/gamebyro-redux/crates/renderer/shaders/include/lighting.glsl:pathEnvironmentRadiance` (lines 232-244); same asymmetry at `/mnt/data/src/gamebyro-redux/crates/renderer/shaders/triangle.frag:2212-2213`
- **Status**: NEW (follow-on to the #2244 fix in `c4cb2614`, not a regression of it)
- **Description**: #2244 correctly established that `sampleDalcCube` returns
  authored *irradiance* and therefore needs `* (1.0 / PI)` before feeding a path
  integrator's environment (radiance) term. But `sampleDalcCube` and
  `sceneFlags.yzw` (XCLL cell ambient) are the **two arms of the same ambient
  term** everywhere else in the shader — `triangle.frag:3389-3401` picks one or
  the other for `indirectLight` with identical downstream treatment, and
  `triangle.frag:2084-2107` builds `ambient` from `sceneFlags.yzw` with no /PI.
  They are the same class of quantity. Yet in `pathEnvironmentRadiance` only the
  DALC branch is divided by π; the exterior branch (line 236) and the interior
  non-DALC fallback (line 243) are not.
- **Evidence**:
  ```glsl
  // lighting.glsl:232
  vec3 pathEnvironmentRadiance(vec3 direction) {
      vec3 rayDir = normalize(direction);
      if (jitter.w > 0.5) {
          float skyWeight = smoothstep(-0.2, 0.8, rayDir.y);
          return mix(sceneFlags.yzw, skyTint.xyz, skyWeight);   // no /PI
      }
      if (dalcFlags.x > 0.5) {
          return sampleDalcCube(rayDir) * (1.0 / PI);           // /PI  (#2244)
      }
      return sceneFlags.yzw * 0.5;                              // no /PI
  }
  ```
  and the reflection-miss sibling:
  ```glsl
  // triangle.frag:2212
  ? sampleDalcCube(R) * (1.0 / PI)
  : sceneFlags.yzw;
  ```
- **Impact**: For identically-authored ambient, a Skyrim DALC-authored cell now
  gets a bounded-path escape / reflection-miss environment term ~π× (≈3.14×)
  dimmer than an FO3/FNV/Oblivion XCLL cell. That is the same systematic
  cross-game ambient gap REND-#1452 fixed on the direct-ambient path, re-opened
  on the indirect path. Visible as darker indirect floors / reflection misses in
  Skyrim interiors relative to Fallout interiors.
- **Related**: REN-D17-02 (`AUDIT_RENDERER_2026-08-02.md:275-277`, fixed by #2244);
  the regression guard `bounded_path_converts_dalc_irradiance_to_environment_radiance`
  (`crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:1148`)
  pins the DALC arm only.
- **Suggested Fix**: Pick one convention for the pair and apply it to all three
  arms of `pathEnvironmentRadiance` (and both arms at `triangle.frag:2212`).
  Given `ambient` at 2084 and `sampleDalcCube(N)` at 3396 are used
  interchangeably, `sceneFlags.yzw` is also irradiance and should take the same
  `1/PI`; extend the regression test to cover the non-DALC arms.

#### REN-D18-NEW-01: `build_tod_keys` afternoon re-anchor clamps against the wrong neighbour — TOD key table goes non-monotonic on short-day climates
- **Severity**: MEDIUM
- **Dimension**: Sky/Weather (Dim 18)
- **Location**: `byroredux/src/systems/weather.rs:build_tod_keys` (line ~37), test `tod_keys_clamp_afternoon_cool_on_compressed_days` (line ~823)
- **Status**: NEW
- **Description**: `build_tod_keys` emits 7 `(hour, slot)` pairs that `pick_tod_pair` walks as a *strictly increasing* piecewise-linear table. Key 3 is the synthetic `afternoon_peak = (sunrise_end + sunset_begin) * 0.5` (HIGH_NOON) and key 4 is the `afternoon_cool` DAY re-anchor, clamped as `(sunset_begin - 2.0).max(sunrise_end + 0.1)`. The clamp is anchored to **key 2** (`sunrise_end`), not to its actual predecessor **key 3** (`afternoon_peak`). Solving `afternoon_cool < afternoon_peak` gives the trigger condition `0.2 < (sunset_begin - sunrise_end) < 4.0` — i.e. every climate whose clear-day window is under 4 hours produces a table where key 4 sits *before* key 3. The dedicated regression test that exists to pin this clamp asserts `keys[4].0 > keys[2].0` (against `sunrise_end`) rather than `keys[4].0 >= keys[3].0`, so it passes on inputs that already violate the invariant; the sibling monotonicity test's corpus (`[7,11,16,19]` is the shortest day it tries, gap = 5h) never crosses the 4h threshold either. Both tests therefore give false assurance.
- **Evidence**:
  ```rust
  // weather.rs::build_tod_keys
  let afternoon_peak = (sunrise_end + sunset_begin) * 0.5;          // key 3
  let afternoon_cool = (sunset_begin - 2.0).max(sunrise_end + 0.1); // key 4 — clamped vs key 2, not key 3
  ```
  Worked example with the *exact input the "clamp" test already uses*, `tod_hours = [5.0, 10.0, 11.0, 20.0]`:
  `afternoon_peak = 10.5`, `afternoon_cool = max(9.0, 10.1) = 10.1` → keys = `[1, 5, 10, 10.5, 10.1, 11, 22]`, with `keys[4] (10.1) < keys[3] (10.5)`.
  Downstream in `pick_tod_pair`, the `h >= h0 && h < h1` scan can never satisfy `h >= 10.5 && h < 10.1`, so the HIGH_NOON→DAY ease-out segment is unreachable and hour `10.5` snaps straight from full HIGH_NOON to `mix(DAY, SUNSET, 0.555)` (matched by the *later* `i=4` segment `10.1 → 11`). `t` never goes negative (the guard makes the inverted segment unmatchable), so there is no NaN — the failure is a hard discontinuity, not a numeric blowup.
  The same `(slot_a, slot_b, t)` tuple drives `tod_slot_night_factor` → `fog_near`/`fog_far`/`fog_medium`, so fog distance snaps in lockstep with the palette. `climate_tod_hours` accepts any TNAM byte in `1..=144`, so nothing upstream filters short-day climates out.
- **Impact**: A single-frame discontinuous jump in zenith/horizon/lower/sun/ambient/sunlight/fog colour **and** fog near/far distance, occurring once per in-game day at `hour == afternoon_peak`, on any worldspace whose CLMT ships `sunset_begin - sunrise_end < 4h`. Vanilla FNV (`[6,10,18,22]`, gap 8h) and FO3 Capital Wasteland (`[5.33,10,17,22]`, gap 7h) are safe, so shipped content is unaffected today; reachable on modded/authored CLMTs and on any synthetic climate. Blast radius is the whole exterior frame (palette + fog + `CellLightingRes.ambient`/`directional_color`), but it is visual-only and self-corrects on the next segment.
- **Related**: Same key table as #463 / #530 / #897 (fog-palette lockstep). Not covered by any open issue in `issues.json`; not mentioned in `AUDIT_RENDERER_2026-07-15_DIM18.md`, whose "TOD color easing" pass-through checked interpolation form rather than key ordering.
- **Suggested Fix**: Clamp against the true predecessor: `let afternoon_cool = (sunset_begin - 2.0).max(afternoon_peak + 0.1).min(sunset_begin - 1e-3);` and tighten `tod_keys_clamp_afternoon_cool_on_compressed_days` to assert full `windows(2)` monotonicity (or add `[5.0, 10.0, 11.0, 20.0]` to `tod_keys_are_monotonic_on_realistic_climates`'s corpus, which fails today).

#### REN-D19-03: Near-field LAND terrain ships a zero tangent, so every TX01 terrain normal map shades through the screen-space-derivative fallback
- **Severity**: MEDIUM
- **Dimension**: Tangent-Space (Dim 19)
- **Location**: `/mnt/data/src/gamebyro-redux/crates/renderer/src/vertex.rs:148` (`Vertex::new_terrain`), consumed at `/mnt/data/src/gamebyro-redux/byroredux/src/cell_loader/terrain.rs:457` (`spawn_terrain_mesh`)
- **Status**: NEW
- **Description**: `Vertex::new_terrain` hard-codes `tangent: [0.0, 0.0, 0.0, 0.0]`, and `spawn_terrain_mesh` is the only builder for near-field LAND tiles. `triangle.frag` gates Path 1 on `dot(vertexTangent.xyz, vertexTangent.xyz) > 1e-4`, so every LAND fragment takes Path 2 (screen-space derivative TBN) — including the per-splat-layer `perturbNormal` calls that apply the TX01 tangent-space normal maps. This is exactly the path the DIM19 checklist wants reserved for "synthetic geometry with no tangent". Terrain UVs are a regular axis-aligned grid, so an authored/synthesized tangent is trivially derivable and would be exact.
- **Evidence**:
  - `crates/renderer/src/vertex.rs:165` — `tangent: [0.0, 0.0, 0.0, 0.0]` inside `new_terrain`.
  - `crates/renderer/shaders/triangle.frag:452-463` — "LAND TX01 normal maps follow the same splat weights and ordering as ..." then `perturbNormal(..., fragTangent)` per layer; `fragTangent` is zero for these vertices, so `material_sampling.glsl:162` fails and Path 2 at `:176` runs.
  - Contrast the **distant** band: `byroredux/src/cell_loader/terrain_lod_btr.rs:168-172` explicitly carries `mesh.tangents` through with the anisotropic XZ correction (`v.tangent = [wt.x, wt.y, wt.z, tg[3]]`), and `lod_support.rs:96-97` does the same for object LOD. Near-field is the only band without tangents.
- **Impact**: Path 2's `T` is constant per triangle (`dFdx/dFdy` of a planar-interpolated quantity), so terrain normal-map detail is shaded with a piecewise-constant tangent frame instead of a vertex-smooth one — faceting along the LAND grid on normal-mapped ground, most visible on high-frequency rock/gravel TX01 maps at grazing angles. It also produces a shading discontinuity across the near/distant terrain LOD boundary, because the BTR band *does* use Path 1. Blast radius: all exterior worldspaces; interiors unaffected.
- **Related**: #2371 (EX-10/11 near-terrain correctness + distant LOD bands) is the natural home; REN-D19-01 (#2245) fixed a Path-2 handedness bug that terrain is the largest remaining consumer of.
- **Suggested Fix**: Have `spawn_terrain_mesh` fill the tangent lane. Because LAND UVs are a uniform grid aligned to world XZ, `T` is the normalized world +X direction re-orthogonalized against the vertex normal with `w = 1.0` (same construction `cell_loader/water.rs:118` already uses); alternatively route the tile through `synthesize_tangents_yup` for a fully general answer.

#### REN-D20-NEW-01: `EguiPass` render pass survives a swapchain *format* change (framebuffers only are rebuilt)
- **Severity**: MEDIUM
- **Dimension**: Debug/Telemetry (Dim 20)
- **Location**: `crates/renderer/src/vulkan/egui_pass.rs:EguiPass::recreate_framebuffers` / `crates/renderer/src/vulkan/context/resize.rs:878-889`
- **Status**: NEW
- **Description**: `recreate_swapchain` explicitly treats a surface-format change as a
  reachable case: it computes `let format_changed = self.swapchain_state.format != old_swapchain_format;`
  (`resize.rs:186`) and, when true, tears down + rebuilds the main render pass and every
  rasterization pipeline (`resize.rs:187-212, 300+`). The `presentation` pipeline — the other
  swapchain-format-dependent object — is destroyed and reconstructed unconditionally on every
  resize (`resize.rs:932-971`), so it also picks up the new format. `EguiPass` does neither. It
  only gets `recreate_framebuffers`, whose doc comment asserts the opposite of what the sibling
  code assumes: *"The render pass itself stays — the swapchain format is the same after resize."*
  Two consequences on a format change: (a) `create_framebuffers` attaches the new image views to
  a render pass whose attachment `format` is the *old* one
  (VUID-VkFramebufferCreateInfo-pAttachments-00880), and (b) `Options::srgb_framebuffer`, computed
  once in `EguiPass::new` from `is_srgb_format(swapchain_format)`, is never recomputed, so the
  overlay's gamma curve silently flips wrong (over-saturated / muddy) even if (a) were tolerated.
- **Evidence**:
  ```rust
  // resize.rs:186 — the codebase's own admission that this is reachable
  let format_changed = self.swapchain_state.format != old_swapchain_format;
  ...
  // resize.rs:883 — egui gets framebuffers only, no format re-check
  if let Some(ref mut pass) = self.egui_pass {
      pass.recreate_framebuffers(&self.device, &self.swapchain_state.image_views,
                                 self.swapchain_state.extent)?;
  }
  ```
  vs. `egui_pass.rs:121-123`: *"The render pass itself stays — the swapchain format doesn't change on resize."*
- **Impact**: Only fires when the surface format actually changes across a recreate (HDR/SDR
  display switch, monitor move, driver-side format renegotiation). When it does, the `?` on
  `recreate_framebuffers` propagates out of `recreate_swapchain`, which is a hard failure of the
  whole resize, not a graceful overlay-off. Blast radius is the entire frame loop, not just the
  overlay. Frequency is low; severity when hit is high.
- **Related**: #576 (PIPE-2, the format-gated pipeline rebuild this path was modelled on);
  #1433 (egui incoming dependency); `resize.rs:932-971` (presentation's unconditional rebuild is
  the pattern to copy).
- **Suggested Fix**: Pass the new format into the egui resize hook and, when it differs from the
  one `EguiPass::new` was built with, drop + rebuild the whole `EguiPass` (as `presentation` does)
  rather than only its framebuffers. **Needs a format-change repro (HDR toggle / monitor move) or
  RenderDoc to observe** — the failure mode does not appear in `cargo test`.

#### REN-D20-NEW-02: Debug-UI HUD sums GPU bracket times into a "Σ ms" total the timer module explicitly forbids
- **Severity**: MEDIUM
- **Dimension**: Debug/Telemetry (Dim 20)
- **Location**: `crates/debug-ui/src/panels.rs:175-176` (producer: `byroredux/src/systems/metrics.rs:110-131`)
- **Status**: NEW
- **Description**: `GpuTimerSnapshot`'s doc comment states the contract in as many words:
  every bracket's START is written at `TOP_OF_PIPE`, so queue-drain time from prior in-flight work
  is absorbed into the bracket — *"the fields must NOT be summed into a 'total GPU ms' without
  that caveat, since overlapping queue-wait could be double-counted across adjacent brackets."*
  The HUD does exactly that sum and presents it as an unqualified headline figure.
- **Evidence**:
  ```rust
  // crates/debug-ui/src/panels.rs:175
  let gpu_total: f32 = m.gpu_pass_ms.iter().map(|(_, v)| *v).sum();
  ui.label(egui::RichText::new(format!("GPU passes — Σ {:.3} ms", gpu_total)).strong());
  ```
  Contract being violated — `crates/renderer/src/vulkan/gpu_timers.rs:124-132`:
  *"**Upper bound, not a precise attribution (#2040 / PERF-D9-01).** … the fields must NOT be
  summed into a 'total GPU ms' …"*
- **Impact**: The overlay is the primary tool used to chase frame-time pathologies (it was built
  for the "540 ms / 1 FPS" investigation — see the metrics.rs comment at line 114-116). A Σ that
  double-counts queue-wait across 14 brackets will read materially higher than wall GPU time,
  and the adjacent "CPU draw_frame — Σ" label at `panels.rs:198` invites a direct GPU-vs-CPU
  comparison that the GPU number cannot support. Risk is a misdiagnosed perf bug, not a crash.
- **Related**: #2040 / PERF-D9-01 (the finding that established the non-summability caveat);
  REN-D20-NEW-03 below (same telemetry surface, same root cause of the caveats not reaching the UI).
- **Suggested Fix**: Either drop the Σ from the GPU row (keep the per-pass grid, which is sound),
  or relabel it to something honest like "Σ upper bounds (overlaps double-counted)" and mirror the
  `gpu_timers.rs` caveat in a tooltip.

#### REN-D21-2026-08-07-01: Cornell can never exercise the Disney BSDF branch — the diffuse lobe every BGSM-sourced game takes
- **Severity**: MEDIUM
- **Dimension**: Cornell Harness (Dim 21)
- **Location**: `/mnt/data/src/gamebyro-redux/byroredux/src/cornell.rs:matte/pbr/glass/emissive/fire_refraction` + `/mnt/data/src/gamebyro-redux/byroredux/src/commands/scene.rs:MatSetCommand::execute`
- **Status**: NEW
- **Description**: Every Cornell probe is built from `Material { .. ..Default::default() }`, and `Material::default()` sets `effect_shader_flags: 0` (`crates/core/src/ecs/components/material.rs:394`). `collect_static_mesh_draws` forwards that verbatim (`effect_shader_flags: mat.map(|m| m.effect_shader_flags).unwrap_or(0)`, `static_meshes.rs:718`) into `GpuMaterial.material_flags`. The shared direct-lighting BRDF branches on that bit: `include/lighting.glsl:155` `if ((mat.materialFlags & MAT_FLAG_PBR_BSDF) != 0u) { ...disneyDiffuseSplit... } else { diffuseBrdf = kD * albedo; }` (same gate again at `triangle.frag:2322`). So *all* Cornell probes — including the two sweep rows that exist specifically to read metalness/roughness response — are shaded through the legacy Lambert path, while every BGSM/BGEM-sourced surface (FO4, Skyrim SE, FO76, Starfield; `material_flag::PBR_BSDF` is set for all `is_pbr` content since #1352) takes the Disney path. `mat.set` has no arm for `material_kind`'s sibling flag word nor for `subsurface`/`sheen`/`sheen_tint`/`anisotropic`, so the harness cannot be flipped into that branch at runtime either.
- **Evidence**: `cornell.rs` `pbr()` returns `Material { diffuse_color, metalness, roughness, ..Default::default() }` → `effect_shader_flags == 0`; `MatSetCommand` field table is `metalness|roughness|alpha|glossiness|emissive_mult|specular_strength|env_map_scale|ior|color|diffuse_color|emissive_color|specular_color|material_kind` — no `material_flags`, no Disney scalars.
- **Impact**: The reference scene silently answers for the wrong BRDF on the majority of target content. A regression isolated to `disneyDiffuseSplit` / the sheen-subsurface lobe (e.g. REN-D17-01's π disagreement) bisects clean in Cornell and then reproduces in-game — the exact false-all-clear failure mode #1942 fixed for the sun path. It also means the standing "metalness looks off" observation cannot be reproduced under the shading path FO4 content actually uses.
- **Related**: #1942 (same class of harness blind spot, sun path); REN-D21-03/#2249 (same class: a material field the harness structurally could not reach); REN-D17-01 (`disneyDiffuseSplit` sheen weight π mismatch — the defect this gap would hide).
- **Suggested Fix**: Add a `mat.set <id> material_flags <u32>` (or a named `pbr_bsdf on|off`) arm plus `subsurface|sheen|sheen_tint|anisotropic` scalar arms wired to the corresponding `Material` fields, and spawn at least one probe row with `effect_shader_flags |= MAT_FLAG_PBR_BSDF` so both diffuse branches are on screen side by side.

#### REN-D22-03: Flicker/pulse *parameters* bypass the per-game boundary the *flags* respect — pre-Skyrim lights can never animate
- **Severity**: MEDIUM
- **Dimension**: Light Animation (Dim 22)
- **Location**: `crates/plugin/src/esm/cell/support.rs:75` (`build_static_object_from_subs`, `b"DATA" if is_ligh` arm) → `byroredux/src/cell_loader/references/attach.rs:417` (`attach_light_flicker_if_needed`)
- **Status**: NEW
- **Description**: `canonical_light_animation_flags` canonicalizes the *flags*
  per game, but the *animation parameters* they drive (`period_secs`,
  `intensity_amplitude`, `movement_amplitude`) are read at fixed **Skyrim**
  `DATA` offsets 28/32/36 for every `DATA`-layout game, gated only on the
  subrecord *length*. Skyrim's LIGH `DATA` is 48 bytes
  (…, 24 Near Clip, 28 Flicker Period, 32 Intensity Amplitude, 36 Movement
  Amplitude, 40 Value, 44 Weight). Oblivion/FO3/FNV's `DATA` is 32 bytes and
  ends `…, 16 Falloff, 20 FOV, 24 Value(u32), 28 Weight(f32)`. Consequences on
  FO3/FNV (and 32-byte Oblivion):
  1. `len >= 32` is true → `period_secs = read_f32(28)` reads the record's
     **Weight**, not a flicker period. The `> 0.0` fallback in
     `attach_light_flicker_if_needed` therefore *doesn't* fire (weight is
     positive), so a garbage period is kept instead of the 0.5 s default.
  2. `len >= 36` is false → `intensity_amplitude = 0.0`, with no fallback
     anywhere. `flicker_intensity` then returns `1.0 + m*0.0*0.5 == 1.0`
     always. Every FNV/FO3/Oblivion torch, candle and campfire that authored
     the Flicker bit gets a `LightFlicker` attached, is iterated every frame,
     and produces **exactly zero** visible animation.
  The doc on `LightData.period_secs` ("Zero when the LIGH record's DATA
  subrecord is truncated (pre-Skyrim … only the 16-byte header)") encodes the
  wrong premise: pre-Skyrim `DATA` is 32 bytes, not 16, so the intended
  "absent → 0 → fall back" path never triggers.
- **Evidence**:
```rust
// support.rs — one Skyrim-layout decode for every DATA-layout game
let period_secs        = if sub.data.len() >= 32 { read_f32(28) } else { 0.0 }; // FNV: Weight
let intensity_amplitude= if sub.data.len() >= 36 { read_f32(32) } else { 0.0 }; // FNV: absent → 0
// attach.rs — only period has a fallback; amplitude has none
let period_secs = if ld.period_secs > 0.0 { ld.period_secs } else { 0.5 };
// light_anim.rs:179
1.0 + modulation * flicker.intensity_amplitude * FLICKER_INTENSITY_DAMPING  // == 1.0 when amp == 0
```
- **Impact**: Visual-only, but wide: flicker/pulse is silently dead on the
  project's most-exercised game (FNV) and on FO3/Oblivion — every interior
  torch is a constant light. Also wasted per-frame work (a `LightFlicker`
  slot + query hit per torch that provably cannot change anything), and a
  latent trap: anyone who later adds an `intensity_amplitude` default would
  immediately start driving the animation at a period sourced from the
  record's Weight field.
- **Related**: #2250 / #2251 (the flag half of the same boundary);
  REN-D22-04 below shares the `flicker_intensity` call path.
- **Suggested Fix**: Discriminate the `DATA` arm on game/length like the
  `DAT2` arm already does — only read 28/32/36 when the layout actually has
  them (Skyrim+/48-byte), and treat pre-Skyrim as "no authored flicker
  parameters". Then give `intensity_amplitude` an explicit default at the
  boundary (same shape as the existing `period_secs` 0.5 fallback) so
  pre-Skyrim Flicker/Pulse bits still animate with engine-chosen amplitude,
  or skip the `LightFlicker` attach entirely when no parameters exist.

#### REN-D22-04: `PULSE_SLOW` is a half-wave-rectified sine at the *same* rate, not a half-speed pulse
- **Severity**: MEDIUM
- **Dimension**: Light Animation (Dim 22)
- **Location**: `byroredux/src/systems/light_anim.rs:145-162` (`flicker_intensity`, pulse branch)
- **Status**: NEW
- **Description**: `speed_scale` (0.5 for the SLOW bits) is applied *inside*
  the sine, but the argument has already been wrapped to one cycle per
  `period_secs` by `rem_euclid`. Multiplying the normalized phase by 0.5
  therefore does not halve the frequency — it truncates the waveform to its
  positive half and repeats it at the original rate. This works correctly for
  the flicker branch (`speed_scale` there multiplies *time*, before the
  bucket step, so 12 Hz → 6 Hz is genuinely half-rate); only the pulse branch
  is wrong.
- **Evidence**:
```rust
let phase_secs = (total_time + flicker.phase_offset_secs).rem_euclid(flicker.period_secs);
let phase = phase_secs / flicker.period_secs;             // sawtooth 0..1 once per period
(phase * speed_scale * std::f32::consts::TAU).sin()       // sin(pi*phase) for SLOW
```
  With `period = 1 s`: modulation peaks at t = 0.5, 1.5, 2.5 … and returns to
  0 at every integer second — it never goes negative. A true half-rate pulse
  (`sin(TAU*t/2)`) would trough at t = 1.5. So `PULSE_SLOW` (a) pulses at the
  same rate as `PULSE`, and (b) only ever *brightens* the light (mean
  `+2/pi * amp * damping`) instead of oscillating around the authored
  intensity.
- **Impact**: Every `PULSE_SLOW` light (ambience set-pieces, glowing
  crystals) renders visibly wrong — brighter on average and at the wrong
  cadence. Same-frequency-but-rectified is also what a "both PULSE and
  FLICKER_SLOW authored" light gets, since `speed_scale` keys off either SLOW
  bit while the pulse branch wins the shape selection.
- **Related**: the existing test `pulse_slow_runs_at_half_angular_velocity`
  cannot detect this — it samples only `t = period/4`, where the rectified
  and the true half-rate waveform coincide exactly (`sin(pi/4)` both ways).
- **Suggested Fix**: Scale the *period*, not the phase —
  `rem_euclid(period / speed_scale)` then divide by the same value (or drop
  the wrap and use `sin(TAU * (t + off) * speed_scale / period)`), and extend
  the test to a sample past one period (e.g. `t = 1.5 * period`) where the two
  waveforms differ in sign.

#### REN-D23-2026-08-07-01: FSR-failure fallback stays at the reduced render resolution with no temporal AA, contradicting `UpscalerMode::Taa`'s own doc
- **Severity**: MEDIUM
- **Dimension**: FSR Upscaler (Dim 23)
- **Location**: `/mnt/data/src/gamebyro-redux/crates/renderer/src/vulkan/upscaling.rs:UpscalerMode::Taa` (doc), `/mnt/data/src/gamebyro-redux/crates/renderer/src/vulkan/frame_upscaler.rs:FrameUpscaler::record` / `record_native_blit`, `/mnt/data/src/gamebyro-redux/crates/renderer/src/vulkan/context/mod.rs:2641` (TAA construction gate)
- **Status**: NEW
- **Description**: `UpscalerMode::Taa`'s doc comment states it is "the compatibility fallback taken whenever FSR context creation or dispatch fails". Nothing in the code takes that fallback. On FSR context-creation failure (`FrameUpscaler::new`, the `Err(error)` arm just logs) or on a latched `dispatch_failure`, `renderer_config.upscaler` remains `Fsr3(..)`, so `frame_extents.render` stays at the preset's reduced extent (1280x720 for Quality at 1080p), `self.taa` is `None` (built only when `renderer_config.upscaler == UpscalerMode::Taa`), and jitter is forced to `(0.0, 0.0)`. The degraded image is therefore a *plain bilinear stretch of an un-anti-aliased 720p render*, not a TAA-resolved native frame. Since FSR Quality is the engine default, this is the state every user with a non-working FSR provider lands in.
- **Evidence**: `frame_upscaler.rs` context-creation failure arm — `log::error!("FSR context creation failed: {error}; using native HDR blit fallback")` with no mode change; `context/mod.rs:2641` `let mut taa = if renderer_config.upscaler == UpscalerMode::Taa { ... } else { log::info!("FSR mode active: TAA history/resolve disabled ..."); None };`; `draw.rs:1573` FSR arm returns `(0.0, 0.0, None, false)` when `!is_fsr_dispatch_active()`.
- **Impact**: Silent, large quality regression (720p bilinear, aliased, no AA) on any machine where the FSR provider fails to initialize, reported only via one `log::error!` and the `ctx`-level telemetry string. Blast radius: the whole frame, permanently, for the session.
- **Related**: `AUDIT_RENDERER_2026-07-28.md` §"FSR 3.1 Residual Scope" (listed forced-failure/live-switching as untested, did not name this); `FrameUpscaler::telemetry`.
- **Suggested Fix**: Either escalate the context-creation failure into a `set_upscaler_mode(UpscalerMode::Taa, ..)` at startup (the machinery already exists and is rollback-safe), or fix the `UpscalerMode::Taa` doc comment to say the fallback is a *native blit at the FSR render extent*, not the TAA mode. The dispatch-failure latch is a harder case (it fires mid-frame) but could set a "re-evaluate mode at next frame boundary" flag.

---

### LOW

#### AS-D1-NEW-02: BLAS registration overwrites an occupied slot without destroying the previous acceleration structure
- **Severity**: LOW
- **Dimension**: AS Correctness (Dim 1)
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs:AccelerationManager::build_blas` and `::build_blas_batched` (Phase 7); mirrored in `blas_skinned.rs:AccelerationManager::build_skinned_blas_batched_on_cmd` (Phase 4)
- **Status**: NEW
- **Description**: All three registration sites assign unconditionally:
  - `self.blas_entries[handle] = Some(BlasEntry { … })` (both static sites)
  - `self.skinned_blas.insert(p.entity_id, BlasEntry { … })` (skinned site)

  If the slot/key already holds a live `BlasEntry`, the old value is dropped as plain memory. `GpuBuffer`'s `Drop` safety net (`buffer.rs`, `#656`) reclaims the backing buffer with a `log::warn!` + debug-assert, but `BlasEntry::accel` is a raw `vk::AccelerationStructureKHR` with no `Drop` — it leaks for the process lifetime. Additionally `total_blas_bytes` / `static_blas_bytes` are incremented for the new entry without decrementing the replaced one, so the eviction budget drifts upward permanently and `evict_unused_blas` starts firing against a phantom footprint.
- **Evidence**: The only structural protection today is caller discipline, and it is not uniform. `context/resources.rs::build_global_blas_for_draws` **does** guard (`if !cmd.in_tlas || accel.has_blas(cmd.mesh_handle) { return None; }`), but the general-purpose `context/resources.rs::build_blas_batched` wrapper does **not** — it filters only on `mesh.rt_capable` and buffer presence. The cell-loader callers happen to be safe because `cell_loader/spawn.rs` pushes to `blas_specs` only inside the fresh-upload branch (cache hits reuse the handle without re-pushing), and `cell_loader/exterior.rs` batches only freshly-created terrain/water meshes. `AccelerationManager::has_blas` exists and is public, but neither `build_blas` nor `build_blas_batched` consults it.
- **Impact**: No live path reaches it today, so this is a latent gap, not an active bug. Should a future streaming/hot-reload/LOD-swap path re-register an occupied handle, the symptom is a slow VRAM leak plus a silently-inflated BLAS budget — both easy to misattribute. Notably the symptom is *not* corruption: the new entry's address is correct, so rendering stays right while memory drifts.
- **Related**: `#1449` / MEM-01 (deferred-destroy on eviction — the pattern this site should reuse); `#372` (`drop_blas` deferred queue).
- **Suggested Fix**: At each of the three registration sites, `take()` any pre-existing entry first, subtract its `size_bytes` from `total_blas_bytes` (and `static_blas_bytes` on the static path), and push it onto `pending_destroy_blas` with `DEFAULT_COUNTDOWN` — exactly what `drop_blas` already does — before writing the new entry.

#### REN-D2-2026-08-07-03: Refraction passthru loop does not decrement `tMax`, so effective reach is 3x the documented 2000

- **Severity**: LOW
- **Dimension**: Ray Queries (Dim 2)
- **Location**: `crates/renderer/shaders/triangle.frag` — IOR refraction block, the `rayQueryInitializeEXT(refrRQ, ...)` call inside the `REFRACT_PASSTHRU_BUDGET` loop
- **Status**: NEW
- **Description**: Every iteration of the passthru loop re-issues the query with
  a hard-coded `2000.0` tMax while advancing `rayOrigin` past each skipped
  interface. Unlike the sibling loops — `traceReflection` (`remaining -= advance`),
  `traceShadowTransmittance` (`opaqueRemaining -= advance` and
  `remaining -= advance`), `traceWaterRay` (`remaining = maxDist - travelled`) —
  the refraction loop never decrements its reach.
- **Evidence**: `accumulatedDist` is tracked for the distance-attenuation term
  (`refrColor *= 1.0 / (1.0 + accumulatedDist * 0.002)`) but is never fed back
  into the query's tMax. With `REFRACT_PASSTHRU_BUDGET = 2` the ray can travel
  up to ~6000 world units across three segments while the code and comments
  describe a 2000-unit reach.
- **Impact**: Cosmetic/consistency, plus a mild cost overrun on stacked-glass
  views: a refraction ray can resolve a terminus three times farther away than
  the intended budget, then be heavily attenuated anyway by the distance term.
  No correctness break, no unbounded walk (iteration count is fixed at 3).
- **Related**: Sibling reach bookkeeping in `raytrace.glsl::traceReflection`
  and `shadow_transport.glsl::traceShadowTransmittance`.
- **Suggested Fix**: Track `refrRemaining = 2000.0` alongside `accumulatedDist`
  and subtract each `hDist + 0.05` per passthru, matching the three sibling
  traversal loops — or amend the comment to state the 3-segment reach is
  intended.

#### REN-D3-2026-08-07-03 / MAT-D7-2026-08-07-01 (MERGED): Load-bearing layout doc comments quote superseded byte sizes (112 B / 300 B)
- **Severity**: LOW
- **Dimension**: GPU-Struct Layout (Dim 3) **+** Material Table (Dim 7) — *cross-dimension duplicate, merged*
- **Location**: `/mnt/data/src/gamebyro-redux/crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:84`, `/mnt/data/src/gamebyro-redux/crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:97`, `/mnt/data/src/gamebyro-redux/crates/renderer/src/vulkan/scene_buffer/constants.rs:168` (`MAX_MATERIALS`)
- **Status**: NEW (generalizes the un-itemized "reference documentation is stale" note in the 2026-07-28 Dimension-3 summary, which the `docs/engine/*.md` side has since fixed; the in-code side has not)

> **Merge note.** Dimension 3 filed this as `REN-D3-2026-08-07-03` covering three in-code
> comment sites; Dimension 7 independently filed the `MAX_MATERIALS` site as
> `MAT-D7-2026-08-07-01`. Both original bodies are reproduced below verbatim.

**Dimension 3 body (`REN-D3-2026-08-07-03`)**

- **Description**: Three comments state byte sizes that the code contradicts.
  The prose is the primary reference a future field-adder reads before touching
  a struct whose whole risk profile is silent byte drift.
- **Evidence**:
  - `gpu_types.rs:84` — "The `size_of::<GpuInstance>() == 112` test below asserts
    the invariant" — sits directly under a layout history whose last line is
    `112 → 128 (#2219, …)`, and the test actually asserts **128**.
  - `gpu_instance_layout_tests.rs:97` — "rely on the size assertion above
    (112 B)" — the assertion above is 128 B.
  - `constants.rs:168` — `MAX_MATERIALS` doc: "16384 × 300 B ≈ 4.9 MB per frame …
    ≈ 9.8 MB total". `GpuMaterial` is **348 B** (pinned), so the real figures are
    5.7 MB / 11.4 MB — which is what `docs/engine/memory-budget.md:21` already
    says. The same doc also cites "the 4 GB total VRAM budget" while the current
    baseline note is 6 GB RT-minimum.
  - (Benign, not counted: `gpu_types.rs:123/126` and `descriptors.rs:317` /
    `upload.rs:558` use 112 B as deliberate *historical* context.)
- **Impact**: No runtime effect. Misleads the next author of a `GpuInstance`
  field addition or a VRAM-budget recalculation; the memory-budget arithmetic in
  `constants.rs` understates material-SSBO VRAM by ~16 %.
- **Related**: REN-D3-2026-08-07-01/02 (same file cluster).
- **Suggested Fix**: s/112/128/ in the two `GpuInstance` comments; recompute the
  `MAX_MATERIALS` doc arithmetic at 348 B and align the budget figure with
  `feedback_vram_baseline.md`.

**Dimension 7 body (`MAT-D7-2026-08-07-01`) — `MAX_MATERIALS` docstring quotes a stale 300 B `GpuMaterial` stride, understating the SSBO VRAM cost by ~16%**

- **Description**: The doc comment sizes the per-frame material SSBO as "16384 × 300 B ≈ 4.9 MB per frame × MAX_FRAMES_IN_FLIGHT (2) ≈ 9.8 MB total". `GpuMaterial` grew 300 → 348 B when the twelve supplemental semantic texture-role indices landed (pinned by `gpu_material_size_is_348_bytes`), so the true reservation is 16384 × 348 B = 5.70 MB per frame and 11.4 MB total. The allocation site itself is correct — it computes from `size_of::<GpuMaterial>()` — so this is a documentation-only drift, but the number is the one a reader consults when weighing a `MAX_MATERIALS` raise against the ~4 GB budget in `feedback_vram_baseline.md`.
- **Evidence**:
  - `constants.rs`: `/// [`super::super::material::MaterialTable`] SSBO. 16384 × 300 B ≈ 4.9 MB`
  - `material.rs`: `fn gpu_material_size_is_348_bytes() { assert_eq!(std::mem::size_of::<GpuMaterial>(), 348); }`
  - `buffers.rs`: `let material_buf_size = (std::mem::size_of::<...::GpuMaterial>() * MAX_MATERIALS) as vk::DeviceSize;` (correct, derives from the real size)
- **Impact**: No runtime effect. Misleads any future decision to raise `MAX_MATERIALS` (a bump to 32768 would cost 22.8 MB, not the 19.6 MB the comment implies). Same class of drift as #2273.
- **Related**: #2273 (stale field-count in `intern`'s collision-policy comment), #797 / SAFE-22, #807.
- **Suggested Fix**: Update the docstring to 348 B / 5.70 MB per frame / 11.4 MB total, and consider phrasing it as "`size_of::<GpuMaterial>()` × 16384" so the next struct growth doesn't re-stale it.

#### REN-D4-2026-08-07-02: `copy_depth_to_history`'s pre-copy barrier omits `DEPTH_STENCIL_ATTACHMENT_WRITE` from its source access scope
- **Severity**: LOW
- **Dimension**: Sync/Barriers (Dim 4)
- **Location**: `crates/renderer/src/vulkan/context/post_passes.rs:VulkanContext::copy_depth_to_history` (the `depth_to_src` `vk::ImageMemoryBarrier`)
- **Status**: NEW (prior audits — `AUDIT_RENDERER_2026-06-28_DIM12_DIM14.md`, `AUDIT_RENDERER_2026-07-14_DIM12_DIM14.md` — record this function as "outside any pass with paired barriers" but did not audit the access masks)
- **Description**: The barrier that moves `depth_image` from `DEPTH_STENCIL_READ_ONLY_OPTIMAL` to `TRANSFER_SRC_OPTIMAL` before the history copy declares `src_access_mask = DEPTH_STENCIL_ATTACHMENT_READ | SHADER_READ` — no `DEPTH_STENCIL_ATTACHMENT_WRITE`. The data being copied *is* the render pass's depth write. A barrier whose first access scope contains only reads performs no availability operation for that write.
- **Evidence**:
  ```rust
  let depth_to_src = vk::ImageMemoryBarrier::default()
      .src_access_mask(
          vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ | vk::AccessFlags::SHADER_READ,
      )
      .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
      .old_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)
      .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
  ```
  emitted with `src_stage = LATE_FRAGMENT_TESTS | FRAGMENT_SHADER`, `dst_stage = TRANSFER`.
- **Impact**: Almost certainly **legal today** via dependency chaining, and I want to be explicit about that rather than overstate the finding: `helpers.rs`'s `dependency_out` has `dst_stage_mask = FRAGMENT_SHADER | COMPUTE_SHADER` / `dst_access_mask = SHADER_READ`, and this barrier's first scope contains `FRAGMENT_SHADER` + `SHADER_READ`, so the two scopes intersect and the render pass's `DEPTH_STENCIL_ATTACHMENT_WRITE` availability propagates through the chain. The exposure is that the correctness of a depth read now depends on an incidental `FRAGMENT_SHADER|SHADER_READ` overlap with a dependency declared for an unrelated consumer (SSAO/SVGF/composite). Narrowing `dependency_out` — a plausible future optimisation — would silently break this copy. Symptom would be stale/garbage soft-particle depth fade, invisible to `cargo test`.
- **Related**: `helpers.rs::create_render_pass` `dependency_out`; #947 (the last change to that dependency's stage masks).
- **Suggested Fix**: **needs sync-validation verification** to confirm whether the layer accepts the chain. If a change is made at all, the minimal one is adding `DEPTH_STENCIL_ATTACHMENT_WRITE` to `depth_to_src.src_access_mask` and `EARLY_FRAGMENT_TESTS` to its `src_stage` so the barrier is self-sufficient rather than chain-dependent — a strict widening, no behavioural narrowing.

#### REN-D4-2026-08-07-03: `record_upscale_pass` consumes the *shared* depth image, extending the `MAX_FRAMES_IN_FLIGHT == 2` contract to a consumer the contract's own doc does not enumerate
- **Severity**: LOW
- **Dimension**: Sync/Barriers (Dim 4)
- **Location**: `crates/renderer/src/vulkan/context/post_passes.rs:VulkanContext::record_upscale_pass` (`depth: self.depth_image`), against `crates/renderer/src/vulkan/sync.rs` (the `const _: () = assert!(MAX_FRAMES_IN_FLIGHT == 2, ...)` doc block)
- **Status**: NEW (documentation/contract-completeness finding; the underlying hazard is #870, already mitigated)
- **Description**: `sync.rs`'s `MAX_FRAMES_IN_FLIGHT` const-assert doc enumerates the shared-depth-image consumers as "frame N's compute consumers (SSAO sampler, SVGF depth read)". FSR (`frame_upscaler.rs`, via `record_upscale_pass`) is a third consumer of the same single `self.depth_image` and is not named. The safety argument itself is unchanged and still correct — the both-slots fence wait covers all of them at `MAX_FRAMES_IN_FLIGHT == 2` — so this is not a live hazard.
- **Evidence**: `depth_image` is declared once (not a per-frame `Vec`, unlike `gbuffer.rs`'s per-FIF images); `record_upscale_pass` passes `depth: self.depth_image` into `UpscaleDispatchInputs` with no frame index.
- **Impact**: None today. The risk is that whoever next evaluates option (a) from the `sync.rs` note ("make the depth image per-frame-in-flight") sizes the work off an incomplete consumer list and misses the FSR binding, or that the enumerated list is read as exhaustive during a future `MAX_FRAMES_IN_FLIGHT` bump review.
- **Related**: #870 / REN-D4-NEW-01 (the original shared-depth finding); #282 (the both-slots wait that makes it safe).
- **Suggested Fix**: Add the FSR/`frame_upscaler` depth read to the consumer list in `sync.rs`'s `MAX_FRAMES_IN_FLIGHT` doc block. Documentation-only; no barrier or pipeline change.

#### D5-01: Half the per-frame scratch cluster is excluded from the peak-shrink policy
- **Severity**: LOW
- **Dimension**: Memory/Lifecycle (Dim 5)
- **Location**: `/mnt/data/src/gamebyro-redux/crates/renderer/src/vulkan/context/draw.rs:3169-3183` (`VulkanContext::draw_frame`, end-of-frame scratch restore)
- **Status**: NEW
- **Description**: `draw_frame` restores five per-frame scratch containers to `self`,
  but only two of them (`gpu_instances_scratch`, `batches_scratch`) get the
  `shrink_scratch_if_oversized(working_set, floor=512)` treatment. `previous_models_scratch`
  (a `Vec<GpuPreviousModel>`) is restored on the immediately preceding line and never
  shrunk, and the two rigid-history `FxHashMap`s (`previous_rigid_models` /
  `current_rigid_models_scratch`) are `mem::swap`ped without any capacity policy at all.
  All of them are `clear()`-then-`reserve(draw_commands.len())`, so their capacity is
  monotonically the session high-water mark, not the working set.
- **Evidence**:
```rust
self.gpu_instances_scratch = gpu_instances;
self.previous_models_scratch = previous_models;   // <- restored, never shrunk
self.batches_scratch = batches;
super::super::acceleration::shrink_scratch_if_oversized(
    &mut self.gpu_instances_scratch, working_instances, 512);
super::super::acceleration::shrink_scratch_if_oversized(
    &mut self.batches_scratch, working_batches, 512);
```
  and at `draw.rs:3125-3127`:
```rust
std::mem::swap(&mut self.previous_rigid_models, &mut current_rigid_models);
current_rigid_models.clear();
self.current_rigid_models_scratch = current_rigid_models;   // no shrink
```
  The struct doc at `context/mod.rs:1092-1102` describes the whole group as one
  "amortization pattern" cluster, which is why the omission reads as drift rather
  than intent — the shrink half of the policy was only wired to two members.
- **Impact**: Host RAM only — no GPU allocation, no leak, no per-frame growth. Bound is
  `MAX_INSTANCES = 0x40000` (262 144, `scene_buffer/constants.rs:135`): a single
  large-exterior peak can pin ~16 MB in `previous_models_scratch` plus ~20 MB per
  rigid-history map, and that residency survives the walk into a small interior for the
  rest of the session. It is exactly the same pressure `#243`/`#496`/`#504` shrink policy
  exists to relieve for the other two members. Not a correctness issue.
- **Related**: `#243` (scratch amortization), `#496`, `#504` (shrink policy),
  `#2174`/D2-03 (FxHashMap swap, explicitly states allocation behaviour is "already
  correct" — true for churn, but it does not address the high-water pin).
  Telemetry already surfaces all five capacities via the `ctx.scratch` command
  (`context/mod.rs:3083-3130`), so the regression is measurable today.
- **Suggested Fix**: Extend the existing `shrink_scratch_if_oversized` call block to
  `previous_models_scratch` with the same `(working_instances, 512)` arguments; for the
  two `FxHashMap`s add an equivalent `if map.capacity() > working * 2 { map.shrink_to(working.max(512)) }`
  after the swap. Purely additive, no ordering constraints.

#### D5-02: `GpuBuffer::destroy` leaves a dangling `self.buffer` handle
- **Severity**: LOW
- **Dimension**: Memory/Lifecycle (Dim 5)
- **Location**: `/mnt/data/src/gamebyro-redux/crates/renderer/src/vulkan/buffer.rs:887` (`GpuBuffer::destroy`)
- **Status**: NEW
- **Description**: `destroy()` takes `self.allocation`, destroys the `VkBuffer`, frees the
  allocation and drops the allocator `Arc` — but never nulls `self.buffer`. The struct
  stays alive with a stale, already-destroyed `vk::Buffer` in a `pub` field. Double-free
  is correctly prevented (the `allocation.take()` gate) and the `Drop` safety net at
  `buffer.rs:1043` correctly short-circuits, so the leak/double-free axes are clean.
  What is not defended is a *read*: any code that keeps the `GpuBuffer` and later reads
  `.buffer` gets a destroyed handle with no way to tell.
- **Evidence**:
```rust
pub fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
    if let Some(allocation) = self.allocation.take() {
        unsafe { device.destroy_buffer(self.buffer, None); }   // self.buffer left as-is
        allocator.lock()...free(allocation).expect(...);
    }
    self.allocator = None;
}
```
  Contrast the sibling helpers, which do null out: `destroy_depth_resources` nulls the
  view/image handles (cited in `context/mod.rs:3583-3586` as "Each handle is nulled by
  the helper so a later Drop is a no-op"), and `TextureRegistry::destroy` nulls
  `depth_history_sampler` at `context/mod.rs:3598`.
- **Impact**: Latent only. Today every call site either consumes the `GpuBuffer`
  (`Option::take` + destroy) or is in a teardown path with no subsequent read, so there
  is no live defect. The exposure is a future call site that destroys through a
  long-lived `&mut GpuBuffer` and then binds `.buffer` — a class of bug that is invisible
  to `cargo test` and only shows up as a validation-layer complaint or GPU fault.
- **Related**: `#656` (Drop safety net), `#927` (allocator `Arc` release in `destroy`) —
  both hardened this same function; nulling the handle is the remaining sibling.
- **Suggested Fix**: Add `self.buffer = vk::Buffer::null();` after the `destroy_buffer`
  call (and the matching line in the `Drop` safety-net arm), matching the
  `destroy_depth_resources` convention.

#### NIFAL-D6-2026-08-07-02: `docs/engine/nifal.md` particle slice contradicts itself and current code on `initial_radius`
- **Severity**: LOW
- **Dimension**: NIFAL Material (Dim 6)
- **Location**: `/mnt/data/src/gamebyro-redux/docs/engine/nifal.md` §2 "Particles — emitter base params converged (2026-05-28)" vs `/mnt/data/src/gamebyro-redux/byroredux/src/systems/particle.rs:apply_emitter_params`
- **Status**: NEW
- **Description**: The spec's first particle bullet states that `initial_radius`
  is deliberately **not** applied and that size stays owned by the preset. Two
  paragraphs later the *same section* states the opposite — that size is authored
  as `initial_radius × base_scale`. The code implements the second version. The
  first bullet is stale text left in place when the size work landed, and it is
  the paragraph an auditor/agent reads first (this dimension's own checklist is
  derived from this file).
- **Evidence**: Spec, first bullet: "`initial_color` (shipped as the white
  nif.xml default) and `initial_radius` (default 1.0) are **intentionally not
  applied** — colour stays owned by the `color_curve` override, size by the
  preset". Spec, later paragraph: "Particle **size** is authored too … the
  translate sets a **constant** `start_size = end_size = initial_radius ×
  base_scale`". Code (`systems/particle.rs:39-41`):
  ```rust
  let size = p.initial_radius * p.base_scale.unwrap_or(1.0);
  preset.start_size = size;
  preset.end_size = size;
  ```
  Only the `initial_color` half of the stale bullet is still true.
- **Impact**: Documentation-only, but on the authoritative NIFAL spec that
  reviewers and audit dimensions treat as the contract. A future change that
  "restores the documented invariant" by removing the size override would
  regress FNV oasis smoke back to ~7× oversized particles (the exact defect the
  `base_scale` work fixed).
- **Related**: #1434 (base_scale sanity), #1775 (radius_variation); NIFAL-D6-2026-08-07-01.
- **Suggested Fix**: Delete `initial_radius` from the "intentionally not applied"
  bullet (leave `initial_color`) and cross-reference the size paragraph below it.

#### NIFAL-D6-2026-08-07-03: `mat.set` writes canonical PBR scalars with no clamp or finite guard, bypassing the `resolve_pbr` invariant
- **Severity**: LOW
- **Dimension**: NIFAL Material (Dim 6)
- **Location**: `/mnt/data/src/gamebyro-redux/byroredux/src/commands/scene.rs:MatSetCommand::execute` (`set_scalar` arms for `metalness` / `roughness` / `ior` / `alpha`)
- **Status**: NEW
- **Description**: `Material::metalness`/`roughness` carry a documented
  engine-wide invariant — "fully resolved, clamped to the renderer ranges
  (`metalness ∈ [0,1]`, `roughness ∈ [0.04,1]`)" — which the render path relies
  on by reading them verbatim into `GpuMaterial`. `mat.set` is the only writer
  that reaches these fields after `translate_material`, and it stores the parsed
  `f32` directly:
  ```rust
  let set_scalar = |slot: &mut f32, vals: &[&str]| -> Result<String, String> {
      let v = MatSetCommand::floats(vals, 1)?;
      *slot = v[0];                      // no clamp, no is_finite check
      Ok(format!("{:.4}", v[0]))
  };
  ```
  Rust's `"NaN".parse::<f32>()` / `"inf".parse::<f32>()` both succeed, and
  `mat.set <id> roughness 0` is a plausible typo that lands below the 0.04 floor.
- **Evidence**: The sibling write path treats this as load-bearing —
  `material_translate.rs:310` returns `None` rather than let a non-finite
  glossiness reach `roughness`, with the rationale "NaN GGX terms poison the lit
  color and stick in SVGF/TAA history" (#1535). `mat.set` has no equivalent guard,
  so a NaN typed at the console produces exactly the failure #1535 was filed to
  prevent — and it persists in the temporal history buffers after the value is
  corrected. The Cornell harness (`docs`/#2249) is built around driving these
  fields live, so this is a reachable workflow, not a theoretical one.
- **Impact**: Debug-tooling only (no shipping content path), but the failure is
  sticky and easy to misattribute to the renderer rather than to the console
  input. Also affects `mat.set ... ior` for the fire-refraction overload, whose
  translate-side sibling `material_optical_scalar` *does* sanitize
  (`clamp(0,1)` + NaN → 0).
- **Related**: #1535 (the NaN-roughness guard this bypasses); #2249 / REN-D21-03
  (added the `ior` arm); #2330 / SKY-D7-03 (the other post-translate writer).
- **Suggested Fix**: Route the three PBR arms through the same clamps
  `resolve_pbr` applies (or simply call `m.resolve_pbr()` after the mutation —
  it is idempotent and already clamp-only for non-NaN input), and reject
  non-finite input with the existing `Err(String)` path so the console reports it.

#### NIFAL-D6-2026-08-07-04: Raw-material→marker-component block is copy-pasted at both spawn sites instead of living behind the boundary
- **Severity**: LOW
- **Dimension**: NIFAL Material (Dim 6)
- **Location**: `/mnt/data/src/gamebyro-redux/byroredux/src/scene/nif_loader.rs:822-847` and `/mnt/data/src/gamebyro-redux/byroredux/src/cell_loader/spawn.rs:1554-1582`
- **Status**: NEW
- **Description**: Immediately after the `translate_material` call, both sites run
  a byte-identical ~26-line block that reads **raw `ImportedMaterial`** fields
  (`is_decal`, `has_alpha`, `alpha_test`, `alpha_threshold`, `src_blend_mode`,
  `dst_blend_mode`, `two_sided`) and derives the `AlphaBlend` / `IsDecalMesh` /
  `TwoSided` components, including the implicit-decal-blend fallback and the
  hard-coded `(6, 7)` alpha-over pair. This is the same duplicated-construction
  shape `translate_material`'s own module doc describes as "itself a translation
  leak: a field added to one site and not the other silently diverged the two
  load paths" — just for the marker-component subset rather than the `Material`
  struct.
- **Evidence**: Both blocks are currently identical (same
  `decal_uses_implicit_alpha_blend` helper, same `(6, 7)` fallback, same three
  conditional inserts), so there is **no live divergence today** — this is a
  structural/latent finding, not an active bug. The shared decision *predicate*
  was already factored out (`decal_uses_implicit_alpha_blend`); the surrounding
  blend-mode selection and the three inserts were not.
- **Impact**: Latent. A future blend/decal rule added to one site silently
  diverges loose-NIF loads from cell-placed REFRs — the failure mode is
  "the same NIF renders differently depending on how it was loaded", which is
  hard to spot and has no test coverage at the two-site level.
- **Related**: `docs/engine/nifal.md` §3 "De-duplication"; #2300 (the same
  consolidation already performed for the particle slice's
  `texture_path`/`src_blend`/`dst_blend` overrides, which had the identical
  copy-paste-at-both-sites shape).
- **Suggested Fix**: Follow the #2300 precedent — add a
  `attach_blend_and_facing_markers(world, entity, &mesh.material)` helper next to
  `translate_material` and call it from both sites, so the marker derivation has
  the single declared boundary the `Material` derivation already has.

#### MAT-D7-2026-08-07-02: `hash_material_slice` docstring cites a `GpuMaterial: Hash` impl that does not exist, with stale line anchors
- **Severity**: LOW
- **Dimension**: Material Table (Dim 7)
- **Location**: `crates/renderer/src/vulkan/scene_buffer/descriptors.rs:hash_material_slice`
- **Status**: NEW
- **Description**: The doc comment says the slice hash is "routed through `GpuMaterial::as_bytes`-equivalent slice cast so the same byte view used by `GpuMaterial`'s `Hash`/`Eq` impls (`vulkan/material.rs:280-309`) drives the slice hash too". `GpuMaterial` has no `Hash` impl — dedup is keyed on the field-walking `hash_gpu_material_fields` (#781 moved the index key off the struct itself); only `PartialEq`/`Eq` use `as_bytes`. The cited line range `280-309` now lands in the supplemental-texture-role field block, not the `as_bytes`/`PartialEq` block (which sits around `material.rs:588-611`).
- **Evidence**: `material.rs` declares only `impl PartialEq for GpuMaterial { fn eq(&self, other: &Self) -> bool { self.as_bytes() == other.as_bytes() } }` and `impl Eq for GpuMaterial {}`. No `impl Hash`. `MaterialTable::index` is `FxHashMap<u64, u32>` keyed on `hash_gpu_material_fields`.
- **Impact**: Documentation only. A reader chasing "which hash does dedup use" is pointed at a non-existent impl and at unrelated line numbers, which is exactly the failure mode the two-walk lockstep contract (#781) depends on people understanding.
- **Related**: #781 / PERF-N4, #878 / DIM8-01, #1368, #2273.
- **Suggested Fix**: Reword to "the same raw-byte view `GpuMaterial::as_bytes` gives the `PartialEq`/`Eq` impls" and drop the hard-coded line numbers in favour of the symbol name.

#### MAT-D7-2026-08-07-03: Stale "75 live scalar fields" count repeated at a second site (`hash_gpu_material_fields`)
- **Severity**: LOW
- **Dimension**: Material Table (Dim 7)
- **Location**: `crates/renderer/src/vulkan/material.rs:hash_gpu_material_fields`
- **Status**: Existing: #2273 (same drift, second site)
- **Description**: #2273 tracks the stale field count in `MaterialTable::intern_by_hash`'s collision-policy comment ("rare on FxHash's 64-bit output over 75 scalar fields"). The identical stale count also heads `hash_gpu_material_fields`: "FxHash (#1368) over the 75 live scalar fields of `GpuMaterial` in declaration order". The struct is 348 B of tightly-packed 4-byte scalars = **87** fields, and the walk in that very function does write all 87. Recording here so the #2273 fix covers both sites rather than one.
- **Evidence**: `348 / 4 == 87`; the hash walk ends with the twelve `*_map_index` writes that post-date the "75" figure.
- **Impact**: Documentation only; no hash or layout defect. Both counts drift again on the next field addition.
- **Related**: #2273, #1368, #1249, #1250.
- **Suggested Fix**: Fold into #2273 — replace both literals with a reference to `gpu_material_size_is_348_bytes`, or derive the count in a test assertion instead of prose.

#### REN-D8-N02: `CompositeParams::underwater` and `depth_params.y` (exposure) are dead fields still documented as live
- **Severity**: LOW
- **Dimension**: Denoiser/Composite (Dim 8)
- **Location**: `crates/renderer/src/vulkan/composite.rs:117-127` (`CompositeParams::underwater` doc), `crates/renderer/shaders/composite.frag:51-56` (UBO field comment), `crates/renderer/src/vulkan/context/draw.rs:574-594` (`build_composite_params`, `depth_params[1] = exposure_value`)
- **Status**: NEW (doc-rot / dead plumbing)
- **Description**: `composite.frag` still declares `vec4 underwater;` in its
  UBO with a comment stating *"The shader's final branch mixes `combined`
  toward `underwater.xyz` by a depth-driven extinction when `underwater.w >
  0`."* No such branch exists in `main()` any more — the underwater post-FX
  moved to `presentation.frag` with the output-resolution frame split, which
  `reflect.rs::composite_frag_spv_matches_recompiled_branch_count` explicitly
  records ("The presentation-only underwater branch moved to
  presentation.frag"). The host-side field doc in `composite.rs` carries the
  same stale description, and `draw.rs` still uploads a live `underwater`
  value into the composite UBO *and* passes the same value to
  `record_presentation_pass`. `depth_params.y` (exposure) is the same shape:
  `build_composite_params` computes and uploads `exposure_value` with the
  comment "composite and the future FSR dispatch consume one value", but
  `composite.frag` only reads `depth_params.x` and `.w`; exposure is consumed
  by `presentation.frag`'s push constants.
- **Evidence**: `composite.frag` `main()` ends at `outColor = vec4(combined,
  direct4.a);` with no `params.underwater` reference; `grep 'depth_params'
  composite.frag` yields only `.x` (line 395) and `.w` (line 157).
  `presentation.frag:113` owns the live `params.underwater.w > 0.0` branch and
  `presentation.frag:111` the live `aces(graded * params.exposure)`.
- **Impact**: No runtime effect (16 wasted UBO bytes plus one f32). The risk
  is directional: a maintainer trusting either doc could "restore" the missing
  composite branch, producing a double underwater tint (once pre-tone-map in
  composite, once post-tone-map in presentation) or a double exposure
  multiply.
- **Related**: `reflect.rs::composite_frag_spv_matches_recompiled_branch_count`
  (#1917) is the only place that records the move correctly.
- **Suggested Fix**: Either drop `underwater` / the exposure slot from
  `CompositeParams` (and the matching GLSL fields + `build_composite_params`
  plumbing), or rewrite both doc blocks to say "reserved — the live consumer
  is `presentation.frag`". Note dropping the field changes the UBO block size,
  so the `composite_params_is_16_byte_aligned_std140_shape` test and the
  `.spv` need a coordinated recompile.

#### REN-D8-N03: `depth_params.z` volumetric-consumption gate no longer exists in the shader, but the host still documents it as the flip switch
- **Severity**: LOW
- **Dimension**: Denoiser/Composite (Dim 8)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:582-592` (`build_composite_params`, `depth_params[2]`), `crates/renderer/src/vulkan/composite.rs:54-58` (`depth_params` field doc)
- **Status**: NEW (broken contract / doc-rot)
- **Description**: The host comment reads *"Composite reads this slot to
  decide whether to consume `vol.a` (transmittance) and `vol.rgb`
  (in-scattering). Pinned to the host const so a future flip of
  `VOLUMETRIC_OUTPUT_CONSUMED` is a single-line change."* That is no longer
  true: `#1926` removed the shader-side branch, and `composite.frag:512`
  applies `combined = combined * vol.a + vol.rgb;` unconditionally
  (the removal is itself pinned by
  `reflect.rs::composite_frag_spv_matches_recompiled_branch_count`, expected
  count 16). Meanwhile `post_passes.rs:425` wraps *both* volumetric dispatches
  in `if VOLUMETRIC_OUTPUT_CONSUMED`. Flipping the const to `false` would
  therefore stop all volume writes while composite keeps multiplying the scene
  by whatever the froxel volume last held — i.e. the advertised "single-line
  change" would now be a two-file change, and its safety rests entirely on the
  implicit `volumetrics.rs::initialize_layouts` neutral clear
  (`float32: [0.0, 0.0, 0.0, 1.0]`), which nothing documents as load-bearing
  for that path.
- **Evidence**: `composite.frag:563-573` documents the removal of the fallback
  branch ("`depth_params.z < 0.5` guard can never pass. Removed per the
  lockstep note this branch used to carry (#1926 / REN-D8-01)") while the host
  comment at `draw.rs:582` still advertises the gate.
  `volumetrics.rs:1297` is the clear that silently rescues the flip.
- **Impact**: None today (`VOLUMETRIC_OUTPUT_CONSUMED = true`). Latent trap for
  anyone bisecting a lighting regression by flipping the const, and the
  `gpu_timers.rs:166` doc still claims `false` is "the current default", so
  the misinformation is already spreading across three files.
- **Related**: #928, #1013, #1926, REN-D8-01 (`AUDIT_RENDERER_2026-07-14`).
- **Suggested Fix**: Rewrite the `draw.rs` / `composite.rs` comments to say
  the slot is vestigial and that the const's off-path relies on the neutral
  froxel clear; add that note to `volumetrics.rs::initialize_layouts` so the
  clear value is not "optimized" to a plain zero.

#### REN-D9-NEW-02: `pending_skin_unload_victims` drain and the `SkinSlot` LRU sweep are gated behind the global vertex SSBO being present
- **Severity**: LOW
- **Dimension**: Skinning (Dim 9)
- **Location**: `crates/renderer/src/vulkan/context/skinned_blas_refit.rs:84` (guard) enclosing `:610-649` (cleanup)
- **Status**: NEW
- **Description**: The cell-unload victim drain (#1003) and the idle-slot LRU sweep (#643 / MEM-2-1) both live inside `if let (Some((input_buffer, input_size)), Some(bone_buf)) = (global_vert_buf, bone_buffer)`. `global_vert_buf` is `self.mesh_registry.global_vertex_buffer`, which `MeshRegistry` legitimately leaves as `None` — it is `take()`n during a geometry-SSBO rebuild (`crates/renderer/src/mesh.rs:886`, `:893`) and is `None` before the first upload. On any frame where the global vertex buffer is absent, no `SkinSlot` is destroyed and no skinned BLAS is dropped, even for entities the cell loader has already despawned.
- **Evidence**: cleanup block `skinned_blas_refit.rs:608-649` is nested three levels inside the `Some(global_vert_buf)` guard opened at `:84`; the only other consumer of `pending_skin_unload_victims` is `byroredux/src/cell_loader/unload.rs:207` (producer).
- **Impact**: Bounded, not unbounded — the next frame with a live global vertex buffer drains the backlog. Worst case is that GPU memory for freed actors' output buffers + BLAS, and their `FREE_DESCRIPTOR_SET` pool slots, stay held across a cell transition window. Matters most on the exact frames where memory headroom is tightest (the low-headroom `device_wait_idle` rebuild path at `mesh.rs:882`).
- **Related**: #1003 (drain), #643 / MEM-2-1 (LRU sweep), #900 (`failed_skin_slots` un-suppression, which also only fires from inside this block).
- **Suggested Fix**: Hoist the eviction/drain block out of the `(global_vert_buf, bone_buffer)` guard — it needs only `skin_pipeline`, `accel` and `alloc`, all of which are already in scope from the outer `if let`.

#### REN-D9-NEW-03: Stale palette-bound number in the `skin_vertices.comp` clamp rationale
- **Severity**: LOW
- **Dimension**: Skinning (Dim 9)
- **Location**: `crates/renderer/shaders/skin_vertices.comp:141`
- **Status**: NEW
- **Description**: The #651 / SH-6 clamp comment says an unclamped index "would read past `bone_offset + 127` into the adjacent mesh's palette". `MAX_BONES_PER_MESH` was raised to 144 (#1135), so the real boundary is `bone_offset + 143`. The code itself is correct (`min(boneIdx, uvec4(MAX_BONES_PER_MESH - 1u))`); only the prose is stale.
- **Evidence**: `crates/renderer/src/shader_constants_data.rs:64` → `pub const MAX_BONES_PER_MESH: u32 = 144;`, matching `crates/core/src/ecs/components/skinned_mesh.rs:52`.
- **Impact**: Documentation only. Flagged because this is a stride/bound comment on a safety clamp, and the M29 failure modes in this dimension are all "two sites drifted and nothing observed it" — a wrong number here is exactly the kind of thing a future reader would trust.
- **Related**: #651 / SH-6, #1135.
- **Suggested Fix**: Change `127` to `MAX_BONES_PER_MESH - 1` (avoid re-baking a literal).

#### REN-D10-NEW-01: #2240's `freqScale` multiplies water's **absolute** textured wave UV, amplifying the one un-rebased large-world consumer
- **Severity**: LOW
- **Dimension**: Camera-Relative Precision (Dim 10)
- **Location**: `/mnt/data/src/gamebyro-redux/crates/renderer/shaders/water.frag:221` (`sampleScrollingNormal`, textured branch); scale sourced at `:415`
- **Status**: NEW (introduced by `6d40f6bf` / #2240, landed 2026-08-05, i.e. after the 2026-08-03 audit)
- **Description**: `sampleScrollingNormal` has two branches. The procedural branch was fixed under
  #1997 to rebase its hash input origin-relative:
  ```glsl
  vec2 uv = (uvBase - originOffset) * scale + scroll * time;   // :210 — relative, correct
  ```
  The textured branch deliberately stays **absolute** so the wrapping sampler has no seam at a
  render-origin crossing:
  ```glsl
  vec2 uv = uvBase * scale * freqScale + scroll * time;        // :221 — absolute
  ```
  #2240 inserted `freqScale = push.misc.y / 0.6` (`:415`, WATR-authored `wave_frequency`, **unclamped**)
  into that product. `uvBase` is `vWorldPos.xz`, up to ~176 k on MarkarthWorld. With the default
  `uv_scale_a = 1/256` and the *default* `wave_frequency = 0.6` (`freqScale == 1.0`) nothing changes
  from the pre-#2240 magnitude (~687, f32 ULP ≈ 6.1e-5 ≈ 1/16 texel on a 1024² normal map). But any
  WATR authoring `wave_frequency > 0.6` scales the UV magnitude — and therefore the quantization
  step — proportionally, with no upper bound. At `freqScale ≈ 3.3` the ULP reaches ~1/4 texel.
- **Evidence**: `:415` `float freqScale = push.misc.y / 0.6;` (no clamp) feeding `:221`. The
  companion in-code precision comment at `:183-193` documents the hazard for the procedural branch
  only and explicitly says the textured branch keeps its "absolute (wrapping) UV".
- **Impact**: Visual only, and only for textured water (Skyrim/FO4 WATR with a bound normal map) in
  a worldspace far from the origin *and* with an authored `wave_frequency` above the 0.6 default —
  the wave normal map stair-steps/aliases instead of resolving smoothly. Invisible near the origin
  and unreachable from `cargo test`; needs a large-world capture to confirm the practical magnitude.
  I did **not** verify what vanilla Skyrim actually authors for `wave_frequency`, so the real-content
  blast radius is unconfirmed — reporting the mechanism, not a claimed observed artifact.
- **Related**: #1997 (procedural-branch rebase), #2240 / `6d40f6bf` (the `freqScale` addition),
  #1502 (original water precision bound).
- **Suggested Fix**: Subtract the *tile-integral* part of the origin so the wrapping sampler is
  unaffected but the magnitude collapses: `vec2 o = floor(originOffset * scale * freqScale);
  vec2 uv = uvBase * scale * freqScale - o + scroll * time;`. Separately consider clamping
  `freqScale` to a sane authored range at the CPU packing site (`byroredux/src/render/water.rs:107`).

#### REN-D10-NEW-02: `caustic_splat.comp` is the one `CameraUBO` re-declarer still missing #2164's `renderOrigin.w` payload note
- **Severity**: LOW
- **Dimension**: Camera-Relative Precision (Dim 10)
- **Location**: `/mnt/data/src/gamebyro-redux/crates/renderer/shaders/caustic_splat.comp:76`
- **Status**: NEW (incomplete application of the fix for prior finding L-10 / #2164)
- **Description**: #2164 fixed the "w unused" documentation trap at `draw.rs`, `water.vert:83` and
  `cluster_cull.comp:69` — all three now read "w = FSR one-frame-reset flag (NOT padding —
  #2164/L-10)". The fourth `CameraUBO` re-declarer, `caustic_splat.comp`, was missed and still reads:
  ```glsl
  vec4 renderOrigin;   // #markarth-precision — camera-relative render origin (added to inv_view_proj world reconstruction below). Keeps CameraUBO == sizeof(GpuCamera).
  ```
  — no mention of `w` at all, and the trailing "Keeps CameraUBO == sizeof(GpuCamera)" reads as
  "this field is here for padding parity", which is exactly the reading #2164 set out to eliminate.
- **Evidence**: `grep -n "w unused" water.vert cluster_cull.comp draw.rs` → 0 hits (fixed);
  `caustic_splat.comp:76` carries neither the corrected wording nor a `w` description.
- **Impact**: Documentation only. Same latent trap class as the tracked
  `VolumetricsParams::render_origin.w` overload (#1928): a future author reading only this site could
  repurpose `w` and silently break the FSR reset-flag contract that `triangle.frag:582`
  (`clamp(renderOrigin.w, 0.0, 1.0)`) depends on.
- **Related**: prior L-10 / #2164; #1928 / REN-D10-01.
- **Suggested Fix**: Copy `cluster_cull.comp:69`'s wording verbatim into `caustic_splat.comp:76`.

#### REN-D11-2026-08-07-01: `find_depth_format` error message names candidates that were removed
- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass (Dim 11)
- **Location**: `crates/renderer/src/vulkan/context/helpers.rs:45` (`find_depth_format`)
- **Status**: NEW (follow-on drift from the REN-D4-NEW-02 fix, audit 2026-05-11 DIM4)
- **Description**: The candidate list was narrowed to pure-depth formats to
  fix the packed depth-stencil aspect/layout foot-gun, but the `bail!`
  diagnostic still advertises the two removed packed formats.
- **Evidence**:
  ```rust
  let candidates = [vk::Format::D32_SFLOAT, vk::Format::D16_UNORM];
  ...
  anyhow::bail!("No supported depth format found (tried D32, D32S8, D24S8, D16)")
  ```
- **Impact**: On the (very unlikely) device where both candidates fail, the
  error blames the engine for having tried packed formats it never tried,
  sending the reader looking for a nonexistent fallback path. Diagnostic-only.
- **Related**: REN-D4-NEW-02 (`docs/audits/AUDIT_RENDERER_2026-05-11_DIM4.md`)
- **Suggested Fix**: Change the message to `(tried D32_SFLOAT, D16_UNORM)`,
  or build it from `candidates` so it can't drift again.

#### REN-D11-2026-08-07-02: `GBufferFormats` doc says "seven attachment formats … six G-buffer color targets"
- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass (Dim 11)
- **Location**: `crates/renderer/src/vulkan/context/helpers.rs:48-50` (`GBufferFormats` doc comment)
- **Status**: NEW
- **Description**: The struct doc predates the two FSR mask attachments
  (`5c56e311`/`5c7acfe2`). The struct itself has 8 fields and describes 9
  render-pass attachments (8 colour + depth); the `fsr_mask_format` field is
  reused for attachments 6 and 7.
- **Evidence**:
  ```rust
  /// The seven attachment formats the main render pass writes — the six
  /// G-buffer color targets plus depth. Groups the formats that travel
  /// together into [`create_render_pass`].
  pub(super) struct GBufferFormats { /* 8 fields, incl. fsr_mask_format */ }
  ```
  Contrast with the accurate inline table further down (`helpers.rs:86-122`)
  and `log::info!("Render pass created (8 color + depth)")` at line 278.
- **Impact**: Someone adding a ninth colour attachment reads "seven" and
  mis-sizes one of the four per-pipeline blend arrays. That failure mode is
  a pipeline-creation error (VUID-…-renderPass-07609), so it's loud, but the
  doc is the first thing a new attachment author reads.
- **Related**: REN-D11-2026-08-07-03 (same drift, sibling function)
- **Suggested Fix**: "The nine attachments the main render pass writes —
  eight G-buffer color targets (the two FSR masks share one format) plus depth."

#### REN-D11-2026-08-07-03: `create_main_framebuffers` doc omits the two FSR mask attachments
- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass (Dim 11)
- **Location**: `crates/renderer/src/vulkan/context/helpers.rs:282-287` (`create_main_framebuffers` doc comment)
- **Status**: NEW
- **Description**: The doc enumerates the bound views but stops at `albedo`,
  even though the function binds 9 views and `GBufferViews` carries
  `reactive_views` + `transparency_views` (correctly documented as
  attachments 6 and 7 on the struct fields themselves).
- **Evidence**:
  ```
  /// Create one main framebuffer per frame-in-flight slot. Each framebuffer
  /// binds that slot's HDR + normal + motion + mesh_id + raw_indirect +
  /// albedo views, plus the shared depth view.
  ```
  vs. the actual `attachments` array at `helpers.rs:336-346`, which has 9
  entries including `reactive_views[i]` and `transparency_views[i]`.
- **Impact**: Doc-only. The `debug_assert_eq!` length checks below do cover
  all seven colour slices, so the code is self-guarding.
- **Related**: REN-D11-2026-08-07-02
- **Suggested Fix**: Append "+ the two FSR masks" to the enumeration.

#### REN-D11-2026-08-07-04: Water blend-state comment names a removed attachment and the wrong index range
- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass (Dim 11)
- **Location**: `crates/renderer/src/vulkan/water.rs:624-628` (`build_pipeline`)
- **Status**: NEW
- **Description**: The comment above the water colour-blend array says
  "Attachments 1..6 are write-masked off" and lists six names ending in
  "reservoir". Post-#1583 there is no reservoir attachment; the masked-off
  range is 1..=5 and attachments 6/7 are *not* masked off — they are the two
  FSR masks, which water writes at full strength with MAX blending (correctly
  implemented ~20 lines below, and correctly documented there).
- **Evidence**:
  ```rust
  // Attachments 1..6 are write-masked off: water never updates
  // the G-buffer (normal / motion / mesh_id / raw_indirect /
  // albedo / reservoir) so SVGF and motion-vector reprojection see
  // only the opaque pass behind the water.
  ```
  followed by `let attachments = [hdr_blend, masked_off ×5, fsr_mask_max ×2];`
- **Impact**: Doc-only, but actively misleading: it asserts water writes no
  FSR mask when water is described elsewhere in the same function as "the
  canonical transparency-and-composition case". A reader debugging FSR ghosting
  on water would be sent the wrong way.
- **Related**: The accurate sibling comment at `water.rs:641-644`; the stale
  reservoir reference is the same class as the already-corrected note at
  `water.rs:660` ("the reservoir attachment was removed under #1583").
- **Suggested Fix**: "Attachments 1..=5 are write-masked off (normal / motion /
  mesh_id / raw_indirect / albedo); 6 and 7 (the FSR masks) are written — see
  below."

#### REN-D11-2026-08-07-05: G-buffer colour formats are never format-feature-queried, unlike depth
- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass (Dim 11)
- **Location**: `crates/renderer/src/vulkan/gbuffer.rs:39-72` (format consts) +
  `crates/renderer/src/vulkan/context/helpers.rs:22` (`find_depth_format` — the
  only `get_physical_device_format_properties` call in the crate)
- **Status**: NEW
- **Description**: The depth format is chosen by querying
  `optimal_tiling_features` for `DEPTH_STENCIL_ATTACHMENT`. Every colour
  attachment format is a hard-coded const with no capability query and no
  fallback. Most are fine — `R16G16_SFLOAT`, `R32_UINT`, `R8_UNORM`,
  `B10G11R11_UFLOAT_PACK32` and `R16G16B16A16_SFLOAT` all carry mandatory
  `COLOR_ATTACHMENT` (and, where the pipelines blend them,
  `COLOR_ATTACHMENT_BLEND`) in the Vulkan mandatory-format table. The
  exception is `NORMAL_FORMAT = R16G16_SNORM`: 16-bit SNORM formats are
  mandatory only for `SAMPLED_IMAGE` / `SAMPLED_IMAGE_FILTER_LINEAR` /
  `BLIT_SRC` / `VERTEX_BUFFER`, **not** for `COLOR_ATTACHMENT`.
- **Evidence**: `grep -rn "get_physical_device_format_properties" crates/renderer/src/`
  returns exactly one hit (`helpers.rs:33`, inside `find_depth_format`).
  `gbuffer.rs::Attachment::allocate` creates the normal image with
  `COLOR_ATTACHMENT | SAMPLED` unconditionally.
- **Impact**: On a conformant device that does not expose `COLOR_ATTACHMENT`
  for `R16G16_SNORM`, `create_image` fails with
  `VK_ERROR_FORMAT_NOT_SUPPORTED` during `GBuffer::new` and the engine
  refuses to start with a generic "Failed to create gb_normal image". Loud,
  not silent — and no desktop driver in the target hardware class (RTX 4070 Ti
  dev GPU, and AMD/Intel desktop) actually lacks it. This is a portability /
  diagnostics gap, not a live defect.
- **Related**: #275 (introduced octahedral RG16_SNORM normals);
  REN-D4-NEW-02 (`AUDIT_RENDERER_2026-05-11_DIM4.md`) applied the same
  "query before you commit to a format" reasoning to depth only.
- **Suggested Fix**: Add a one-shot startup check that asserts
  `COLOR_ATTACHMENT` in `optimal_tiling_features` for each G-buffer colour
  format (plus `COLOR_ATTACHMENT_BLEND` for the four the blend/water pipelines
  blend), failing with a format-naming error. A real fallback format for
  normals is not worth it; a precise error message is.

#### D12-2026-08-07-01: `record_post_passes` returns a `Result` that can never be `Err` — the caller's recovery branch is dead code that contradicts the #2146 invariant
- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass (Dim 12 — Command-buffer recording)
- **Location**: `crates/renderer/src/vulkan/context/post_passes.rs:record_post_passes` (sig at :168, body :194-223); caller `crates/renderer/src/vulkan/context/draw.rs:2914-2943`
- **Status**: NEW (pre-existing; predates the #2258 split — verified `7bb517b2^` was equally infallible)
- **Description**: `record_post_passes` calls eight `record_*_pass` helpers, all of which return `()`, then ends with an unconditional `Ok(())`. It is structurally incapable of returning `Err`. The caller nevertheless wraps it in a 30-line `if let Err(e) = … { recreate_image_available_for_frame(); return Err(e); }` recovery block. That block is unreachable today — but it is exactly the escape hatch `#2146` warns must not exist. `record_upscale_pass`'s own doc says: *"`record` is infallible on purpose. It runs after `svgf.dispatch`/`taa.dispatch` have latched `dispatched_this_frame`, so an error escaping to `draw_frame` would skip `queue_submit` *and* `mark_frame_completed`, leaving those latches set for a dispatch that never reached the GPU."* Keeping the fallible signature means a contributor who adds a single `?` inside any of the eight new helpers silently activates that hazard with no compile-time or test signal.
- **Evidence**:
  ```rust
  // post_passes.rs:194-223 — no `?`, no fallible call
  self.record_svgf_pass(cmd, frame);
  … self.record_presentation_pass(cmd, frame, img, underwater, image_space_modifier);
  Ok(())
  ```
  vs `draw.rs:2914` `if let Err(e) = self.record_post_passes(…) { … return Err(e); }`
- **Impact**: No runtime effect today. Latent: a future fallible call between the SVGF/TAA `dispatched_this_frame` latch and `queue_submit` would bail the frame with the latches set, so `mark_frame_completed` never runs and the next frame assumes temporal history the GPU never wrote (ghosting / stale-history artifacts) — the precise failure #2146 documented. Blast radius: the whole post chain.
- **Related**: #2146 (`FrameUpscaler::record` infallibility contract), #2258 (`7bb517b2` per-pass split), #917 / REN-D10-NEW-03 (`mark_frame_completed` moved to post-submit)
- **Suggested Fix**: Change `record_post_passes` to return `()` and delete the caller's recovery branch, so any future fallible call is a compile error at the point of introduction rather than a silent semantic change; carry the #2146 rationale onto the new signature as a doc comment.

#### D12-2026-08-07-02: `upload_indirect_draws` failure is warn-swallowed, but the draw loop still executes `cmd_draw_indexed_indirect` over the un-updated buffer
- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass (Dim 12 — Command-buffer recording)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:2672-2685` (upload site) → `crates/renderer/src/vulkan/context/geometry_pass.rs:408-439` (`cmd_draw_indexed_indirect`)
- **Status**: NEW
- **Description**: The indirect-command upload uses the same `unwrap_or_else(|e| log::warn!(…))` soft-fail pattern as the neighbouring `upload_instances` / `upload_materials` / `upload_previous_models` calls. For data SSBOs that is correct — a stale or zero buffer only misrenders. For the **indirect** buffer it is qualitatively different: `index_count` / `first_index` / `vertex_offset` / `first_instance` are *fetched and executed by the GPU*. On a failed upload the draw loop still issues `cmd_draw_indexed_indirect(indirect_buffer, i*stride, group_size, stride)` sized from **this** frame's `batches`, reading commands that belong to a previous frame's global-geometry layout (or, on the first use of a FIF slot, never-written host-visible memory). `upload_indirect_draws` correctly declines to stamp `last_uploaded_indirect_hash` on failure (`upload.rs:747-750`), so the *next* frame re-uploads — but the current frame has already recorded the draw.
- **Evidence**:
  ```rust
  // draw.rs:2682
  self.scene_buffers
      .upload_indirect_draws(&self.device, frame, indirect_scratch)
      .unwrap_or_else(|e| log::warn!("Failed to upload indirect draws: {e}"));
  ```
  no flag is set, and `geometry_pass.rs:428` unconditionally issues the indirect draw whenever `use_indirect` (`global_bound && multi_draw_indirect_supported`).
- **Impact**: Requires `mapped_slice_mut()` or `flush_range()` to fail (rare — host-visible, persistently mapped). When it does: stale `first_index`/`vertex_offset` after a `rebuild_geometry_ssbo` shrink is an out-of-range index fetch; uninitialised memory on a slot's first frame yields arbitrary `index_count`/`instance_count`. Both are GPU page-fault / TDR class, i.e. the failure mode is much louder than the warn suggests.
- **Related**: #309 (indirect path), #1809 (`upload_indirect_draws` dirty gate), #1587 (partial flush), #2215 (open indirect-grouping regression)
- **Suggested Fix**: Have the upload set a per-frame `indirect_upload_ok` flag (or return the `Result` up to `draw_frame`) and force the direct-draw fallback for that frame when it is false — `dispatch_direct` already handles every batch correctly.

#### D12-2026-08-07-03: Four `expect()` panics recorded inside the open render pass (water + UI vertex/index buffers) violate the #956 no-panic-while-recording rule
- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass (Dim 12 — Command-buffer recording)
- **Location**: `crates/renderer/src/vulkan/context/geometry_pass.rs:529-536` (water vb/ib) and `:620-627` (UI vb/ib)
- **Status**: NEW
- **Description**: Between `cmd_begin_render_pass` (`:36`) and `cmd_end_render_pass` (`:637`), the water and UI draw paths use `.expect("water mesh requires a per-mesh vertex buffer")` / `"UI mesh requires a per-mesh vertex buffer"` on `mesh.vertex_buffer` / `mesh.index_buffer`. `#956` / REN-D5-NEW-05 established the opposite house rule for this exact region (a `debug_assert!` was removed from the instance-overflow site because "it leaks the in-flight cmd buffer on unwind"), and the sibling `dispatch_direct` closure **twenty lines above** handles the identical `None` case gracefully: *"A global-only scene mesh (distant terrain LOD, #1370) carries no per-mesh buffers — skip it"*.
- **Evidence**: `mesh.rs::upload_scene_mesh_global_only` (`:526-544`) produces `vertex_buffer: None`. Its callers today are LOD-only (`placement_lod.rs:485`, `object_lod.rs:295`, `terrain_lod.rs:657`, `terrain_lod_btr.rs:202`), so water/UI meshes always take the per-mesh path and the panic is unreachable **right now**. The precondition is a call-site convention, not a type-level guarantee.
- **Impact**: If any future path ever registers a water plane or UI quad global-only (e.g. a WATAL water-LOD tier), the panic unwinds with `cmd` mid-render-pass, `image_available[frame]` signal-pending, and `images_in_flight[img]` already pointing at this frame's fence — the leak class the six explicit `recreate_image_available_for_frame` recovery sites in `draw_frame` exist to prevent.
- **Related**: #956 / REN-D5-NEW-05, #1370 (global-only meshes), #910 / REN-D5-NEW-01, #1188 / REN-D1-NEW-05
- **Suggested Fix**: Replace the four `expect()`s with `let … else { continue; }` (water loop) / `else { /* skip overlay */ }` (UI block), mirroring `dispatch_direct`'s existing graceful skip, plus a one-line `log::warn!` gated by a `Once`.

#### REN-D14-2026-08-07-02: EMA decay pass still floors while the deposit stochastically rounds (#2239 half-fix)
- **Severity**: LOW
- **Dimension**: Caustics (Dim 14)
- **Location**: `crates/renderer/shaders/caustic_splat.comp`, the `pc.decayOnly == 1u` block
- **Status**: NEW (residual of the fix for #2239)
- **Description**: #2239 identified that the parked-camera EMA drove dim caustics to zero
  because the per-tap deposit truncated sub-ULP values every frame, and fixed it by
  stochastically rounding the deposit. The *paired* operation — the decay pass — was not
  changed and still truncates: `uint(float(v) * pc.decayFactor)` discards a mean 0.5
  fixed-point ULP per texel per frame. That is a constant additive drain, so the EMA's
  steady state is `A* = (D - 0.5) / (1 - decay)` instead of `D / (1 - decay)`, i.e. short by
  `0.5 / (1 - decay)` fixed-point units. At the `CAUSTIC_DECAY_MAX = 0.995` cap that is
  100 units ≈ `100 / 65536 = 0.0015` luminance; any pool texel whose true per-frame deposit
  is below 0.5 ULP still collapses to exactly zero no matter how many frames pass, which
  reproduces the #2239 symptom on the decay side.
- **Evidence**:
  ```glsl
  if (pc.decayOnly == 1u) {
      uint v = imageLoad(causticAccum, pixel).r;
      imageStore(causticAccum, pixel, uvec4(uint(float(v) * pc.decayFactor), 0u, 0u, 0u));
      return;
  }
  ```
  contrasted with the deposit path, which *does* dither:
  ```glsl
  if (pc.decayFactor > 0.0) {
      float fracPart = depositF - float(fv);
      ...
      if (fracPart > ditherThreshold) { fv += 1u; }
  }
  ```
- **Impact**: Bounded erosion of the dim outskirts of a parked-camera caustic pool
  (hard-edged, slightly-too-small pool; sub-0.0015-luminance caustics vanish entirely).
  Much smaller than the pre-#2239 unbounded collapse, and only while parked.
- **Related**: #2239, commit `4279c195`; `AUDIT_RENDERER_2026-08-02.md` REN-D14-02.
- **Suggested Fix**: Apply the same PCG-hash stochastic rounding to the decay `imageStore`
  (round `v * decayFactor` up when its fraction exceeds a per-(texel, frame) threshold), so
  the multiply is unbiased in expectation like the deposit now is.

#### REN-D14-2026-08-07-03: `parked_frames` is global but the accumulators are per-frame-in-flight
- **Severity**: LOW
- **Dimension**: Caustics (Dim 14)
- **Location**: `crates/renderer/src/vulkan/caustic.rs::CausticPipeline::{parked_frames, dispatch}`
- **Status**: NEW
- **Description**: `parked_frames` is a single counter incremented once per `dispatch` call,
  but the accumulator it drives (`slots[frame].image`) is **per frame-in-flight** — each slot
  only receives every `MAX_FRAMES_IN_FLIGHT`-th deposit. The progressive-average weight
  `decay = N/(N+1)` is therefore computed from a sample count `N` that is
  `MAX_FRAMES_IN_FLIGHT`× larger than the number of samples that slot has actually
  accumulated. The recursion still converges to the correct expectation (each slot's own
  `decay*A + (1-decay)*x` chain is internally consistent), so this is a convergence-*rate*
  and early-frames-variance defect, not an energy error: the pool converges at roughly half
  the advertised rate and the two slots hold independent, differently-noisy estimates that
  alternate on screen, which can read as a 2-frame shimmer in the first ~30 parked frames.
- **Evidence**: `self.parked_frames = self.parked_frames.saturating_add(1);` is unconditional
  on `frame`, while `let slot_img = self.slots[frame].image;` and
  `self.descriptor_sets[frame]` are per-FIF. The in-code comment claims the counter tracks
  "Consecutive parked (camera-static) frames, for progressive 1/N EMA convergence" without
  noting the per-slot divide.
- **Impact**: Cosmetic; slower convergence and brief slot-to-slot flicker right after the
  camera parks. No stale-data or synchronization hazard (each slot is fenced independently).
- **Related**: REN-D14-2026-08-07-02 (the other EMA-accuracy residual).
- **Suggested Fix**: Make it `parked_frames: [u32; MAX_FRAMES_IN_FLIGHT]` indexed by `frame`
  (reset all entries on motion), or divide by `MAX_FRAMES_IN_FLIGHT` when forming `n`.

#### REN-D14-2026-08-07-04: Skipped caustic dispatch leaves a frozen pool that composite keeps adding
- **Severity**: LOW
- **Dimension**: Caustics (Dim 14)
- **Location**: `crates/renderer/src/vulkan/context/post_passes.rs::record_caustic_splat_pass`
- **Status**: NEW
- **Description**: Both skip paths — the `caustic_failed` permanent latch and
  `tlas_handle(frame) == None` — bypass the *entire* body, including the
  `cmd_clear_color_image`. The accumulator retains its last contents in `GENERAL`, and
  `composite.frag` samples `causticTex` unconditionally (no RT/validity gate) and adds
  `albedo * causticLum` to `combined` on every subsequent frame. Because the accumulator is
  screen-space, the frozen pool does not track camera motion — it paints a fixed pattern over
  the whole scene until a resize recreates the slots. The doc comment's "at worst one stale
  caustic frame hangs around until resize" understates this: it is re-composited every frame,
  not once.
- **Evidence**: everything, including the clear, is nested inside the guard:
  ```rust
  if !self.caustic_failed {
      if let Some(ref mut caustic) = self.caustic {
          let tlas_handle = self.accel_manager.as_ref().and_then(|a| a.tlas_handle(frame));
          if let Some(tlas) = tlas_handle {
              caustic.write_tlas(...);
              let caustic_result = caustic.dispatch(&self.device, cmd, frame, camera_static);
              ...
  ```
  and `composite.frag` has no gate:
  ```glsl
  uint causticRaw = texelFetch(causticTex, causticPixel, 0).r;
  ...
  combined = direct + indirect * albedo + caustic;
  ```
- **Impact**: Requires the TLAS to go `Some` → `None` (an `ensure_tlas_state` build/allocation
  failure after a successful build) or a `dispatch` `Err` (a `write_mapped` failure on a
  persistently-mapped host-visible UBO) — both rare — so the probability is low, but the
  consequence is a permanently-visible screen-locked artifact rather than a graceful
  degradation to "no caustics".
- **Related**: #479 (the SVGF-shaped permanent-failure latch this mirrors).
- **Suggested Fix**: On either skip path, record a one-shot `cmd_clear_color_image` on
  `slots[frame]` (with the existing GENERAL→GENERAL pre/post barriers) so the feature fails
  to *black* rather than to *frozen*; a `caustic_cleared_on_skip: [bool; MAX_FRAMES_IN_FLIGHT]`
  latch keeps it to one clear per slot.

#### REN-D15-NEW-03: Composite binding 8 falls back to the *glass* caustic view when `WaterCausticAccum` is absent, double-counting glass caustics — and the code comment asserts the opposite
- **Severity**: LOW
- **Dimension**: Water (Dim 15)
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:2596-2603` (init `water_caustic_views` fallback); `crates/renderer/src/vulkan/context/resize.rs:852-857` (resize fallback); consumed at `crates/renderer/shaders/composite.frag:439-447`
- **Status**: NEW
- **Description**: Both the init and resize paths bind composite's binding 8 (`waterCausticTex`) to `caustic_views` — the **glass/MultiLayerParallax** accumulator's sampled views, i.e. the exact same images already bound at binding 5 (`causticTex`) — whenever `water_caustic_accum` is `None`. `composite.frag` then sums the two:
  ```glsl
  uint causticRaw      = texelFetch(causticTex, causticPixel, 0).r;
  uint waterCausticRaw = texelFetch(waterCausticTex, causticPixel, 0).r;
  float causticLum = (float(causticRaw) + float(waterCausticRaw)) / CAUSTIC_FIXED_SCALE;
  ```
  With both bindings aliasing one image that is `2 × glass caustic luminance`, not `glass + 0`. The init-site comment justifying the fallback is factually wrong:
  ```rust
  // None on init failure → use the existing causticAccum views
  // as a degenerate fallback so binding 8 has a valid resource.
  // This is safe: water.frag's writes go to a NEVER-bound image
  // when the accumulator failed init, so composite at binding 8
  // reads the same all-zero causticAccum (which is correct for
  // "no water caustics this session").
  ```
  `causticAccum` is not all-zero — `caustic_splat.comp` writes it every frame for glass/MLP refractors. The premise ("all-zero") only holds when the glass caustic pipeline is *also* absent, in which case `caustic_views` is `mesh_id_views_seed` (a different, unrelated aliasing hazard already documented at `mod.rs:2511-2517`). Note the codebase already has the right resource for this — `placeholder_caustic_sink`, used by the resize path at `resize.rs:704-707` for the *write* side (WaterPipeline set 2) — but composite's read side never uses it.
- **Evidence**: `mod.rs:2598-2602` and `resize.rs:852-857` both `=> caustic_views.clone()`; `mod.rs:2518-2521` shows `caustic_views` = the live glass caustic sampled views when `caustic.is_some()`.
- **Impact**: Degraded-path only — fires when `WaterCausticAccum::new` fails at init or `recreate_on_resize`/`initialize_layouts` fails at resize (VRAM pressure / OOM). Result is 2× glass caustic brightness for the rest of the session, partially masked by the `CAUSTIC_FIREFLY_MAX = 16.0` clamp. No crash, no validation error, no memory hazard. Its main cost is that the false "this is safe" comment will defeat the next reviewer who checks this path.
- **Related**: #2142 / RL-D6-02 (the sibling bug on the *write* side of the same fallback, already fixed with `placeholder_caustic_sink`); #1257 / #1210 Phase E.
- **Suggested Fix**: Bind binding 8 to a genuinely zero-valued R32_UINT image on the fallback path (a full-render-resolution sibling of `placeholder_caustic_sink`, since `composite.frag` `texelFetch`es at `textureSize(causticTex, 0)` coordinates and a 1×1 sink would be out of range), and correct the comment. Minimum viable alternative: keep the aliasing but gate the sum in the shader on a "water caustics enabled" flag bit.

#### REN-D16-2026-08-07-02: Per-froxel shadow-ray budget is up to 10 rays, not the documented "single ray"
- **Severity**: LOW
- **Dimension**: Volumetrics (Dim 16)
- **Location**: `/mnt/data/src/gamebyro-redux/crates/renderer/shaders/volumetrics_inject.comp:main`
- **Status**: NEW (documentation/cost drift, not a correctness bug)
- **Description**: The design contract (and the file's own header comment, "shadow visibility is the standard 'trace toward light, miss = lit'") describes one `TerminateOnFirstHit` ray per froxel. Current code casts: 1 opaque sun ray, +1 glass-mask sun ray for interiors, and then up to `MAX_FROXEL_LIGHTS = 4` local lights × up to 2 rays each (opaque mask, then glass mask) = **up to 10 ray-query traversals per froxel**. At the default grid (`1920/12 × 1080/12 × 64` = 160×90×64 = 921,600 froxels) that is a worst case near 9.2 M ray queries per frame from the injection pass alone.
- **Evidence**: `volumetrics_inject.comp:503-519` (sun opaque + interior glass), `:582-601` (`needsVisibility` opaque `traceShadowBinary` then `shadowPolicyUsesGlass` glass `traceShadowBinary`), `:552` `const uint MAX_FROXEL_LIGHTS = 4u`.
- **Impact**: No visual defect; a GPU-cost cliff in dense-light interiors that the checklist/design docs do not budget for. Also means any future "cost of volumetrics" estimate derived from the docs is off by ~10×.
- **Related**: M-LIGHT v2 shadow-policy work; #2205 (spot-cone guard in the same loop).
- **Suggested Fix**: Update the `volumetrics_inject.comp` header comment and the `VOLUMETRIC_OUTPUT_CONSUMED` doc block in `volumetrics.rs` to state the real per-froxel ray budget, and consider gating the second (glass) ray behind a cheap "did the opaque-architecture mask miss AND is a glass-capable light" precheck so the common case stays at 1 ray.

#### REN-D17-NEW-03: stale line citation in the `sun_angular_radius` guard
- **Severity**: LOW
- **Dimension**: Soft Shadows (Dim 17)
- **Location**: `/mnt/data/src/gamebyro-redux/byroredux/src/render/sky.rs:104-107`
- **Status**: NEW
- **Description**: The debug-assert's rationale cites
  `triangle.frag:2418-2425` for the tangent-plane-approximation derivation.
  That block now lives at `triangle.frag:3029-3060` (the legacy-WRS arm) with a
  second copy of the sampler at `triangle.frag:2916-2921` (the ReSTIR arm, which
  is the default-on path and carries **no** such derivation comment).
- **Evidence**: `sky.rs:105` — *"Tangent-plane disk approximation valid only for
  α < ~0.05 rad (documented in triangle.frag:2418-2425)"*; lines 2418-2425 of
  `triangle.frag` are now ReSTIR pHat/reservoir prose, unrelated to the sun disk.
- **Impact**: Doc rot only; a future reader tuning `sun_angular_radius` (or a
  per-cell / per-TOD override, which #1023 made a one-line host-side write)
  lands on unrelated code and may not find the α < 0.05 rad validity bound.
  Note the guard threshold (0.10) is already 2× the documented validity bound.
- **Related**: #1023 / REN-D20-002; the ReSTIR path at `triangle.frag:2916`.
- **Suggested Fix**: Repoint to the symbol rather than the line number
  (`triangle.frag`'s directional shadow-jitter block) and add a one-line
  back-reference in the ReSTIR arm at 2916 so the default-on path carries the
  same caveat.

#### REN-D18-NEW-02: In-flight `WeatherTransitionRes` is never collapsed or cleared on a worldspace change
- **Severity**: LOW
- **Dimension**: Sky/Weather (Dim 18)
- **Location**: `byroredux/src/scene/world_setup.rs:apply_worldspace_weather` / `insert_procedural_fallback_resources`; consumed in `byroredux/src/systems/weather.rs:weather_system`
- **Status**: NEW
- **Description**: `WeatherTransitionRes` is a one-shot state machine (`elapsed_secs`, `duration_secs: 8.0`, `done`) that blends the live `WeatherDataRes` toward `target` and, on completion, promotes `target` into `WeatherDataRes`. Nothing ever removes it — `cell_loader/unload.rs` explicitly documents that the worldspace-scoped weather resources are *not* released on cell unload (#1199), and the only writers are the single `insert_resource` in `apply_worldspace_weather` and the `done = true` latch in `weather_system`. Two paths mishandle a transition that is still in flight when a second worldspace change lands:
  1. **WTHR branch retarget**: `insert_resource(WeatherTransitionRes { target: new_weather, elapsed_secs: 0.0, .. })` overwrites the in-flight transition while leaving `WeatherDataRes` at the *original* source snapshot. The on-screen colour was `lerp(src, oldTarget, t)`; the new fade restarts from `src`, so the frame of the switch pops backwards by `t * (oldTarget - src)`.
  2. **Procedural-fallback branch**: `insert_procedural_fallback_resources` replaces `WeatherDataRes` with `procedural_fallback_weather()` but leaves the in-flight transition installed. `weather_system` then keeps blending the procedural sky toward the *previous worldspace's* target, and on completion promotes that target's `sky_colors` / `fog` / `tod_hours` / `wind_speed` / `skyrim_dalc_per_tod` over the procedural fallback — the climateless worldspace ends up permanently rendering the prior worldspace's weather.
- **Evidence**:
  ```rust
  // world_setup.rs::apply_worldspace_weather — WTHR branch
  if world.try_resource::<WeatherDataRes>().is_some() {
      world.insert_resource(WeatherTransitionRes {
          target: new_weather, elapsed_secs: 0.0, duration_secs: 8.0, done: false,
      });                       // <- clobbers an in-flight fade; WeatherDataRes still holds the old source
  } else { ... }
  ```
  ```rust
  // world_setup.rs::insert_procedural_fallback_resources — no WeatherTransitionRes reset
  world.insert_resource(crate::env_translate::procedural_fallback_weather());
  ensure_game_time(world);
  ```
  Reachability: `app_step.rs:542` calls `apply_worldspace_weather` on **every** exterior-destination transition ("Always rebuild on exterior-destination transitions, even intra-worldspace"), plus `scene.rs:414` and `debug_load.rs:394`. Two exterior door transitions inside the 8-second `duration_secs` window are enough. `grep -rn "WeatherTransitionRes"` confirms no `remove_resource` call site anywhere in the tree.
- **Impact**: Case 1 is a one-frame colour pop, self-healing within 8s — cosmetic. Case 2 is persistent wrong weather (palette, fog distances, wind-driven cloud scroll, DALC cube) on a climateless worldspace until the next worldspace change. Both require two worldspace transitions within 8 seconds; case 2 additionally requires the second worldspace to have no CLMT/default WTHR, so vanilla content is effectively immune. No crash, no NaN (the `done` latch from REN-D15-NEW-07 still prevents `elapsed_secs` saturation).
- **Related**: Extends the M33.1 crossfade state machine hardened by #1101 / #1102 / #1103 / REN-D15-NEW-07, none of which addressed transition *lifetime* across a worldspace boundary. #1199 (worldspace-scoped weather resource lifetime) is the reason nothing clears it.
- **Suggested Fix**: Before installing a new transition (or a procedural-fallback `WeatherDataRes`), collapse any in-flight one: write the current blended snapshot into `WeatherDataRes` (or, cheaply, `lerp` at the live `t`) and set `done = true` / reset `elapsed_secs`. A `collapse_weather_transition(world)` helper called at the top of both branches of `apply_worldspace_weather` covers both cases in one place.

#### REN-D19-04: `perturbNormal` Path 1 multiplies by the raw interpolated `vertexTangent.w` instead of clamping it to ±1, unlike the three sibling TBN sites
- **Severity**: LOW
- **Dimension**: Tangent-Space (Dim 19)
- **Location**: `/mnt/data/src/gamebyro-redux/crates/renderer/shaders/include/material_sampling.glsl:170` (`perturbNormal`); same pattern at `crates/renderer/shaders/include/lighting.glsl:128` and `crates/renderer/shaders/triangle.frag:2288`
- **Status**: NEW
- **Description**: `.w` is exactly ±1 **per vertex** (guaranteed at import by `crates/nif/src/types.rs:154` `bitangent_sign` → `clamp_sign`, and by #2246 for the Starfield UDEC3 path). It is *not* ±1 **per fragment**: the varying is linearly interpolated, so any triangle whose three vertices disagree on handedness yields `w ∈ (-1, 1)`, hitting 0 at the mid-line. `perturbNormal` then builds `B = vertexTangent.w * cross(N, T)`, a *shortened* (or zero) bitangent, while `T` and `N` stay unit length — the TBN is no longer orthonormal and the V-axis component of the normal-map sample is attenuated toward zero. The POM sibling in the same file and the RT sibling both clamp first: `material_sampling.glsl:43` `tangentSign = vertexTangent.w < 0.0 ? -1.0 : 1.0;` and `include/ray_hit.glsl:191` `float tangentSign = localTangent.w < 0.0 ? -1.0 : 1.0;`.
- **Evidence**:
  ```glsl
  // material_sampling.glsl:169-171  (Path 1)
  T = normalize(T - dot(T, N) * N);
  vec3 B = vertexTangent.w * cross(N, T);   // raw interpolated w
  mat3 TBN = mat3(T, B, N);
  ```
  vs. the clamped form 127 lines above it in the same file (`:43`) and in `ray_hit.glsl:191`.
- **Impact**: Mixed-sign triangles are rare in authored Bethesda content (UV-seam vertices are duplicated, so a triangle normally spans one shell), but they are reachable through `synthesize_tangents` / `synthesize_tangents_yup`, where the sign is derived per vertex from *averaged* `tan_u` / `tan_v` accumulators — a vertex sitting on a UV fold can legitimately land on the opposite sign from its neighbours without the mesh duplicating it. Result is a band of washed-out normal-map relief (and, at `w ≈ 0`, a degenerate `mat3` column) along that seam. Cheap to make impossible; currently only 2 of 5 TBN reconstruction sites are hardened.
- **Related**: REN-D19-02 / #2246 (import-side ±1 clamp — this is the fragment-side residual it does not cover); REN-D19-01 / #2245.
- **Suggested Fix**: In `perturbNormal` (and for consistency `lighting.glsl:128`, `triangle.frag:2288`), replace the raw multiply with `float s = vertexTangent.w < 0.0 ? -1.0 : 1.0; vec3 B = s * cross(N, T);`, matching the POM and `ray_hit.glsl` sites, and note in the comment that the per-vertex ±1 guarantee does not survive interpolation.

#### REN-D20-NEW-03: `GpuTimerSnapshot::*_active` flags have zero consumers — #2278 landed only the producer half
- **Severity**: LOW
- **Dimension**: Debug/Telemetry (Dim 20)
- **Location**: `crates/renderer/src/vulkan/gpu_timers.rs:193-206` (fields) / `crates/renderer/src/vulkan/context/mod.rs:3162-3216` (`fill_skin_coverage_stats`, the only real consumer of the snapshot)
- **Status**: NEW (incomplete fix of Existing PERF-D9-01 / #2278)
- **Description**: PERF-D9-01 was "`0.0` is ambiguous between 'inactive' and 'genuinely
  instantaneous'". #2278 added fourteen `*_active: bool` companions to `GpuTimerSnapshot` and
  `snapshot_from_bits` fills them correctly (three unit tests pin the behaviour). But nothing
  outside the module ever reads them. `fill_skin_coverage_stats` — the sole path from the snapshot
  to the world — copies the fourteen `_ms` fields and drops all fourteen `_active` flags;
  `SkinCoverageStats` (`crates/core/src/ecs/resources/mod.rs`) has no `_active` members;
  `fill_upscaler_telemetry` reads only `upscale_ms`. A repo-wide grep for the flag names outside
  `gpu_timers.rs` returns only the module's own tests.
- **Evidence**: `grep -rn "_active\b" --include="*.rs" crates byroredux tools | grep -v vulkan/gpu_timers.rs`
  matches nothing in the renderer/telemetry path (all hits are unrelated: quest/audio/physics).
  `mod.rs:3173-3204` copies `snap.skin_dispatch_ms` … `snap.presentation_ms` and nothing else;
  the `else` branch zeroes the same fourteen `_ms` fields, which means "no timer at all" is
  *also* indistinguishable from "ran at 0 ms" at every surface.
- **Impact**: The original ambiguity is fully intact everywhere a human actually looks — the
  debug-UI `gpu_pass_ms` grid, `skin.coverage`, and the bench summary all still print `0.000 ms`
  for "the pass was skipped this frame", "the pass ran instantly", and "this GPU has no timestamp
  support". Interacts with REN-D20-NEW-02: skipped brackets contribute a clean `0.0` to the Σ,
  which makes the Σ look more trustworthy than it is. Diagnostic-quality only; no runtime effect.
- **Related**: PERF-D9-01 / #2278; #2040; REN-D20-NEW-02.
- **Suggested Fix**: Add the matching `bool` fields to `SkinCoverageStats`, copy them in
  `fill_skin_coverage_stats`, and have `metrics.rs` emit `None`/"n/a" rather than `0.0` into
  `gpu_pass_ms` for inactive brackets (widening the tuple to `(String, Option<f32>)`).

#### REN-D21-2026-08-07-02: `subsurface`/`sheen`/`sheen_tint`/`anisotropic` are hardcoded to zero in the draw path, so no scene can drive them
- **Severity**: LOW
- **Dimension**: Cornell Harness (Dim 21)
- **Location**: `/mnt/data/src/gamebyro-redux/byroredux/src/render/static_meshes.rs:collect_static_mesh_draws` (lines ~627-633)
- **Status**: NEW
- **Description**: The `DrawCommand` construction writes `subsurface: 0.0, sheen: 0.0, sheen_tint: 0.0, anisotropic: 0.0` as literals with a "when the importer surfaces them" TODO. `GpuMaterial` carries the fields, `hash_gpu_material_fields` hashes them, `include/pbr.glsl` and `lighting.glsl` consume them — but no CPU producer can ever make them non-zero, from game content or from the harness. This is the enabling half of finding 01: even if `MAT_FLAG_PBR_BSDF` were set on a Cornell probe, `disneyDiffuseSplit` would run with all three of its distinguishing parameters pinned at zero and degenerate back toward Burley-only.
- **Evidence**: literals at `static_meshes.rs:627-633`; the only non-zero writer of `GpuMaterial::subsurface` in the tree is `presets::skin_wax_marble()` in `crates/renderer/src/vulkan/material.rs`, which is a test/reference fixture with no render-path caller.
- **Impact**: Three shipped shader features (fake-SSS, sheen, anisotropic GGX) are dead code end-to-end with no runtime signal that they are inert; #1249/#1250 read as delivered from the shader side alone. Blast radius is limited to those lobes.
- **Related**: Finding 01 (same root gap seen from the harness side); #1249, #1250.
- **Suggested Fix**: Plumb the four scalars from `Material` (adding the fields if absent) through `DrawCommand`, then expose them via `mat.set` so Cornell can sweep them; until then, mark the shader-side lobes explicitly as unreachable in their doc comments.

#### REN-D21-2026-08-07-03: `glass()`'s `alpha: 0.25` is not reachable by `mat.list`'s own advertised round-trip
- **Severity**: LOW
- **Dimension**: Cornell Harness (Dim 21)
- **Location**: `/mnt/data/src/gamebyro-redux/byroredux/src/cornell.rs:glass`
- **Status**: NEW (documentation/observability nit; not a rendering defect)
- **Description**: `glass()` sets `alpha: 0.25` with a doc comment stating it is "currently unconsumed downstream". That is accurate for `taa.comp`/`composite.frag`, but the value *does* reach `GpuMaterial.material_alpha` through `to_gpu_material` and participates in `hash_gpu_material_fields`, i.e. it forces the two glass probes into distinct material-table slots from an otherwise identical opaque dielectric. The comment reads as "inert", which invites a future reader to treat the field as free to change; it is not free with respect to dedup identity.
- **Evidence**: `material.rs` `hash_gpu_material_fields` writes `mat.material_alpha.to_bits()`; `MaterialTable::intern_by_hash` keys on that hash.
- **Impact**: Cosmetic/doc only today. Matters if someone later uses the Cornell glass probes to measure `MaterialTable` dedup ratio (`ctx.scratch`, #780/PERF-N1) and is surprised by the extra slot.
- **Related**: #676 / DEN-6 (cited in the same doc comment).
- **Suggested Fix**: Amend the `glass()` doc comment to say the value is unconsumed *by the composite/TAA passes* but is part of the material dedup key.

#### REN-D22-05: Authored `period_secs` is ignored on the FLICKER path (hardcoded 12 Hz)
- **Severity**: LOW
- **Dimension**: Light Animation (Dim 22)
- **Location**: `byroredux/src/systems/light_anim.rs:164` (`flicker_intensity`, flicker branch)
- **Status**: NEW
- **Description**: The pulse branch honours `flicker.period_secs`; the
  flicker branch steps its hash buckets at a hardcoded `12.0` Hz and never
  reads the authored period at all. Skyrim authors a per-light FNAM period
  (candles ~0.5 s, larger fixtures longer), so every flickering light in a
  scene runs at an identical rate regardless of what the record asked for —
  the only per-light variation left is `phase_offset_secs`.
- **Evidence**:
```rust
let raw = (total_time + flicker.phase_offset_secs) * 12.0 * speed_scale; // period_secs unused
```
- **Impact**: Visual-only and subtle; a roomful of mixed fixtures flickers
  homogeneously. The `12.0` is documented as a *tuning* value (24 Hz → 12 Hz
  in Phase 19) with no note that it deliberately supersedes the authored
  period, so this reads as an oversight rather than a decision.
- **Related**: REN-D22-03 — on pre-Skyrim games the parsed `period_secs` is
  garbage anyway, so fixing this one without that one would make FNV worse.
- **Suggested Fix**: Derive the bucket rate from `period_secs` (e.g.
  `buckets_per_sec = k / period_secs`, `k` chosen so the current 0.5 s Skyrim
  candle still lands at ~12 Hz), or state explicitly in the comment that the
  authored period is intentionally not used for the noise path.

#### REN-D22-06: Shadow sibling keeps unnamed bits for Oblivion/FO3NV where the animation sibling drops them — mirrored-pair policy asymmetry
- **Severity**: LOW
- **Dimension**: Light Animation (Dim 22)
- **Location**: `byroredux/src/systems/light_anim.rs:99` (`canonical_light_shadow_flags`)
- **Status**: NEW
- **Description**: The two canonicalizers apply opposite policies to
  *unnamed* bits. `canonical_light_animation_flags` deliberately masks out
  `0x40`/`0x100` for FO4/FO76 precisely because those positions are unnamed
  in those games' LIGH definitions (the documented rationale: an unnamed bit
  must not decode into behavior). `canonical_light_shadow_flags` takes the
  opposite stance for Oblivion/FO3/FNV: it applies the full TES5
  `LIGHT_FLAG_SHADOW_MASK` (`0x400|0x800|0x1000`) to them, and its docstring
  justifies this with an absence-of-evidence argument ("No divergence has
  been identified"). Of those three bits only `0x400` (Spot Shadow) is a
  named flag in the Oblivion/FO3/FNV LIGH layouts; `0x800`/`0x1000` have no
  named meaning there. This is exactly the "per-game divergence added to one
  and not the other" shape the pair is supposed to be audited for, expressed
  as a policy split rather than a missing arm.
- **Evidence**:
```rust
// unnamed bits are DROPPED here for FO4/FO76 …
GameKind::Fallout4 | GameKind::Fallout76 => LIGHT_FLAG_FLICKER | LIGHT_FLAG_PULSE,
// … but KEPT here for Oblivion/Fallout3NV (0x800/0x1000 unnamed in their layouts)
GameKind::Starfield => 0,
_ => LIGHT_FLAG_SHADOW_MASK,
```
  `every_game_shares_the_same_shadow_mask_today` pins the permissive
  behaviour for `Oblivion` and `Fallout3NV` explicitly.
- **Impact**: An Oblivion/FO3/FNV LIGH that happens to carry `0x800` or
  `0x1000` (reserved/junk there) is silently promoted to "casts shadows" and
  gets RT shadow rays. Unconfirmed against real record data — reported as a
  consistency/policy gap, not a verified data corruption. Blast radius is
  bounded by however many such records exist (possibly zero).
- **Related**: #2250 (REN-D22-01), #2251 (REN-D22-02).
- **Suggested Fix**: Either give Oblivion/Fallout3NV an explicit
  `=> LIGHT_FLAG_SHADOW_SPOTLIGHT` arm (only the bit their layouts name), or
  add a sentence to the docstring stating the deliberate asymmetry — that
  shadow decode is permissive-by-default while animation decode is
  strict-by-default — so the next auditor doesn't read it as drift.

#### REN-D23-2026-08-07-02: `fsr_gated_dof` keys off `fsr_temporal.is_some()`, so DOF stays disabled in the FSR-failed fallback where there is no FSR jitter to conflict with
- **Severity**: LOW
- **Dimension**: FSR Upscaler (Dim 23)
- **Location**: `/mnt/data/src/gamebyro-redux/crates/renderer/src/vulkan/context/draw.rs:fsr_gated_dof` (call site `draw.rs:1628`)
- **Status**: NEW
- **Description**: `let active_dof = fsr_gated_dof(dof, self.fsr_temporal.is_some());` forces `aperture = 0.0` whenever FSR *mode* is selected. `fsr_temporal` is `Some` for the whole of `UpscalerMode::Fsr3(..)`, including when the FSR context never got created or `dispatch_failure` has latched — states where the frame runs completely unjittered on the native blit. The documented rationale ("combining the independent Halton(5,7) lens sequence with FSR's own projection jitter would violate the motion/reprojection contract") does not apply there, since FSR's projection jitter is exactly what has been switched off.
- **Evidence**: `draw.rs:1573-1604` sets `fsr_jitter_pixel = None` and `jx/jy = 0.0` when `!upscaler.is_fsr_dispatch_active()`, yet `draw.rs:1628` still passes `self.fsr_temporal.is_some()` (unchanged by dispatch failure) to `fsr_gated_dof`.
- **Impact**: Authored DOF is silently dropped in the degraded FSR path. Visual-only, only in an already-degraded state. Also a latent inconsistency if a future change makes the two predicates matter independently.
- **Related**: REN-D23-2026-08-07-01 (same "FSR mode selected != FSR running" conflation).
- **Suggested Fix**: Pass the same predicate the jitter gate uses — `self.frame_upscaler.as_ref().is_some_and(|u| u.is_fsr_dispatch_active())` — so DOF and jitter are gated on one fact.

#### REN-D23-2026-08-07-03: A mid-frame dispatch failure presents a jittered-but-unresolved frame
- **Severity**: LOW
- **Dimension**: FSR Upscaler (Dim 23)
- **Location**: `/mnt/data/src/gamebyro-redux/crates/renderer/src/vulkan/frame_upscaler.rs:FrameUpscaler::record` (dispatch-`Err` recovery arm)
- **Status**: NEW
- **Description**: The projection jitter for frame N is chosen at the *top* of `draw_frame` from `is_fsr_dispatch_active()`, but `dispatch_failure` can be set later in the same frame inside `record`. On that one frame the geometry pass has already rendered with a sub-pixel FSR jitter offset applied, and the recovery path blits that jittered image straight through. No pass resolves it. This is the same class of hazard `taa_jitter`'s `!taa_failed` gate (#1932 / TAA-D13-01) was added to close on the TAA side; the FSR side has no equivalent for the failing frame itself (subsequent frames are correctly unjittered).
- **Evidence**: `draw.rs:1573` reads `is_fsr_dispatch_active()`; `frame_upscaler.rs` sets `self.dispatch_failure = Some(error.to_string())` inside the `if let Err(error) = dispatch` arm, then calls `record_native_blit` on the already-jittered `inputs.scene_color`.
- **Impact**: One frame of un-resolved sub-pixel offset, i.e. a single-frame image shift/shimmer. Reachable only via a genuine SDK error or `BYRO_FSR_FORCE_DISPATCH_FAIL=1`, and only once per swapchain generation (the latch suppresses further attempts).
- **Related**: #2140, #2146; `BYRO_FSR_FORCE_DISPATCH_FAIL` fault injection.
- **Suggested Fix**: Document it as accepted (one frame, degraded path) rather than adding machinery — or, if it matters, have the recovery arm also call `signal_temporal_discontinuity(1)` so nothing downstream reprojects against that frame.

#### REN-D23-2026-08-07-04: `UpscalerMode::Taa` pays a full-resolution 1:1 image blit every frame that produces a byte-identical image
- **Severity**: LOW
- **Dimension**: FSR Upscaler (Dim 23)
- **Location**: `/mnt/data/src/gamebyro-redux/crates/renderer/src/vulkan/frame_upscaler.rs:FrameUpscaler::record_native_blit`
- **Status**: NEW
- **Description**: In `UpscalerMode::Taa`, `FrameExtentSet::for_output` sets `render == output`, so the bridge's `cmd_blit_image` src/dst offsets are identical and the `LINEAR` filter degenerates to an exact copy. Every TAA-mode frame therefore reads and writes a full-resolution `R16G16B16A16_SFLOAT` image (~16 MB of traffic at 1080p, ~66 MB at 4K) plus two pipeline barriers, purely to move data into a target `presentation.frag` could have sampled directly. The module doc frames the split as deliberate ("keeps scene composition and presentation decoupled, and gives FSR one explicit frame-graph slot"), which is a sound design argument — the cost is the part that isn't documented.
- **Evidence**: `upscaling.rs:FrameExtentSet::for_output` — `UpscalerMode::Taa => output`; `record_native_blit` builds `src_offsets` from `self.extents.render` and `dst_offsets` from `self.extents.output`.
- **Impact**: Pure bandwidth on the non-default path. Not a correctness issue. Grows with output resolution.
- **Related**: `docs/engine/fsr3-upscaler-integration-plan.md` phase 4 (native bridge).
- **Suggested Fix**: If TAA mode ever matters for perf again, let `PresentationPipeline` bind composite's scene view directly when `render == output` and skip the blit; otherwise add the cost to the module doc so the next reader does not rediscover it. Do NOT re-bench the FSR matrix off the back of this — it does not touch the FSR path.

---

## Prioritized Fix Order

Ordering is **correctness → safety → optimization**, then cheapest-first within a tier. Items whose
only responsible next step is a capture are listed in "Needs-RenderDoc" instead of being scheduled
here; they are marked *(gated)* where they also appear below.

### Tier 0 — GPU memory safety (do first)

1. **`AS-D1-NEW-01`** (HIGH) — union `skinned_blas` into the `shrink_blas_scratch_to_fit` peak walk,
   including the `peak == 0` early-drop arm, and add the `debug_assert!` in `refit_skinned_blas`.
   Pure CPU bookkeeping over an already-recorded field, unit-testable, no barrier/stage change.
   Fix first, then *verify* with `BYRO_VALIDATION=1` *(gated for the "does the driver actually
   fault?" question, not for the fix itself — the fix is unambiguously correct either way)*.

### Tier 1 — Correctness: shading/decode defects with a wrong result on real content

2. **`REN-D22-03`** — pre-Skyrim LIGH flicker parameters decoded at Skyrim offsets. Flicker is
   provably dead on FNV/FO3/Oblivion *and* `period_secs` currently reads the record's Weight.
   Widest real-content blast radius of any MEDIUM here; the fix is a length/game discriminant in
   one `match` arm plus an `intensity_amplitude` default at the boundary.
3. **`REN-D15-NEW-01`** — water caustics are entirely suppressed underwater (regression of #2223).
   One-line light-side normal (`NperturbedLight`), plus the comment tying the two normals together.
4. **`REN-D9-NEW-01`** — compute-vs-raster zero-weight skinning fallback. Prefer the *import-side*
   half of the suggested fix (renormalise/zero-fill in the SSE packed-half path) so `wsum == 0`
   becomes unreachable; it removes a whole class of degenerate skinned BLAS AABBs.
5. **`REN-D17-NEW-02`** — apply `1/PI` to all three arms of `pathEnvironmentRadiance` (and both arms
   at `triangle.frag:2212`) and extend the existing regression test to the non-DALC arms. Fixes a
   systematic ~π cross-game ambient step.
6. **`REN-D17-NEW-01`** — specular-AA α-vs-α². Correct the units, re-check the `0.025²` floor's
   meaning, recompile `triangle.frag.spv` + `water.frag.spv`.
7. **`REN-D2-2026-08-07-01`** — viewer-flip the GI hemisphere axis (or hoist a single
   `fragNormalEffectiveView`). *(gated on a visual check only for magnitude; the logic
   inconsistency is definite.)*
8. **`REN-D16-2026-08-07-01`** — pick one froxel depth convention and apply it on both ends.
   Prefer the composite-side `(u*N - 0.5)/N` re-centering, which leaves `inject`'s #1462 front-edge
   convention untouched.
9. **`REN-D22-04`** — scale the *period*, not the phase, in the pulse branch, and extend
   `pulse_slow_runs_at_half_angular_velocity` to a sample past one period.
10. **`REN-D8-N01`** — composite the sky *behind* the main-pass result. *(gated: confirm the intended
    layering with an exterior particle-over-sky capture before shipping — the one-layer `direct4.a`
    version and the accumulated-coverage-lane version differ visibly.)*
11. **`REN-D18-NEW-01`** — clamp `afternoon_cool` against its true predecessor and tighten the
    monotonicity test corpus. Not reachable on vanilla content, but it is a two-token fix with a
    test that currently gives false assurance.
12. **`REN-D14-2026-08-07-01`** — caustic EMA dynamic-scene invalidation. Prefer option (b) first
    (lower `CAUSTIC_DECAY_MAX`), which is a one-constant change, and treat the scene-dirty signal
    as the follow-up.
13. **`REN-D15-NEW-02`** — pass the already-computed `uvOrigin` into `foamFlowStreaks`. Same one-line
    pattern as the `sampleScrollingNormal` rebase.
14. **`REN-D19-03`** — fill the terrain tangent lane in `spawn_terrain_mesh`.
15. **`NIFAL-D6-2026-08-07-01`** — land `grayscale_to_palette_scale` on `Material` + `translate_material`
    + the "copies every canonical field" test now; open a tracker for the `GpuMaterial`/shader half.
16. **`REN-D23-2026-08-07-01`** — decide the FSR-failure policy: either escalate to a real
    `set_upscaler_mode(Taa)` at startup, or correct the doc. Do not leave the two disagreeing.

### Tier 2 — Safety nets and latent traps (no wrong pixels today; remove the foot-guns)

17. **`REN-D3-2026-08-07-02`** — add `DalcCubeUBO` to the existing `reflect.rs` `.spv` size table.
    A few lines against machinery that already exists; highest value-per-effort in this tier.
18. **`REN-D3-2026-08-07-01`** — add `size_of` + `offset_of!` + GLSL-field-list pins for
    `GpuTerrainTile`, reusing the `strip_struct_body`/`extract_struct_body` helpers.
19. **`D12-2026-08-07-01`** — make `record_post_passes` return `()` and delete the caller's dead
    recovery branch, so a future `?` is a compile error rather than a silent #2146 violation.
20. **`D12-2026-08-07-02`** — set an `indirect_upload_ok` flag and force the direct-draw fallback;
    turns a potential TDR into a visibly-degraded frame.
21. **`AS-D1-NEW-02`** — `take()` + budget-decrement + `pending_destroy_blas` at the three BLAS
    registration sites.
22. **`REN-D20-NEW-01`** — rebuild the whole `EguiPass` on a swapchain format change, mirroring
    `presentation`. *(gated: needs an HDR-toggle / monitor-move repro to observe, but the code
    asymmetry against `resize.rs:186` is unambiguous.)*
23. **`REN-D14-2026-08-07-04`** — one-shot clear on the caustic skip paths so the feature fails to
    black rather than to frozen.
24. **`D12-2026-08-07-03`** — convert the four in-render-pass `expect()`s to `let … else`.
25. **`NIFAL-D6-2026-08-07-03`** — clamp/finite-guard the `mat.set` PBR arms (or just call
    `resolve_pbr()` after the mutation).
26. **`REN-D19-04`** — clamp `vertexTangent.w` to ±1 at the three unhardened TBN sites.
27. **`D5-02`** — null `self.buffer` in `GpuBuffer::destroy` and the `Drop` safety-net arm.
28. **`REN-D15-NEW-03`** — bind a genuinely zero R32_UINT image on the composite binding-8 fallback,
    and correct the "this is safe" comment that is factually wrong.
29. **`NIFAL-D6-2026-08-07-04`** — extract `attach_blend_and_facing_markers` and call it from both
    spawn sites (follow the #2300 precedent).
30. **`REN-D9-NEW-02`** — hoist the skin-victim drain / LRU sweep out of the global-vertex-buffer guard.
31. **`REN-D23-2026-08-07-02`** — gate DOF on `is_fsr_dispatch_active()` rather than
    `fsr_temporal.is_some()`, so DOF and jitter share one predicate.
32. **`REN-D22-06`** — either narrow the Oblivion/FO3NV shadow arm to the named bit or document the
    deliberate strict-vs-permissive asymmetry.
33. **`REN-D21-2026-08-07-01`** + **`REN-D21-2026-08-07-02`** — fix together: plumbing the four Disney
    scalars through `DrawCommand` is the enabling half, the `mat.set` arms + a `PBR_BSDF` probe row
    is the harness half. Without both, Cornell keeps answering for the wrong BRDF.

### Tier 3 — Optimization / cost

34. **`D5-01`** — extend `shrink_scratch_if_oversized` to `previous_models_scratch` and add a capacity
    policy to the two rigid-history maps. Purely additive, no ordering constraints, measurable today
    via `ctx.scratch`.
35. **`REN-D16-2026-08-07-02`** — gate the second (glass) froxel shadow ray behind a cheap precheck so
    the common case stays at 1 ray, *and* correct the documented budget either way.
36. **`REN-D10-NEW-01`** — collapse the textured water UV magnitude by subtracting the tile-integral
    origin, and clamp `freqScale` at the CPU packing site. *(gated on a large-world capture for
    magnitude; the clamp is worth doing regardless.)*
37. **`REN-D14-2026-08-07-02`** — stochastically round the caustic decay `imageStore`.
38. **`REN-D14-2026-08-07-03`** — make `parked_frames` per-FIF (or divide by `MAX_FRAMES_IN_FLIGHT`).
39. **`REN-D23-2026-08-07-04`** — skip the 1:1 blit when `render == output`, or document the cost.
40. **`REN-D11-2026-08-07-05`** — one-shot startup format-feature check for the G-buffer colour
    formats. Do it whenever non-NVIDIA bring-up starts.
41. **`REN-D20-NEW-03`** — plumb the `*_active` flags to `SkinCoverageStats` and emit `n/a` instead
    of `0.0`. Pairs naturally with #42.
42. **`REN-D20-NEW-02`** — drop or honestly relabel the GPU "Σ ms" headline.
43. **`REN-D2-2026-08-07-03`** — track `refrRemaining` in the refraction passthru loop (or amend the
    comment to state the 3-segment reach is intended).
44. **`REN-D23-2026-08-07-03`** — either document the one-frame jittered-blit as accepted or call
    `signal_temporal_discontinuity(1)` in the recovery arm.

### Tier 4 — Documentation / contract rot (batch these; each is minutes)

45. **`REN-D11-2026-08-07-04`** — water blend-state comment (actively misleading: it asserts water
    writes no FSR mask). Highest-priority doc fix.
46. **`REN-D11-2026-08-07-02`** + **`REN-D11-2026-08-07-03`** — same drift, fix together.
47. **`REN-D8-N02`** + **`REN-D8-N03`** — composite dead-field and vestigial-gate docs; note that
    *removing* the `underwater` field changes the UBO block size and needs a coordinated `.spv`
    recompile plus a `composite_params_is_16_byte_aligned_std140_shape` update.
48. **`REN-D3-2026-08-07-03` / `MAT-D7-2026-08-07-01`** (merged) — 112 B → 128 B in two comments,
    and recompute the `MAX_MATERIALS` arithmetic at 348 B; align the budget figure with
    `feedback_vram_baseline.md` (6 GB RT minimum, not 4 GB).
49. **`MAT-D7-2026-08-07-03`** — fold the second "75 fields" site into #2273.
50. **`MAT-D7-2026-08-07-02`** — reword the `hash_material_slice` docstring; drop the line numbers.
51. **`REN-D4-2026-08-07-03`** — add the FSR depth read to the `sync.rs` `MAX_FRAMES_IN_FLIGHT`
    consumer list.
52. **`NIFAL-D6-2026-08-07-02`** — delete `initial_radius` from the stale "intentionally not applied"
    bullet in `docs/engine/nifal.md`.
53. **`REN-D18-NEW-02`** — add `collapse_weather_transition(world)` at the top of both branches of
    `apply_worldspace_weather`. (Behavioural, but the reachable half is cosmetic and the persistent
    half needs a climateless worldspace, so it batches here.)
54. **`REN-D9-NEW-03`** — `127` → `MAX_BONES_PER_MESH - 1`.
55. **`REN-D10-NEW-02`** — copy `cluster_cull.comp:69`'s `renderOrigin.w` wording into
    `caustic_splat.comp:76`.
56. **`REN-D17-NEW-03`** — repoint the `sun_angular_radius` citation to a symbol, and back-reference
    from the ReSTIR arm.
57. **`REN-D11-2026-08-07-01`** — `find_depth_format` error string.
58. **`REN-D21-2026-08-07-03`** — amend the Cornell `glass()` doc comment re: dedup identity.

---

## Needs-RenderDoc

Items whose responsible next step is a **capture or validation-layer run**, not a code edit. This
respects the project's standing anti-speculation policy on Vulkan barrier / render-pass / pipeline
changes: failure modes here are invisible to `cargo test`, so a static read is not sufficient
evidence to ship a change.

### Blocking — do not change code before capturing

| Finding | Severity | What to capture | Why gated |
|---|---|---|---|
| `REN-D4-2026-08-07-01` | MEDIUM | `BYRO_VALIDATION=1` **with synchronization validation enabled**; look for an acquire-ordering hazard on the swapchain image | The finding's own Suggested Fix says explicitly: *"needs RenderDoc / sync-validation verification. … Do not blind-fix."* Changing a `SUBPASS_EXTERNAL` dependency mask on speculation is precisely the class of change `feedback_speculative_vulkan_fixes.md` prohibits. |
| `REN-D4-2026-08-07-02` | LOW | Sync-validation run over `copy_depth_to_history` | The finding states the chain is *almost certainly legal today* via an incidental `FRAGMENT_SHADER \| SHADER_READ` overlap. Confirm the layer accepts the chain before widening the barrier. The proposed change is a strict widening, so it is safe *if* made — but it should be justified, not guessed. |

### Verification-only — fix is unambiguous, capture confirms severity/behaviour

| Finding | Severity | What to capture | What it settles |
|---|---|---|---|
| `AS-D1-NEW-01` | HIGH | `BYRO_VALIDATION=1` (BDA / sync validation) during a resize-with-NPCs and an exterior→interior unload | Whether the driver actually faults on the scratch overrun or silently over-reserves. **Does not gate the fix** — the peak walk is wrong either way and the correction is pure CPU bookkeeping. |
| `REN-D8-N01` | MEDIUM | Exterior frame with a particle/smoke billboard silhouetted against open sky | Confirms the intended layering before choosing between the cheap `direct4.a` one-layer version and a real accumulated-coverage lane. |
| `REN-D20-NEW-01` | MEDIUM | A genuine surface-format change: HDR/SDR display toggle, or dragging the window between monitors | The failure mode does not appear in `cargo test`; needs a repro to observe the framebuffer/format mismatch and the `srgb_framebuffer` flip. |
| `REN-D2-2026-08-07-01` | MEDIUM | Visual check on two-sided foliage/curtain content viewed from the back face | The logic inconsistency is definite; the *pixel-level magnitude* of the AO-floor pinning and wrong-hemisphere GI is not measured. |
| `REN-D2-2026-08-07-02` | MEDIUM | Curved glass at grazing angles, glass-passthru debug viz (`DBG_VIZ_GLASS_PASSTHRU`) | Confirms whether the `tMin = 0.0` self-hit actually commits on real content or is absorbed by the 0.05 nudge. |
| `REN-D10-NEW-01` | LOW | Large-world capture (MarkarthWorld, ~176 k units) on textured water | The whole dimension is invisible below ~100 k world units. Also unresolved: **what vanilla Skyrim/FO4 WATR actually author for `wave_frequency`** — deliberately not guessed (per `feedback_no_guessing.md`), and it drives the real blast radius. |

### Standing capture debt carried forward (not new findings)

- **`#2258` / `#2259` post-refactor smoke run** — the `record_post_passes` per-pass split
  (`7bb517b2`) still has an outstanding RenderDoc/validation smoke run from
  `AUDIT_RENDERER_2026-08-03.md`. Dimension 12 re-verified by code review that the pass sequence is
  byte-for-byte order-preserving, but that is not a substitute for the capture. Process hygiene,
  already recorded there.
- **`REN-D23-07`** (from `AUDIT_RENDERER_2026-08-02.md`) — the `record_fsr_barriers_after`
  old-layout assumption is an *empirically validated* contract (900-frame `BYRO_VALIDATION=1` sweep,
  clean). Dimension 23 deliberately proposed **no** barrier change and notes that **any FFX SDK bump
  must re-run that sweep** rather than be reasoned about statically.
- **Dimension 10, whole-dimension caveat** — every positive verdict in the camera-relative /
  precision dimension is a static-trace result, not an observed-pixel result. Nothing there is
  reachable from `cargo test`, and the actual rendered result at MarkarthWorld scale has not been
  captured.
- **Dimension 11, water `DEPTH_BIAS` dynamic-state gap** (verified-clean item, not a finding) — the
  water pipeline omits `DEPTH_BIAS` from its dynamic states, so binding it resets command-buffer
  bias state. Dormant today because nothing after water in the pass uses bias; it becomes observable
  only if draw order changes, and proving it would need a capture. Recorded as a forward-compat trap.
- **Dimension 11, outgoing subpass dependency** — nothing in the barrier / subpass-dependency graph
  looked wrong on read, and per the no-speculative-fixes rule no change is proposed. Tightening the
  outgoing `dst_stage_mask` would require a capture, not a code read.

### Explicitly *not* needing a capture

Dimensions 12 and 13 both state it directly: all of Dimension 12's findings are statically
verifiable, and Dimension 13 produced no findings at all. The remaining MEDIUM/LOW findings in this
report are host-side logic, GLSL math, or documentation, and are testable or reviewable without a
GPU capture.

---

## Appendix — per-dimension finding counts

| Dim | Area | CRIT | HIGH | MED | LOW | Total |
|---|---|---|---|---|---|---|
| 1 | Acceleration structures (BLAS/TLAS) | 0 | 1 | 0 | 1 | 2 |
| 2 | SSBO/index plumbing & RT ray queries | 0 | 0 | 2 | 1 | 3 |
| 3 | GPU-struct layout (shader lockstep) | 0 | 0 | 2 | 1 | 3 |
| 4 | Synchronization & barriers | 0 | 0 | 1 | 2 | 3 |
| 5 | GPU memory & resource lifecycle | 0 | 0 | 0 | 2 | 2 |
| 6 | NIFAL material canonical translation | 0 | 0 | 1 | 3 | 4 |
| 7 | Material table | 0 | 0 | 0 | 3 | 3 |
| 8 | Denoiser & composite | 0 | 0 | 1 | 2 | 3 |
| 9 | GPU skinning + BLAS refit | 0 | 0 | 1 | 2 | 3 |
| 10 | Camera-relative origin & f32 precision | 0 | 0 | 0 | 2 | 2 |
| 11 | Pipeline state & render pass / G-buffer | 0 | 0 | 0 | 5 | 5 |
| 12 | Command-buffer recording | 0 | 0 | 0 | 3 | 3 |
| 13 | TAA (M37.5) | 0 | 0 | 0 | 0 | **0 — clean** |
| 14 | Caustic splat | 0 | 0 | 1 | 3 | 4 |
| 15 | Water M38 + water-side caustics | 0 | 0 | 2 | 1 | 3 |
| 16 | Volumetrics (M55) & bloom (M58) | 0 | 0 | 1 | 1 | 2 |
| 17 | Disney BSDF / PBR gating + soft shadows | 0 | 0 | 2 | 1 | 3 |
| 18 | Sky / weather / exterior lighting | 0 | 0 | 1 | 1 | 2 |
| 19 | Tangent space & normal maps | 0 | 0 | 1 | 1 | 2 |
| 20 | Debug overlay & GPU telemetry | 0 | 0 | 2 | 1 | 3 |
| 21 | Cornell-box RT harness | 0 | 0 | 1 | 2 | 3 |
| 22 | Light animation canonical translation | 0 | 0 | 2 | 2 | 4 |
| 23 | FSR 3.1 upscaler & presentation chain | 0 | 0 | 1 | 3 | 4 |
| | **Raw totals** | **0** | **1** | **22** | **43** | **66** |
| | **After merging the D3/D7 duplicate** | **0** | **1** | **22** | **42** | **65** |
