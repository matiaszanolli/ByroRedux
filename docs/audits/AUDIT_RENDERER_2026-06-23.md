# Renderer Audit — 2026-06-23

Deep audit of the Vulkan deferred + ray-traced renderer across all 21 skill
dimensions (AS correctness, SSBO/RT ray-query plumbing, GPU-struct layout,
sync/barriers, GPU memory/lifecycle, NIFAL material translation, material table,
denoiser/composite, GPU skinning, camera-relative precision, pipeline/render
pass, command-buffer recording, TAA, caustics, water, volumetrics/bloom, Disney
BSDF/soft shadows, sky/weather, tangent-space, debug/telemetry, Cornell harness).

**Depth**: deep (data-flow traced, invariants validated against
`docs/engine/shader-pipeline.md` + `docs/engine/memory-budget.md`).

## Executive Summary

The renderer is in **excellent** condition. After verifying every documented
regression guard against the live code, I found **no NEW CRITICAL, HIGH, or
MEDIUM correctness defect**. All 335 `byroredux-renderer` lib tests pass,
including every GPU-struct layout pin (`gpu_instance_is_112_bytes`,
`gpu_camera_is_336_bytes`, `gpu_material_size_is_300_bytes`,
`gpu_material_field_offsets_match_shader_contract`,
`triangle_frag_declares_six_color_outputs`,
`camera_ubo_size_matches_gpu_camera_in_every_shader`).

Findings by severity: **0 CRITICAL, 0 HIGH, 0 MEDIUM, 1 LOW (NEW)**.
The single prior renderer finding (REN-D10-01, soft-particle precision mix) is
**fixed** (commit `f0c81539`, #1642). The stale-comment findings from the
2026-06-16 audit (REN-D3-02/03, "7 color attachments") are **resolved** — no
stale 7-attachment / reservoir references remain in `pipeline.rs` or
`shader-pipeline.md`.

This audit is primarily a **regression-guard confirmation pass**: it documents
that the load-bearing invariants the codebase has accreted over ~70 prior
renderer audits all still hold after the M47.2 scripting work landed (which does
not touch the renderer).

## RT Pipeline Assessment

**BLAS/TLAS (Dim 1)** — clean.
- BLAS build geometry is correct: `R32G32B32_SFLOAT` at offset 0, `UINT32`
  index type, `OPAQUE` flag, `vertex_stride = size_of::<Vertex>()` in both the
  single-shot (`build_blas`) and batched (`build_blas_batched`) paths
  (`blas_static.rs`).
- Build-flag constants stable and split per buffer class (`STATIC_BLAS_FLAGS`,
  `SKINNED_BLAS_FLAGS`, `UPDATABLE_AS_FLAGS` in `constants.rs`); the deliberate
  `SKINNED_BLAS_FLAGS` = `FAST_BUILD | ALLOW_UPDATE` (R6a-prospector-regress) is
  intact and documented.
- The load-bearing AS/SSBO contract holds: `instance_custom_index_and_mask =
  Packed24_8::new(ssbo_idx, 0xFF)` in `tlas.rs`, guarded by a `debug_assert!`
  mirroring the `MAX_INSTANCES < (1 << 24)` const-assert in
  `scene_buffer/constants.rs`. `MAX_INSTANCES = 0x40000` stays ~64× under the
  24-bit ceiling.
- `TRIANGLE_FACING_CULL_DISABLE` is correctly gated on `draw_cmd.two_sided`
  (#416), not applied unconditionally.
- TLAS BUILD-vs-UPDATE keys on `last_blas_addresses` via `decide_use_update`,
  with the `instance_count != built_primitive_count` guard (VUID-03708) and the
  `last_blas_addresses.len() == instance_count` bookkeeping debug-assert (#914)
  both present.
- **Deferred BLAS destruction** (#a476b256) intact: `drop_blas` /
  `evict_unused_blas` (static) and the skinned drop route through
  `pending_destroy_blas` with `DEFAULT_COUNTDOWN`. Every immediate
  `destroy_acceleration_structure` call in `blas_static.rs` (lines ~354/658/813/932)
  is an **error/rollback path before any command buffer references the
  structure** — verified safe, not an eviction-site use-after-free.

**SSBO indexing & ray queries (Dim 2)** — clean.
- The instance-index contract is internally consistent: raster reads
  `instances[gl_InstanceIndex]` (where `firstInstance == ssbo_idx`); RT reads
  `instances[rayQueryGetIntersectionInstanceCustomIndexEXT(...)]` (the 24-bit
  custom index == `ssbo_idx`). Both resolve to the same SSBO entry.
- RT gating (`sceneFlags.x > 0.5`) checked before ray queries; glass refraction
  uses the Frisvad orthonormal basis (`math_common.glsl`), not the degenerate
  `cross(N, up)`.
- **BC1 punch-through alpha guard** (#ae285062 / #1653) intact end-to-end:
  `triangle.frag` pins `texColor.a = 1.0` when `INSTANCE_FLAG_DIFFUSE_ALPHA`
  (bit 8) is clear and no alpha test is active; the CPU bit is set in `draw.rs`
  from `texture_registry.handle_has_alpha`, which is `false` for `BC1_RGBA`
  (`format_has_alpha` excludes it, pinned by the `dds.rs` test).

**Ray-query safety / denoiser stability** — the firefly clamp in
`svgf_temporal.comp` runs **before** the `hasHistory` branch (REG-07 / #1639 /
#1481 invariant comment intact). Composite reassembly order is correct: caustic
(both accumulators, promoted to float before the add per #1575 to avoid u32
wrap) + indirect×albedo added to `combined`, bloom added pre-ACES, ACES, then
display-space fog. SSAO modulates the ambient/indirect term in the geometry
pass, not direct.

## GPU-Struct & Memory Assessment

**Layout pins (Dim 3)** — fully locked.
- `GpuInstance` declared in all **5 sites** (`include/bindings.glsl` +
  `triangle.vert` / `water.vert` / `ui.vert` / `caustic_splat.comp`),
  **byte-identical field order** across all of them.
- `gpu_material_field_offsets_match_shader_contract` asserts every named field
  offset through the Disney tail (`ior` @ 280, `subsurface` @ 284, `sheen` @ 288,
  `sheen_tint` @ 292, `anisotropic` @ 296); the GLSL `struct GpuMaterial` in
  `bindings.glsl` matches these offsets exactly.
- `GpuMaterial` Hash/Eq are byte-level over `as_bytes()`; pad fields
  (`_pad_id0`, `_pad_albedo`, …) are explicitly zeroed in the `Default` impls,
  so no uninit bytes feed the dedup hash.

**Sync (Dim 4)** — clean.
- `render_finished` is **per-swapchain-image**, indexed `render_finished[img]`
  at the submit site in `draw.rs` (548c1b69 revert intact; `sync.rs` recreates
  N-per-image on resize).
- AS-build **input** barrier uses `SHADER_READ` at the
  `ACCELERATION_STRUCTURE_BUILD_KHR` stage (#507945d8), not the wrong
  `ACCELERATION_STRUCTURE_READ_KHR`.
- egui pass: `loadOp = LOAD`, `initialLayout = PRESENT_SRC_KHR`, explicit
  `SUBPASS_EXTERNAL` incoming + outgoing dependencies (#1433).

**Memory/lifecycle (Dim 5)** — clean.
- The `AllocatorResource` is removed from the `World` **before**
  `VulkanContext::drop()` on *every* teardown path, including panic-unwind, via
  `impl Drop for App` (#1477 / #1640 / #1406). The ordering is structural and
  idempotent with the `CloseRequested` handler.

**Other dimensions** — Disney BSDF gate is `MAT_FLAG_PBR_BSDF` only with the
`deriveAxAy` anisotropic [0,1] clamp (#1254) and `dielectricF0FromIor` eta clamp
(#1248); `distributionGGXAniso` reduces to isotropic GGX at `ax == ay` (verified
algebraically). `VERTEX_STRIDE_FLOATS = 25` is imported (not hardcoded) and
asserted against `size_of::<Vertex>()` in `skin_compute.rs`. `VOLUMETRIC_OUTPUT_CONSUMED`
is `false` and the dispatch is gated on it in `draw.rs` (no
dispatched-then-ignored work). Tangent handedness reads the Bethesda bitangent
half into `Vertex.tangent.xyz` (#786). Mesh-ID overflow is a warn-once + clamp,
not a `debug_assert!` (#956). The Cornell harness, soft-particle depth fade
(now render-origin-relative), and water Fresnel path are all intact.

## Findings

### REN-2026-06-23-L01: `GpuMaterial::glass()` preset unused; doc comment & issue #1627 title now mutually stale
- **Severity**: LOW
- **Dimension**: Material Table / Tech-Debt
- **Location**: `crates/renderer/src/vulkan/material.rs` (`GpuMaterial::glass`)
- **Status**: Existing: #1627 (premise drifted — see below)
- **Description**: `GpuMaterial::glass()` has zero non-test call sites
  (`grep` for `GpuMaterial::glass` / `.glass()` outside the `glass_matches_hyperion_table`
  test returns nothing). The constructor's doc comment was updated to read
  "tracked by #1627; #1248 closed", so it **no longer "names a CLOSED issue"** —
  but the open tracking issue #1627's *title* still asserts it does
  ("TD5-002: ...transmission TODO names a CLOSED issue; preset unused"). The
  "preset unused" half is still accurate; the "names a CLOSED issue" half is now
  stale on both the issue title and would mislead a future reader.
- **Evidence**: Comment at `material.rs:608-609`: *"not yet plumbed into our
  GpuMaterial — left as a TODO for when the transmission lobe lands (tracked by
  #1627; #1248 closed)."* No production caller of `glass()`.
- **Impact**: None functional — the preset is dead code pending the transmission
  lobe. Pure doc/issue hygiene: the issue title misrepresents the current
  comment state.
- **Related**: #1627 (open), #1248 (closed).
- **Suggested Fix**: When the transmission lobe lands, wire `glass()` (or delete
  it). Until then, update the #1627 issue title to drop the "names a CLOSED
  issue" clause so it reads simply as "GpuMaterial::glass() preset unused pending
  transmission lobe". No code change required.

## Prioritized Fix Order

1. **Correctness** — none required.
2. **Safety** — none required.
3. **Optimization / hygiene** — REN-2026-06-23-L01 (retitle #1627; optionally
   prune `glass()` if the transmission lobe is deferred indefinitely).

## Needs-RenderDoc

None. No sync/barrier/render-pass change is proposed in this audit (per the
no-speculative-Vulkan-changes guidance). All sync invariants checked were
confirmed *present and correct* against their documented form, not modified.

## Disproved / Confirmed-Fixed (not reported)

- **REN-D10-01** (soft-particle depth fade mixes relative/absolute precision,
  2026-06-16) — **fixed** in `f0c81539` (#1642). `triangle.frag:644-656` now
  rebases the camera into render-origin-relative space (`camRel = cameraPos -
  renderOrigin`) before the along-ray gap, mirroring `ssao.comp`.
- **REN-D3-02 / REN-D3-03** (stale "7 color attachments" / removed reservoir
  G-buffer references, 2026-06-16) — **resolved**; no stale references remain.
- Cornell metalness-vs-lighting confound and glass-stipple / IGN refraction
  jitter — known open observations per memory, not re-reported as new.
