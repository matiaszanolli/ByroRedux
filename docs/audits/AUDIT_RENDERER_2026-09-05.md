# Renderer Audit — 2026-09-05 (scoped: `volumetrics-deep`)

**This is a SCOPED run, not a full renderer audit.** It covers only four of
`/audit-renderer`'s dimensions, selected by the `volumetrics-deep` audit-suite
preset (`/audit-renderer --focus 1,2,5,16`):

| Dim | Name | Why it is in scope |
|---|---|---|
| 1 | Acceleration Structures (BLAS/TLAS) | volumetrics' per-froxel TLAS shadow ray vs BLAS/TLAS lifecycle + eviction |
| 2 | SSBO/Index plumbing & RT ray queries | the froxel shadow/boundary ray queries and their SSBO indexing |
| 5 | GPU memory & resource lifecycle | `VolumetricsPipeline` allocation/destroy pairing, resize, dispatch-skip |
| 16 | Volumetrics (M55) & Bloom (M58) | the subject |

**Dimensions 3–4, 6–15, and 17–23 were NOT examined.** In particular
GPU-struct layout (3), sync/barriers (4), NIFAL (6), material table (7),
denoiser/composite (8), skinning (9), precision (10), pipeline/render pass
(11), command recording (12), TAA (13), caustics (14), water (15), Disney
BSDF (17), sky/weather (18), tangent space (19), telemetry (20), Cornell (21),
light animation (22) and the FSR/presentation chain (23) carry no coverage
from this report.

**Trigger**: a run of recent volumetric-lighting changes — `81c63681`
(Fix #3611, far-plane single-source-of-truth), `140d8bad` (atmospheric wind
advection in the combustion transport solver), `d924bf81`
(`FogProfile::OilExplosion`/`NuclearExplosion` + mushroom-cloud SDF),
`ae71ace9` (`froxelLightAtten` range recovery via the shared cull multiplier),
and `17b744b5` (doc-rot). The preset traces volumetrics through AS / SSBO /
memory rather than reading `volumetrics.rs` and its two shaders in isolation.

**Verification discipline applied**: every finding is anchored on a symbol
(struct/fn/const/test name) confirmed by `grep` against the live tree, not on
line numbers. No render-pass / pipeline / barrier edit is proposed on
reasoning alone. No bench numbers are quoted. Dimension 16's claims were run
through the actual test suite (36 volumetrics tests, `fog_volume`, the UBO
block-size reflection test, the three `ae71ace9` pinning tests — all green)
rather than hand-derived. The single CRITICAL finding below was independently
re-verified by the orchestrator against live source before inclusion.

**Dedup baseline**: `gh issue list --limit 200`
(`/tmp/audit/renderer/issues.json`, 65 open); `docs/audits/` scanned for prior
renderer reports (most recent full sweep: `AUDIT_RENDERER_2026-08-30.md`; most
recent scoped: `AUDIT_RENDERER_2026-09-04.md` `water-deep`; prior dedicated
Dim-16 reports: `AUDIT_RENDERER_2026-05-23_DIM16.md`,
`AUDIT_RENDERER_2026-05-26_DIM16.md`, `AUDIT_RENDERER_2026-07-14_DIM8_DIM16.md`).

---

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 1 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 5 |
| **Total** | **6** |

By dimension: Dim 1 → 2 LOW (both pre-existing); Dim 2 → 1 CRITICAL + 1 LOW;
Dim 5 → 1 LOW; Dim 16 → 2 LOW (one deduplicated against Dim 2's — see below).

**Of the 5 LOW findings, 3 are NEW and 2 are pre-existing** (`REN-D1-01` is
already open as #3824; `REN-D1-02` was reported in the 2026-08-30 sweep and
still has no GitHub issue).

Pipeline areas affected: the volumetrics combustion-transport boundary-collision
read path (the CRITICAL), and four documentation sites — two in
`docs/engine/shader-pipeline.md` and `docs/engine/memory-budget.md`, one
in-shader comment, and two stale acceleration-structure comments.

**Headline**: one CRITICAL SSBO stride mismatch in the volumetrics injection
shader, introduced passively 13 days ago when `GpuInstance` grew and a sixth,
untracked GLSL mirror outside the lockstep guard's coverage did not.

**Notable negative result**: the volumetric *numerics* are clean. A full read
of both shaders found no NaN/inf hazard anywhere — every division has a floor,
the HG phase clamp is exactly as specified, and `VolumetricsParams` is in full
Rust↔GLSL lockstep including this week's two new wind fields (and is pinned by
a test that computes `size_of` dynamically, so it cannot silently drift). The
per-froxel TLAS shadow ray was also cleared of the "races a BLAS/TLAS rebuild
or eviction" hypothesis this run was launched to test.

---

## RT Pipeline Assessment

**BLAS/TLAS (Dim 1) — clean.** The special-focus question was whether
volumetrics' per-froxel TLAS shadow query can hold a stale handle, miss
eviction bookkeeping, or race a rebuild. Answer: **no**, on all three counts.
`VolumetricsPipeline` holds no `vk::AccelerationStructureKHR` across frames —
`record_volumetrics_pass` (`context/post_passes.rs`) fetches the TLAS fresh
each frame via `accel.tlas_handle(frame)`, and `VolumetricsPipeline::write_tlas`
takes it by value and sets a one-shot `tlas_written[frame]` bool latch
(consumed by a `debug_assert!` in `dispatch`). It never touches
`has_blas` / `mark_*_used` / `evict_*` / `pending_destroy_*`. Build →
barrier → dispatch → eviction all land in the same per-frame command buffer in
program order, and `build_tlas` re-stamps `last_used_frame` for every
referenced BLAS before the LRU scan runs, so a referenced BLAS cannot be the
eviction victim. Volumetrics is a pure per-frame consumer, mirroring
`CausticPipeline::write_tlas`. Build flags, `instance_custom_index` encoding,
the `MAX_INSTANCES < (1 << 24)` const-assert, deferred BLAS destruction
(`pending_destroy_blas` + `DEFAULT_COUNTDOWN`), and the shrink call-site split
all re-confirmed unchanged.

**SSBO indexing & ray queries (Dim 2) — one CRITICAL.** The main raster/RT
path is clean: `instance_custom_index` discipline, shadow-ray disk/cone
geometry and `tMin` bias, reflection rays, 1-bounce GI, the glass/IOR family
(including all four named regression tests), RT gating on `sceneFlags.x`,
TLAS binding correctness at both `volumetrics.rs` `write_tlas` call sites,
IGN/frame-counter noise seeding, ReSTIR-DI spatial reuse + the stable
surface-ID tag, and the BC1 punch-through alpha gate were all verified with no
regressions. The CRITICAL is confined to the volumetrics boundary-geometry
read path (bindings 19/20/21) — a code path the last full sweep explicitly
logged as "not examined by this dimension", which is why 13 days elapsed
before anyone looked at it.

**Ray-query safety — one suspected bug investigated and disproved.** Three
sites (`shadow_common.glsl`'s `traceShadowBinary`, `triangle.frag`'s
`windowRQ`, `water.frag`'s `foamShoreline`) call `rayQueryProceedEXT` once
rather than in a `while` loop like every other site. Verified against ARM's
ray-query documentation and the Vulkan Documentation Project: a single
call-and-ignore-return is spec-sanctioned specifically when
`gl_RayFlagsOpaqueEXT` is set, because no any-hit decision point can ever
occur. All three sites set that flag. **Correct as written — recorded here so
it is not re-litigated by a future sweep.**

---

## GPU-Struct & Memory Assessment

**Memory & lifecycle (Dim 5) — clean, no leaks.** `VolumetricsPipeline`'s
allocation set was traced field-by-field against `destroy()`: 14 images
(12 per-FIF `FroxelSlot`s across 6 volume kinds + 2 singleton noise volumes),
12 `GpuBuffer`s, 2 descriptor pools/layouts, 4 pipeline objects, 3 samplers —
**all matched, no gaps**. `MemoryLocation` choices are correct
(`GpuOnly` for every image; `create_host_visible` / `create_host_readback` for
buffers per access pattern). Construction-failure rollback (`try_or_cleanup!`)
and both real call sites (`context/init.rs`, `context/resize.rs`) destroy
correctly on partial/failed init.

**Resize is safe.** `recreate_bloom_and_volumetrics` is a full
destroy-then-recreate, but it runs inside `recreate_swapchain_core`'s
`device_wait_idle`, so the TLAS-style use-after-destroy hazard (#1390) has no
analogue here — there is no incremental in-flight resize to guard. The #905
invariant (resize rebinds *both* volumetric and composite descriptors) is not
merely intact but **hardened beyond the original claim**:
`recreate_composite_and_egui` independently re-checks `self.volumetrics` and
hard-fails rather than keeping a stale binding, and both `recreate_swapchain`
call sites treat `Err` as fatal (`event_loop.exit()`), so no frame is ever
drawn against a partially-recreated state. The `DEFAULT_VOLUME_FAR` /
`None`-fallback code is genuinely defensive, not a live bug.

**Dispatch-skip path allocates nothing** on either branch
(`requires_dispatch` / `record_neutral_frame`), so repeated skips (interior
cells, no sun) cannot leak.

**Drop ordering is a non-question.** `VulkanContext`'s field declaration order
is irrelevant for volumetrics because `Drop` is hand-written and fully
explicit; Rust's automatic field-drop glue runs afterwards against
already-emptied state. `VolumetricsPipeline` correctly stays *inside* the
`Some(allocator)`-guarded `destroy_allocator_owned_resources` block (it owns
allocator-backed resources), unlike the genuinely allocator-independent
subsystems hoisted out by #1483.

**GPU-struct lockstep.** `VolumetricsParams` (the volumetrics-private UBO) is
in full field-by-field Rust↔GLSL agreement across all 15 fields, including
`wind_params`/`wind_gust` added this week. Critically, its pin
(`volumetrics_ubo_sizes_match_host_structs_in_every_shader`, `vulkan/reflect.rs`)
calls `size_of::<VolumetricsParams>()` **dynamically** and reflects the
committed `.spv` bytes, so a field added without recompiling the shader fails
automatically. This is the pattern the CRITICAL finding's struct lacks.

---

## Findings

### CRITICAL

#### REN-2026-09-05-D2-01: `volumetrics_inject.comp`'s `GpuBoundaryInstance` is a stale 128-byte mirror of the now-160-byte `GpuInstance`, corrupting every boundary-geometry read past instance index 0

- **Severity**: CRITICAL (SSBO index mismatch — the severity floor for this
  dimension per `_audit-severity.md`)
- **Dimension**: SSBO/Indexing (volumetrics × SSBO × ray queries)
- **Location**: `crates/renderer/shaders/volumetrics_inject.comp` —
  `struct GpuBoundaryInstance` and its `BoundaryInstanceBuffer` (binding 19)
  declaration; consumed by `rigidBoundaryNormal`, queried from
  `combustionPathBlocked`. Bound by
  `VolumetricsPipeline::write_boundary_geometry`
  (`crates/renderer/src/vulkan/volumetrics.rs`) from the call site in
  `record_volumetrics_pass` (`crates/renderer/src/vulkan/context/post_passes.rs`).
  Ground truth: `GpuInstance` in
  `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs`. The guard that does
  **not** cover it: `gpu_instance_glsl_copies_stay_in_lockstep`
  (`crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs`).
- **Status**: **NEW.** Not examined by the last full sweep —
  `AUDIT_RENDERER_2026-08-30.md` scoped Dimension 2's entry points to
  `triangle.frag` + includes and `water.frag`, explicitly logging
  `volumetrics_inject.comp`'s ray queries under "not examined by this
  dimension". No matching open issue (searched `volumetrics`, `boundary`,
  `GpuBoundaryInstance`, `SSBO`, `stride`).
- **Description**: `write_boundary_geometry` binds binding 19 directly to the
  **same** `vk::Buffer` the main scene uses for its per-frame `GpuInstance[]`
  SSBO (`self.scene_buffers.instance_buffers()[frame].buffer` — confirmed the
  only non-canonical binder of that buffer in the whole renderer), and
  `rigidBoundaryNormal` indexes it with the `instanceIndex` returned by
  `rayQueryGetIntersectionInstanceCustomIndexEXT` on a committed hit against
  the same TLAS the rest of the renderer builds. The shader therefore assumes
  `boundaryInstances[i]` and the raster/RT pass's `instances[i]` address the
  same record at the same byte offset. They do not.

  `GpuBoundaryInstance` ends at `uint surfaceId` (offset 108) followed by a
  single `uvec4 _boundaryTail` — total **128 B** std430 stride. Its own
  comment still asserts the now-false invariant: *"preserves the exact
  128-byte std430 stride of the canonical Rust `GpuInstance`."* The canonical
  Rust struct is **160 B**: after `surfaceId` it carries
  `skinned_vertex_address` (112), `_reserved` (120 → 128, the old end), then
  `morph_delta_address` (128), `morph_weight_address` (136),
  `morph_target_count` (144) and three reserved `u32`s (148 → 160), pinned by
  `gpu_instance_is_160_bytes_std430_compatible`.

  #3231 (`5f4dea46`, 2026-08-23) grew the real struct by exactly the 32 bytes
  `_boundaryTail` never gained. The boundary-geometry feature (`715b9230`,
  2026-08-18) predates that growth by five days — verified via
  `git merge-base --is-ancestor 715b9230 5f4dea46` — so this mirror was
  correct when written and has been silently wrong for the ~13 days since.

  **Why no test caught it**: `gpu_instance_glsl_copies_stay_in_lockstep`'s
  `SOURCES` is a hardcoded 5-file list (`include/bindings.glsl`,
  `triangle.vert`, `ui.vert`, `water.vert`, `caustic_splat.comp`) that does not
  include `volumetrics_inject.comp`. Its companion completeness guard
  `assert_mirror_list_is_complete` discovers mirrors by searching for the
  literal declaration `"struct GpuInstance"` — which does not match
  `"struct GpuBoundaryInstance"` as a substring, so the discovery half cannot
  see it either. (`volumetrics_inject.comp` *is* in the SOURCES of
  `gpu_light_glsl_copies_stay_in_lockstep`, but that test pins `struct
  GpuLight`, an unrelated struct.) This is a sixth mirror sitting entirely
  outside the tracked set, not a symptom of a wider pattern.

  Bitterly ironic: the Rust-side doc comment on the `_reserved2a/b/c` padding
  already documents this exact failure mode from when the team hit it *inside*
  the five tracked mirrors during #3231's own development — *"Confirmed the
  hard way (#3231): this exact substitution produced a GPU device-lost hang
  with zero validation-layer diagnostic — every instance past the first read at
  the wrong offset."* That lesson did not propagate to the untracked mirror.
- **Evidence**: Byte-by-byte std430 offset comparison of both live
  declarations (independently re-verified by the orchestrator, not taken on the
  sub-agent's word): GLSL `mat4 model` (0..64), seven `uint`/`float` scalars
  and three albedo floats (64..108), `uint surfaceId` (108), `uvec4
  _boundaryTail` (112..128) → 128 B, already 16-aligned so no tail padding.
  Rust: 160 B per `gpu_instance_is_160_bytes_std430_compatible`. Binder
  confirmed by `grep -rn "instance_buffers()\[" crates/renderer/src/vulkan/` →
  a single hit in `post_passes.rs`.
- **Impact**: Scoped to the volumetrics combustion-transport boundary-collision
  feature — fire/explosion smoke advection near solid geometry, gated on
  `carriesCombustion(...)`, i.e. only while an active or lingering fire /
  explosion fog volume is actually transporting chemistry. It does **not**
  affect the main raster/RT shading path, whose `instances[]` reads go through
  the correctly-updated `bindings.glsl` copy. When triggered, for any TLAS hit
  whose `instance_custom_index != 0`, every recovered field (`boneOffset`,
  `vertexOffset`, `indexOffset`, `vertexCount`, `model`) is read from the wrong
  byte range, misaligned by 32 bytes per unit of instance index.

  **The failure mode is silently wrong data, not an OOB access or device
  loss**: the affected fields feed bounds checks and a triangle-normal lookup
  rather than a raw pointer dereference, the descriptor's bound range is the
  full (larger) instance buffer, and `instanceIndex` only ever originates from
  a real TLAS hit. Garbage `boneOffset` frequently trips the "assume skinned,
  bail to conservative normal" path; when garbage values instead pass the loose
  bounds checks, `rigidBoundaryNormal` returns a plausible-looking but wrong
  triangle normal sourced from an unrelated instance's geometry. Visible
  effect: fire/smoke plumes near architecture tunnel through walls that should
  block them, get blocked by walls that are not there, or deflect off the wrong
  surface normal — for every instance except whichever entity happens to sort
  to draw index 0 that frame.
- **Related**: #3231 (introduced the 128→160 B growth this mirror missed),
  `715b9230` (introduced the now-stale mirror), #2748 /
  `REN-D3-2026-08-12-01` (the lockstep test this mirror falls outside of).
- **Suggested Fix**:
  1. Widen `_boundaryTail` from one `uvec4` (16 B) to the full 48 B the real
     struct now carries past `surfaceId` — three `uvec4` fields — restoring
     byte-identical 160 B stride. Follow `gpu_types.rs`'s own discipline of
     deliberately avoiding any type whose std430 alignment could silently
     drift.
  2. Correct the now-false "128-byte" claim in the struct's comment.
  3. **Close the structural gap so this cannot drift a third time.** Either
     teach `gpu_instance_glsl_copies_stay_in_lockstep` to accept a deliberate
     prefix-mirror under a different struct name and add
     `volumetrics_inject.comp` to `SOURCES`, or — cheaper and sufficient,
     since this mirror only needs to agree on *stride*, not on carrying every
     named field — add a narrow sibling test parsing `GpuBoundaryInstance`'s
     total std430 size and asserting it equals `size_of::<GpuInstance>()`.

---

### HIGH

None.

### MEDIUM

None.

### LOW

#### REN-2026-09-05-DOC-01: `shader-pipeline.md`'s `volumetrics_inject.comp` binding table documents 12 bindings; the live shader declares 24

- **Severity**: LOW (authoritative-doc divergence, no runtime effect)
- **Dimension**: Volumetrics / SSBO-Indexing (doc-rot)
- **Status**: **NEW.** *Reported independently by both Dimension 2
  (`REN-D2-VOL-02`) and Dimension 16 (`DIM16-2026-09-05-01`) — merged here.*
  Precedent for the class: `REN-2026-08-30-D2-03` (same doc, adjacent Set-1
  table, since fixed).
- **Location**: `docs/engine/shader-pipeline.md`, the table headed
  ``​`volumetrics_inject.comp` (12 bindings, widened by #2228/#2231's fog-volume
  work…)``. Ground truth: `crates/renderer/shaders/volumetrics_inject.comp`,
  bindings 0–23 inclusive.
- **Description**: The table stops at binding 11 (`detailDensityNoise`). The
  live shader declares twelve more, entirely undocumented: 12
  (`emissionHistory`), 13 (`previousEmissionHistory`), 14–17
  (`combustionState` / `previousCombustionState` / `combustionDynamics` /
  `previousCombustionDynamics`), 18 (`CombustionLightMomentBuffer`), 19–21
  (`BoundaryInstanceBuffer` / `BoundaryVertexBuffer` / `BoundaryIndexBuffer`),
  22–23 (`combustionOptical` / `previousCombustionOptical`) — exactly doubling
  the documented count. The doc's own hedge ("verify against the source before
  relying on this table for a new binding") proved warranted, but the drift has
  gone well past what that caveat implies.
- **Evidence**:
  `grep -oE "binding = [0-9]+" crates/renderer/shaders/volumetrics_inject.comp | grep -oE "[0-9]+" | sort -nu`
  → `0`…`23` (24 values). The doc table's last row is binding 11.
- **Impact**: Audit-methodology only, but higher-stakes than a typical size
  mislabel — this is the doc every audit is told to prefer over re-deriving
  facts from source. An auditor trusting its binding *count* could reasonably
  conclude the boundary-geometry read path does not exist rather than checking
  it. **That is close to what actually happened**: bindings 19–21 are where
  this run's CRITICAL finding lives, and they are absent from the table
  despite the audit brief's framing implying they were in it.
- **Related**: #2228 / #2231 (the fog-volume work the table says it was last
  updated for), `REN-2026-08-30-D2-03`, #2314 / `TD3-206` (the reason this
  table exists at all — it drifted once before and got a table for it; the
  table has now drifted again).
- **Suggested Fix**: Regenerate the table's rows from the live `layout(...)`
  declarations. Longer-term, a table that has now drifted twice needs a
  generation script or a drift-pinning test — the pattern
  `froxel_grid_cost_matches_the_memory_budget_doc` already uses (`include_str!`
  the doc, assert substrings) applied to the shader's actual binding count
  would have caught this before it reached double the documented size.

#### REN-2026-09-05-DOC-02: `volumetrics_inject.comp` documents `sun_color.a` as "unused" while the same file reads it as the cluster-far basis

- **Severity**: LOW
- **Dimension**: Volumetrics (in-code doc-rot)
- **Status**: **NEW** (Dim 16, `DIM16-2026-09-05-02`).
- **Location**: `crates/renderer/shaders/volumetrics_inject.comp` — the
  `VolumetricsParams` UBO block's `sun_color` declaration comment, vs the
  `clusterFar` computation in the local-light loop ~2570 lines later in the
  same file.
- **Description**: The UBO block comment reads
  `// rgb = sun radiance (already multiplied by intensity), a = unused`. The
  local-light clustered-lookup code then reads `params.sun_color.a` as the
  basis for `clusterFar`
  (`float clusterFar = params.sun_color.a > 1.0 ? max(params.sun_color.a, CLUSTER_FAR_FLOOR) : CLUSTER_FAR_FALLBACK;`),
  with its own adjacent comment explaining why — the cell's fog-far, plumbed
  to match `screen.w` as the identical basis for the exponential depth-slice
  distribution. The Rust-side struct doc in `volumetrics.rs` correctly
  documents `.a` as the cell's XCLL fog-far distance. **Only the GLSL
  declaration-site comment is stale.**
- **Impact**: None on rendering — the code path is correct and matches both
  the Rust doc and the later in-file comment. The risk is a maintainer reading
  the UBO block top-to-bottom, trusting "a = unused", and repurposing the
  field.
- **Suggested Fix**: Update the declaration comment to match the Rust-side doc
  and the actual use, e.g. `a = the cell's fog-far distance (see the
  local-light loop below; matches screen.w's cluster basis)`.

#### REN-2026-09-05-D5-01: `memory-budget.md`'s RT-Denoiser section intro is self-contradicted by its own subsections

- **Severity**: LOW
- **Dimension**: Memory/Lifecycle (doc-rot)
- **Status**: **NEW** (Dim 5, `MEM-5-01`). Not in the 65 open issues; not in
  the 2026-08-30 or 2026-09-04 reports.
- **Location**: `docs/engine/memory-budget.md` — the intro paragraph of
  `## RT-Denoiser & Post-Process Screen-Sized Resources`, vs its own
  `### Volumetrics (M55)` and `### Glass + Water Caustics` subsections in the
  same section.
- **Description**: The section opens by asserting that every resource in it
  "had **no ledger entry here** until this sweep (#1872 … grep confirmed zero
  mentions of SVGF, Bloom, SSAO, TAA, Volumetrics, Water, or Caustic anywhere
  on this page)." That was true when #1872 landed. It is no longer true of the
  page's current content: the same section now contains a detailed
  `Volumetrics (M55)` subsection and a `Glass + Water Caustics` subsection, and
  the VRAM roll-up table has its own Volumetrics row.
- **Evidence**: `grep -n "^### " docs/engine/memory-budget.md` shows both
  subsections nested under the very heading whose intro claims zero mentions of
  either.
- **Impact**: Doc-trust only, no runtime effect — but it is the specific
  sentence that misdirected this audit run's own brief into asserting the
  volumetrics ledger entry was missing, so it will likely misdirect the next
  reader too.
- **Suggested Fix**: Reword the parenthetical to past tense, or simply drop the
  "grep confirmed zero mentions… anywhere on this page" clause, which is no
  longer a true statement about the page it sits on.

#### REN-2026-09-05-D1-01: `STATIC_BLAS_FLAGS` doc + `build_blas_batched` comment still name the deleted single-shot `build_blas`

- **Severity**: LOW
- **Dimension**: AS Correctness (doc-rot)
- **Status**: **EXISTING — already open as #3824** (`REN-WD-D1-01`, confirmed
  in the issue cache). Listed here only to record that the premise still holds
  at HEAD.
- **Location**: `crates/renderer/src/vulkan/acceleration/constants.rs`
  (`STATIC_BLAS_FLAGS` docstring),
  `crates/renderer/src/vulkan/acceleration/blas_static.rs` (the pre-batch
  eviction comment inside `build_blas_batched`).
- **Evidence**: `grep -rn "fn build_blas" crates/renderer/src/` still returns
  only `blas_static.rs::build_blas_batched` and its `context/resources.rs`
  wrapper; neither comment has been updated since the 2026-08-30 sweep found
  them.
- **Impact**: Documentation only.
- **Related**: #2914 (the deletion), #3824.
- **Suggested Fix**: See #3824.

#### REN-2026-09-05-D1-02: `TlasIntegritySnapshot` remains a dead accessor with no consumer anywhere in the workspace

- **Severity**: LOW
- **Dimension**: AS Correctness (observability)
- **Status**: **EXISTING but UNFILED.** Reported as `REN-2026-08-30-D1-01` in
  `AUDIT_RENDERER_2026-08-30.md` and never converted to a GitHub issue — a
  live keyword search of the issue cache (`integrity`, `snapshot`, `1228`)
  returns nothing. Re-verified the premise still holds at HEAD.
- **Location**: `crates/renderer/src/vulkan/acceleration/mod.rs`
  (`TlasIntegritySnapshot`, the `tlas_integrity` field),
  `crates/renderer/src/vulkan/acceleration/tlas.rs` (`integrity_snapshot`
  accessor; the write in `build_tlas_instances`).
- **Evidence**: `grep -rn "tlas_integrity\|TlasIntegritySnapshot"` still
  returns the same 5 hits (definition, field decl, `Default` init, one write,
  one accessor); `integrity_snapshot()` still has zero call sites outside its
  own definition.
- **Impact**: A steady-state RT-membership regression (stuck LRU eviction,
  failing skinned first-sight build) is observable only via a once-per-second
  rate-limited `log::warn!`; the snapshot that would make it a positive
  per-frame assertion is computed and thrown away.
- **Related**: #1228 (the underlying telemetry gap),
  `AUDIT_RENDERER_2026-08-30.md` `REN-2026-08-30-D1-01`.
- **Suggested Fix**: Wire a warmup-guarded `debug_assert_eq!` at the end of
  `build_tlas`, or surface the fields through an existing debug command family,
  and stop leaving a `pub` accessor with no reader. **Recommended as the one
  finding from this run that should actually be filed** — it has now survived
  two sweeps without an issue number.

---

## Prioritized Fix Order

Correctness → safety → optimization.

1. **`REN-2026-09-05-D2-01` (CRITICAL)** — widen `GpuBoundaryInstance` to 160 B
   and fix its comment. This is a live data-corruption bug in shipped code;
   everything else in this report is documentation. Do the struct widening and
   the comment together — they are a two-line change in one file.
2. **`REN-2026-09-05-D2-01` step 3 (the structural guard)** — add the
   stride-only sibling test. Separable from the fix itself, and the more
   valuable half: without it, the seventh mirror will drift the same way. This
   is the *only* code change in this report beyond the fix itself.
3. **`REN-2026-09-05-DOC-01`** — regenerate the `volumetrics_inject.comp`
   binding table. Highest-leverage of the doc fixes, because this table's
   incompleteness is causally connected to how long the CRITICAL survived.
4. **`REN-2026-09-05-D1-02`** — file the issue for the dead
   `TlasIntegritySnapshot` accessor (two sweeps without one).
5. **`REN-2026-09-05-D5-01`**, **`REN-2026-09-05-DOC-02`** — one-sentence
   doc/comment corrections, batchable with any nearby work.
6. **`REN-2026-09-05-D1-01`** — already tracked as #3824; no new action.

---

## Needs-RenderDoc

**Nothing in this report requires a GPU capture to act on.** No render-pass,
pipeline, or barrier edit is proposed anywhere, per the standing
no-speculative-Vulkan-fixes rule. Two items are recorded as capture-only *if
ever re-questioned*:

- The `rayQueryProceedEXT` single-call-vs-loop question (Dim 2) was settled
  from documented extension semantics, not a capture. If it is ever in doubt
  again, the definitive check on this specific hardware/driver is a RenderDoc
  capture comparing shadow results with and without the loop.
- `append_combustion_surface_lights`'s host readback of the combustion-light
  SSBO (binding 18) was not independently fence-traced by Dim 16; the
  function's own doc states the precondition and its single call site sits in
  the standard per-frame fence-wait scope. A full trace is sync/concurrency
  territory (`/audit-concurrency`), not this run's.

---

## Investigated and Disproved (recorded so they are not re-litigated)

Four plausible-looking issues were chased down and ruled out. They are
documented here deliberately — each cost real investigation time this run, and
each would look like a finding to the next sweep:

- **`rayQueryProceedEXT` called once instead of looped** (3 sites) —
  spec-sanctioned under `gl_RayFlagsOpaqueEXT`, which all three set. Correct.
- **`FogProfile::OilExplosion` has no shader-side special-casing** — by
  design. `FogProfile::is_oil_explosion()` is defined as
  `matches!(self, Self::Explosion | Self::OilExplosion)`; the two are
  deliberately two names for the same conventional-fireball behavior, and only
  `NuclearExplosion` diverges. The shader mirrors this correctly.
- **Wind-direction axis mismatch** between the new volumetrics wind consumer
  (`atmosphericWindVelocity`, mapping `wind_params.xy` → `vec3(x, 0, y)`) and
  the pre-existing foliage consumer (`apply_speedtree_wind` in
  `byroredux/src/systems/billboard.rs`) — confirmed identical X/Z convention.
- **Dangling composite descriptor when `self.volumetrics` is `None` after a
  failed resize** — unreachable by construction; any such failure is fatal via
  `event_loop.exit()` before another frame draws.

---

## Regression Guards Confirmed (previously reported / fixed, re-verified live)

- **#3611** far-plane triple-copy (`REN-2026-08-30-D16-05`) — fixed by
  `81c63681`; `VolumetricsConfig::DEFAULT` is now the single literal and
  `VOLUME_FAR` is test-pinned against it.
- **`B-DIM16-03`** bloom upsample DC-gain comment — now correctly states
  "~5× peak at up[0]"; fixed since the 2026-07-14 report.
- **`REN-2026-08-30-D16-01/02/04`** — doc-rot batch fixed by `17b744b5`;
  froxel divisor 8 / 160×90×64 quoted consistently across `volumetrics.rs` and
  both shaders.
- **`ae71ace9`** — `froxelLightAtten` now recovers range through the shared
  `LEGACY_LIGHT_CULL_RANGE_MULTIPLIER`; all three new pinning tests pass.
- **#905** — resize rebinds both volumetric and composite descriptors; intact
  and hardened (hard-fail, not silent stale binding).
- **#928 / `VOLUMETRIC_OUTPUT_CONSUMED`** — still `true`, still gates the
  dispatch in both directions; no route where the dispatch runs but composite
  ignores the result, or vice versa.
- **#1928** — `VolumetricsParams.render_origin.w` is a claimed slot
  (`is_exterior`), consistent on both sides. Not a free slot; not re-flagged.
- **#1406 / #1477** — `AllocatorResource` remove-before-`renderer.take()`
  intact in both `App::drop` and `App::shutdown`.
- **#1782** — deferred BLAS-scratch destruction via `pending_destroy_scratch`
  intact; `build_skinned_blas_batched_on_cmd`'s deliberate immediate-destroy
  exception untouched.
- **#a476b256** — deferred BLAS destruction (`pending_destroy_blas` +
  `DEFAULT_COUNTDOWN`) intact at both `drop_blas` and `evict_unused_blas`.
- **#3298 / #3372 / #3443** — all four geometry-compaction regression tests
  exist under their documented names.
- **#ae285062**, **#883f57cd**, **#d523b9b3** — BC1 punch-through alpha gate,
  stable surface-ID tag, and ReSTIR spatial normal cone all verified unchanged.

---

## Tracked Elsewhere / Not Re-Filed

- **The `--grid` half of #1793** — shared `frame_counter` false-eviction
  hazard, gated behind `static_blas_bytes > budget`, unreachable on the 12 GB
  dev card. Per the checklist's explicit instruction, only this half is
  recast; the recovery-path half is closed.
- **`TD1-078` / #2256** — `volumetrics.rs` crossed the 2000-LOC threshold and
  is now 3863 LOC, nearly double what the issue was filed against. Already
  tracked as tech debt; noted for whoever picks it up.
- **`REN-2026-08-30-D13-01` / #3572** — TAA bypasses volumetrics (and
  bloom/caustics/sky/indirect) in its resolve. A Dimension 13 finding that
  names volumetrics as one bypassed term, not a Dim 16 defect.
- **`REN-WD-D2-02` / #3825** — the `GLASS_RAY_BUDGET` checklist wording,
  already open from the 2026-09-04 run; re-confirmed still applicable.
- **#1390** — `tlas.rs`'s `device_wait_idle()`-before-`allocator.free()`; not
  re-read line-by-line this run, no new evidence either way.

---

## Coverage Notes / Disclosed Gaps

- **Dim 2** did not audit `volumetrics_inject.comp`'s combustion chemistry and
  turbulence math (semi-Lagrangian advection weighting, curl-noise domain warp,
  blackbody radiance calibration) beyond confirming the `combustionPathBlocked`
  call sites and their gating — a numerical audit of that math is outside
  SSBO-indexing scope. Glass/IOR mechanics (Frisvad basis, window-portal
  demote, `DBG_VIZ_GLASS_PASSTHRU`) were spot-checked, not re-proven from
  scratch; no delta was found since the last full sweep.
  `caustic_splat.comp`'s own ray queries remain unexamined by any recent Dim-2
  pass.
- **Dim 5** did not re-verify `tlas.rs`'s `device_wait_idle()` placement
  (#1390), BGSM/failed-path half-eviction (#1430), the geometry-compaction
  two-phase publish *logic* (test existence confirmed, assertions not read),
  or general non-volumetrics `MemoryLocation` correctness across the renderer.
- **Dim 16** did not independently re-verify the fence-wait discipline around
  `append_combustion_surface_lights`'s host readback (see Needs-RenderDoc), nor
  re-derive bloom's per-FIF barrier folding from scratch (#931) — nothing has
  touched that code since the 2026-07-14 report verified it.
- **No GPU device was available** for this run. Every conclusion rests on
  source reading, `git` history, and the test suite.
