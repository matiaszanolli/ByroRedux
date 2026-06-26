# Renderer Audit — 2026-06-26

Deep audit of the Vulkan deferred + ray-traced renderer across all 21 skill
dimensions (AS correctness, SSBO/RT ray-query plumbing, GPU-struct layout,
sync/barriers, GPU memory/lifecycle, NIFAL material translation, material table,
denoiser/composite, GPU skinning, camera-relative precision, pipeline/render
pass, command-buffer recording, TAA, caustics, water, volumetrics/bloom, Disney
BSDF/soft shadows, sky/weather, tangent-space, debug/telemetry, Cornell harness).

**Depth**: deep (data-flow traced; every documented regression guard
re-verified by symbol against live code, not inherited from the prior report).
Invariants checked against `docs/engine/shader-pipeline.md` +
`docs/engine/memory-budget.md`.

## Executive Summary

The renderer is in **excellent** condition. Across all 21 dimensions there are
**0 NEW CRITICAL, 0 HIGH, 0 MEDIUM, 0 LOW correctness defects**. The full
`byroredux-renderer` lib suite passes (**335 passed, 0 failed**), including every
GPU-struct layout pin (`gpu_instance_is_112_bytes`, `gpu_camera_is_336_bytes`,
`gpu_material_size_is_300_bytes`, `gpu_material_field_offsets_match_shader_contract`,
`gpu_material_glsl_field_order_matches_rust_struct`,
`triangle_frag_declares_six_color_outputs`).

Findings: **0 CRITICAL / 0 HIGH / 0 MEDIUM / 0 LOW**, plus **2 INFO**
(audit-skill checklist-wording drift, no code defect) and **1 tracker-hygiene
note** (issue #1627 left OPEN after its code fix landed).

This audit is, by design, a **regression-guard confirmation pass**. The renderer
is structurally unchanged since the clean 2026-06-23 report: `git log --since`
shows the *only* renderer file touched is `crates/renderer/src/vulkan/material.rs`,
and that change (commit `eb71bcb9`, #1627) is a **doc-comment-only** rewrite of
the `presets::glass()` / `presets::car_paint()` constructors — no field, offset,
flag, or layout change. The prior report's sole LOW finding
(REN-2026-06-23-L01) is **resolved in code**. The value added here is independent
re-verification of each load-bearing invariant by symbol, which surfaced two
stale items in the audit skill's own checklist and confirmed the #1627 GitHub
issue was never closed.

## RT Pipeline Assessment

**BLAS/TLAS (Dim 1)** — clean. BLAS geometry is `R32G32B32_SFLOAT` @ offset 0,
`UINT32` index, `OPAQUE`, `vertex_stride = size_of::<Vertex>()` in both the
single-shot and batched build paths. The three build-flag constants
(`STATIC_BLAS_FLAGS`, `SKINNED_BLAS_FLAGS`, `UPDATABLE_AS_FLAGS`) match
memory-budget.md, including the deliberate `SKINNED_BLAS_FLAGS = FAST_BUILD |
ALLOW_UPDATE` (R6a-prospector-regress). The load-bearing AS/SSBO contract holds:
`instance_custom_index == ssbo_idx` via `Packed24_8::new(ssbo_idx, 0xFF)`, doubly
guarded by a per-call `debug_assert!` and the static `const_assert MAX_INSTANCES
< (1 << 24)`. `decide_use_update` keys on `last_blas_addresses` with the
`instance_count != built_primitive_count` demote (VUID-03708).
`column_major_to_vk_transform` is test-pinned. Deferred BLAS destruction
(#a476b256) routes drop/evict through `pending_destroy_blas`; every immediate
`destroy_acceleration_structure` is an error-rollback or post-fence-retire path,
not an eviction-site use-after-free.

**SSBO indexing & ray queries (Dim 2)** — clean. RT reads
`rayQueryGetIntersectionInstanceCustomIndexEXT` → `instances[]` →
`materials[materialId]`, round-tripping with the Dim-1 CPU index; raster reads
`instances[gl_InstanceIndex]` (`firstInstance == ssbo_idx`) — both resolve to the
same entry. RT gated on `sceneFlags.x > 0.5`; glass refraction uses the Frisvad
orthonormal basis (not the degenerate `cross(N, up)`); `GLASS_RAY_BUDGET`
overshoot-by-design confirmed with **no CPU read** of the counter. The ReSTIR-DI
spatial pass rejects neighbours on the 25° geometric-normal cone
(`SPATIAL_NORMAL_COS = 0.906`) packed into the reservoir `pad0`. The BC1
punch-through alpha guard (#ae285062) is intact end-to-end: `triangle.frag` pins
`texColor.a = 1.0` unless `INSTANCE_FLAG_DIFFUSE_ALPHA`, and `format_has_alpha`
excludes `BC1_RGBA`.

**Ray-query safety / denoiser stability (Dim 8)** — clean. The SVGF firefly
clamp runs **before** the `hasHistory` branch (REG-07/#1639/#1481) and the
clamped value flows into the no-history path. SVGF motion convention
(`prevUV = uv - motion`) matches the `triangle.frag` motion-vector output (HIGH
"wrong motion vectors" floor not tripped). Composite reassembly order is correct:
`direct + indirect×albedo + caustic` (both caustic accumulators float-promoted
before the add per #1575), bloom added **pre-ACES**, ACES after reassembly,
display-space fog on direct only.

## GPU-Struct & Memory Assessment

**Layout pins (Dim 3)** — fully locked. `GpuInstance` is declared at all 5 sites
(`include/bindings.glsl` + `triangle.vert` / `ui.vert` / `water.vert` /
`caustic_splat.comp`), byte-identical in field order (the recurring
`ui.vert`/`water.vert` trap is clean). `GpuMaterial` is declared once in
`bindings.glsl`, is scalar-only with byte-level Hash/Eq over explicitly-zeroed
pads; `gpu_material_field_offsets_match_shader_contract` (the within-vec4 guard)
and `gpu_material_glsl_field_order_matches_rust_struct` (#1657) both pass. Flag
constants are single-sourced in `shader_constants_data.rs`; capacities match
memory-budget.md with the over-cap → id-0 + warn-once path intact.

**Sync (Dim 4)** — clean. `render_finished` is per-swapchain-image, indexed
`render_finished[img]` at the submit site in `context/draw.rs` (548c1b69 revert
vs VUID-...-00067). The AS-build **input** barrier uses `SHADER_READ` at
`ACCELERATION_STRUCTURE_BUILD_KHR` at both sites (TLAS instance-copy in
`tlas.rs`, skinned compute-write in `draw.rs`), while the BLAS→TLAS *structure*
read legitimately keeps `ACCELERATION_STRUCTURE_READ_KHR` (#507945d8/#1436). The
egui pass supplies its own explicit `SUBPASS_EXTERNAL` incoming + outgoing
dependency (#1433). No speculative barrier change proposed.

**Memory/lifecycle (Dim 5)** — clean. No per-frame leak, no destroy-after-device,
no immediate free at a deferred-destroy site. `AllocatorResource` is removed from
the `World` before `renderer.take()` in `impl Drop for App` — structural on the
panic-unwind path, not just `CloseRequested` (#1406/#1477/#1640).
`VulkanContext::Drop` does `device_wait_idle` first, splits allocator-independent
vs allocator-guarded teardown (#1483), destroys every named subsystem in reverse
order, and drops the allocator via `Arc::try_unwrap` before `destroy_device` with
the #665 leak-not-UAF fallback. TLAS resize runs `device_wait_idle` before
freeing the old allocation (#1390). Deferred-destroy countdown =
`MAX_FRAMES_IN_FLIGHT`, ticked after the fence wait. Shrink fns are slack-gated
(16 MB / 256 KB matching memory-budget.md) and run at cell-unload.

**Material translation & table (Dims 6/7)** — clean, with extra scrutiny on the
one changed file. `translate_material` has exactly the two sanctioned callers
(`scene/nif_loader.rs`, `cell_loader/spawn.rs`); the only other `Material {…}`
literals are test fixtures and the Cornell reference scene, which route through
the same `MaterialTable` path. `Material::metalness`/`roughness` are plain
resolved f32, `resolve_pbr` is idempotent (`resolve_pbr_is_idempotent`). No
per-game branch sits between `Material` and `MaterialTable::intern`. `intern`
gives stable per-frame ids, over-cap returns id 0 + warn-once (#797), upload
truncates to `min(len, MAX_MATERIALS)`. WaterShaderProperty / bare
BSShaderProperty produce distinct entries (#1243/#1244); BGSM smoothness is
normalized once (#1241).

**Other dimensions (9–21)** — all clean and confirmed by symbol: skinning
`VERTEX_STRIDE_FLOATS = 25` imported (not hardcoded) and asserted against
`size_of::<Vertex>()`; the two precision conventions (raster relative / RT
absolute) never mixed and `RT_ABSOLUTE_PRECISION_CEILING = 2^20` debug-asserted;
six-attachment G-buffer with no stale "7 color"/reservoir text; TAA Halton(2,3)
jitter with un-jittered motion vectors and parked-camera α-floor (#1497);
caustic added to **direct** only (double-count guard); water Fresnel F0 ≈ 0.02
(not glass IOR); volumetrics gated by `VOLUMETRIC_OUTPUT_CONSUMED` (no
dispatched-then-ignored work); bloom `B10G11R11_UFLOAT` throughout, added
pre-ACES, sourced from pre-TAA HDR; Disney gate is `MAT_FLAG_PBR_BSDF` only with
the `deriveAxAy` [0,1] clamp and `dielectricF0FromIor` eta clamp; sky/weather
clock monotonic with CLMT-driven sun arc and interior-fill RT-shadow gate
(#1200); tangent decode reads the Bethesda bitangent half into `Vertex.tangent`
(#786) across all three import paths; egui/GPU-timer teardown + driver-absent
`Ok(None)` path intact; the Cornell harness still builds a valid TLAS through the
production path with `mat.*` round-tripping via `MaterialTable`.

## Findings

### REN-2026-06-26-I01: Dim-12 checklist mis-describes #1235 — it's NIF-root-flags parity, not a "world-resource vs cached-snapshot" read
- **Severity**: INFO (audit-hygiene; no code defect)
- **Dimension**: Command-buffer recording / audit-skill maintenance
- **Location**: `.claude/commands/audit-renderer/SKILL.md` (Dimension 12 checklist); live code at `byroredux/src/cell_loader/spawn.rs` (`SceneFlags::from_nif(cached.root_flags)`), `cell_loader/nif_import_registry.rs`.
- **Status**: NEW (checklist wording correction)
- **Description**: The Dim-12 checklist reads *"cell-loader REFR spawn reads `SceneFlags` from the world resource, not a cached snapshot (#1235)."* The live #1235 (LC-D1-NEW-01) has nothing to do with a global world-resource scene-flag: it attaches a **per-entity** `SceneFlags` ECS component on the placement root, derived from the NIF root `NiAVObject.flags` (`cached.root_flags`), for parity with the loose-NIF loader. There is no "cached snapshot vs world resource" divergence to guard against. The underlying guard holds; only the description is wrong.
- **Evidence**: `spawn.rs` — `if cached.root_flags != 0 { world.insert(placement_root, SceneFlags::from_nif(cached.root_flags)); }`. APP_CULLED (bit 0) filtered import-side in `walk/mod.rs`; remaining bits ride through.
- **Impact**: None functional. A future auditor following the literal text would look for the wrong mechanism.
- **Suggested Fix**: Reword the Dim-12 checklist bullet to: *"cell-loader REFR spawn attaches a per-entity `SceneFlags` from the NIF root `NiAVObject.flags` for parity with the loose-NIF loader (#1235)."* No code change.

### REN-2026-06-26-I02: Dim-13 checklist points at `byroredux/src/render/camera.rs` for jitter assembly; the Halton jitter lives in `context/draw.rs`
- **Severity**: INFO (audit-hygiene; no code defect)
- **Dimension**: TAA / audit-skill maintenance
- **Location**: `.claude/commands/audit-renderer/SKILL.md` (Dimension 13 entry points + checklist); live code at `crates/renderer/src/vulkan/context/draw.rs` (`halton` fn + the `(jx, jy)` jitter block in `draw_frame`).
- **Status**: NEW (checklist path correction)
- **Description**: The Dim-13 entry-point list and checklist state the Halton(2,3) jitter is assembled in `byroredux/src/render/camera.rs`. That file contains zero Halton/jitter computation (`grep -c halton` → 0); it only assembles the un-jittered `view_proj`. The per-frame jitter is computed in `draw.rs::draw_frame` (`halton(idx, 2/3)`, `idx = (frame_counter % 16) + 1`) and uploaded into `GpuCamera.jitter`. The guard holds — jitter advances per frame and is applied in NDC — only the file pointer is stale.
- **Evidence**: `draw.rs` `fn halton(...)`, `halton(idx, 2)` / `halton(idx, 3)`; `camera.rs` grep returns 0 Halton hits.
- **Impact**: None functional. Misdirects a future auditor to the wrong file.
- **Suggested Fix**: Update the Dim-13 entry-point / checklist references to point jitter assembly at `crates/renderer/src/vulkan/context/draw.rs` (`halton` + the `(jx,jy)` block). No code change.

### REN-2026-06-26-N01: GitHub issue #1627 left OPEN after its code fix landed (tracker hygiene)
- **Severity**: (note — not a code finding)
- **Dimension**: Material Table / tracker hygiene
- **Location**: `crates/renderer/src/vulkan/material.rs` (`presets::glass`, `presets::car_paint`); GitHub issue #1627.
- **Status**: Existing: #1627 (code resolved, issue not closed)
- **Description**: The prior LOW finding REN-2026-06-23-L01 (the `glass()`/`car_paint()` doc comments naming the wrong tracker) is **resolved in code** by commit `eb71bcb9` — both comments now describe the deferral without the wrong issue number, and `glass()` is documented as a tested reference preset. However, `gh issue view 1627` still reports `state: OPEN` with the stale title. The fix shipped but the issue was never closed.
- **Evidence**: `git show eb71bcb9 -- crates/renderer/src/vulkan/material.rs` shows the comment rewrite; `gh issue view 1627` → OPEN.
- **Impact**: None functional. `presets::*` remain test-only (no production callers, consistent with the prior "preset unused" observation, pending the Disney transmission lobe).
- **Suggested Fix**: Close #1627 (its code deliverable is merged), or retitle it to track only the deferred transmission-lobe wiring if kept open as a feature reminder.

## Prioritized Fix Order

1. **Correctness** — none required.
2. **Safety** — none required.
3. **Hygiene** — (a) close or retitle #1627 (REN-2026-06-26-N01); (b) fix the two
   stale Dim-12 / Dim-13 checklist references in
   `.claude/commands/audit-renderer/SKILL.md` (REN-2026-06-26-I01 / -I02) so the
   next pass doesn't chase the wrong mechanism/file. All three are zero-risk doc
   edits; no source change.

## Needs-RenderDoc

None. No sync/barrier/render-pass/pipeline change is proposed in this audit (per
the no-speculative-Vulkan-changes guidance). Every sync invariant checked — the
per-image `render_finished`, the AS-build-input `SHADER_READ` access flag, the
egui `SUBPASS_EXTERNAL` dependencies, the SVGF/TAA/caustic/volumetric/bloom
GENERAL-layout barriers — was confirmed *present and correct* against its
documented form, not modified.

## Disproved / Confirmed-Fixed (not reported)

- **REN-2026-06-23-L01** (`GpuMaterial::glass()` doc names wrong tracker) —
  **fixed in code** by `eb71bcb9`; only the GitHub issue remains open (see
  REN-2026-06-26-N01).
- **REN-D10-01** (soft-particle depth fade mixes relative/absolute precision) —
  stayed fixed (`f0c81539` / #1642): the `MAT_FLAG_EFFECT_SOFT` along-ray gap
  rebases `camRel = cameraPos - renderOrigin` before the relative-space gap.
- **REN-D3-02 / REN-D3-03** (stale "7 color attachments" / removed reservoir
  G-buffer references) — confirmed still resolved; no stale text in `pipeline.rs`,
  `gbuffer.rs`, or `shader-pipeline.md` (the six-output set is pinned by
  `triangle_frag_declares_six_color_outputs`).
- Cornell metalness-vs-lighting confound and glass-stipple / IGN refraction
  jitter on opaque glass — known open observations per memory; not re-reported.
- The #681 skinned-output `VERTEX_BUFFER` usage flag — **not** a regression: the
  slot buffer deliberately omits `VERTEX_BUFFER` (raster reads skinned verts via
  the global vertex SSBO; the slot feeds only the BLAS refit). Documented
  Phase-2 state.
