# Safety Audit — 2026-06-11

- **Scope**: Full `audit-safety` sweep — all 12 dimensions (unsafe blocks, Vulkan spec,
  memory safety, thread safety, FFI, RT pipeline, compute pipelines, R1 material table,
  IOR refraction, NPC/animation spawn, NIFAL NaN boundary, debug-ui teardown).
- **Baseline**: `main` @ `1e8a25ab` (post camera-relative-rendering cascade, PR #1485).
- **Context**: Part of `/audit-suite --preset renderer-deep`. The same-day concurrency
  audit (`docs/audits/AUDIT_CONCURRENCY_2026-06-11.md`) verified Vulkan sync, resource
  lifecycle, and compute→AS→fragment chains clean — those territories are **not**
  re-derived here; this report covers the safety-specific dimensions plus a regression
  sweep of the 59 findings from `AUDIT_SAFETY_2026-06-01.md` (published as #1382–#1449)
  and a deep pass over code landed since June 1 (camera-relative rendering, two-surface
  glass, caustic temporal-EMA, FO4 precombine decode, spawn-time roughness resolve).
- **Dedup pool**: fresh `gh issue list` snapshot (200 issues, all states) at
  `/tmp/audit/safety/issues_all.json`; reused `/tmp/audit/concurrency/issues.json`.

## Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 1 |
| MEDIUM   | 2 |
| LOW      | 2 |
| **Total**| **5** |

The 2026-06-01 safety audit's findings were near-completely remediated (49 of 59
published issues CLOSED with verified fixes; the 9 still-OPEN ones re-confirmed and
deduplicated below, none regressed). Today's findings are concentrated in **code that
landed after June 1**: the Markarth geometry-corruption diagnostics, the FO4 precombine
(CSG) decode path, and the #1480 spawn-time normal-alpha-spec roughness resolve.

---

## Findings

### SAFE-D6-NEW-01: Mesh-index overshoot guard is log-only — inconsistent geometry still uploads, draws, and feeds BLAS builds
- **Severity**: HIGH
- **Dimension**: 6 — RT Pipeline Safety / 3 — Memory Safety
- **Location**: `crates/renderer/src/mesh.rs:374-404` (`accumulate_global_geometry`)
- **Status**: NEW
- **Description**: Commit `01251733` (June 9) added a "GEOMETRY CORRUPTION
  (#markarth-fragments)" diagnostic that detects a mesh whose maximum local index is
  `>= vertices.len()` — but it only `log::error!`s and then **uploads the mesh anyway**
  (`pending_vertices.extend_from_slice` / `pending_indices.extend_from_slice` run
  unconditionally right after the check).
- **Evidence**:
  - `mesh.rs:388-401` — `if max_idx as usize >= vertices.len() { log::error!(...) }`
    with no `bail!`, clamp, or skip; lines 403-404 append to the global pool regardless.
  - `device.rs` never enables `robustBufferAccess` (no hits for `robust_buffer_access`
    anywhere in `crates/renderer/src/`), so an out-of-range vertex fetch is undefined
    behavior, not a clamped read.
  - Static BLAS builds declare `max_vertex(vertex_count.saturating_sub(1))`
    (`blas_static.rs:201/486`); an index above `maxVertex` makes the acceleration-structure
    build input invalid per the Vulkan spec.
- **Impact**: A self-inconsistent (index, vertex) pair — from a NIF decode remap bug
  (the exact class the diagnostic was added to bisect), a corrupt file, or a mispointed
  CSG offset (see SAFE-D6-NEW-02) — produces: (a) raster reads into *other meshes'*
  vertices in the shared global pool (the "exploding spike" artifact), (b) for a
  pool-tail mesh, an out-of-bounds GPU vertex fetch with robustness off (UB, potential
  DEVICE_LOST), and (c) an invalid BLAS build input. Severity is impact-based: GPU-level
  UB on the AS/SSBO-indexing axis; the trigger needs malformed decode output, but the
  diagnostic exists precisely because that condition was suspected live on MarkarthWorld.
- **Related**: SAFE-D6-NEW-02 (a producer that can emit exactly this); #1392 (CLOSED —
  the analogous `instance_custom_index` guard was hardened from debug-only to a release
  runtime check); #1294 (`WorldBound.radius` trap referenced by the same commit).
- **Suggested Fix**: Turn the guard into a hard gate: `bail!` (skip the mesh, keep the
  log) or clamp offending indices to `vertices.len() - 1` before appending. The
  diagnostic value is preserved either way; the upload of known-inconsistent geometry
  is not.

### SAFE-D6-NEW-02: FO4 precombine decode emits triangle indices with no bounds check against `num_verts`
- **Severity**: MEDIUM
- **Dimension**: 6 — RT Pipeline Safety / NIF parse
- **Location**: `crates/nif/src/import/precombine.rs:109-146` (`decode_shared_geom_object`)
- **Status**: NEW
- **Description**: The new M49 precombine path reads raw u16 triples from the PSG blob
  and converts them straight to `u32` indices (`precombine.rs:140-145`) without
  validating any index `< num_verts`. Unlike inline NIF geometry, the PSG slice is
  located by a `(filename_hash, data_offset)` pointer into a separate `.csg` blob —
  a hash collision or stale/mispointed offset silently decodes garbage bytes as
  indices (values up to 65535) against an arbitrary vertex count.
- **Evidence**: `let tris = stream.read_u16_triple_array(tri_count)?;` then a plain
  push loop; `num_verts` is in scope and unused for validation. The result flows into
  `ImportedMesh` → `accumulate_global_geometry`, whose only guard is the log-only
  diagnostic of SAFE-D6-NEW-01.
- **Impact**: Producer-side half of SAFE-D6-NEW-01: a corrupt CSG read becomes OOB
  draw/BLAS input instead of a rejected object. MEDIUM per the "translatable block /
  parse mismatch" class — escalating consequences are owned by finding 01.
- **Related**: SAFE-D6-NEW-01; `docs/engine/fo4-csg-format.md` (reverse-engineered
  format, M49).
- **Suggested Fix**: After the read loop, `if max_index >= num_verts as u32 { return
  Err(io::Error::new(InvalidData, ...)) }` — one pass, decode-time rejection with the
  object's hash in the message.

### SAFE-D11-NEW-03: Spawn-time normal-alpha-spec roughness writes NaN into canonical `Material.roughness` after `resolve_pbr`
- **Severity**: MEDIUM
- **Dimension**: 11 — NIFAL Canonical-Translation Safety (NaN-on-GPU)
- **Location**: `byroredux/src/material_translate.rs:213-239`
  (`normal_alpha_spec_roughness`) + `:254-290` (`resolve_normal_alpha_spec_roughness`)
- **Status**: NEW (code landed June 9, commit `44171cd5` / #1480 / REN-D22-NEW-01)
- **Description**: The #1480 fix correctly moved the normal-alpha-as-spec roughness
  derivation from per-draw render time to a once-at-spawn resolve — but the formula
  `(1.0 - glossiness / 100.0).clamp(0.05, 0.95)` runs **after** `resolve_pbr()` and
  **overwrites** the resolved canonical roughness. `Material.glossiness` is a raw NIF
  binary float (`walker.rs:314` `shader.glossiness`, `:600` `mat.shininess`) with no
  `is_finite()` guard anywhere on its path, and Rust's `f32::clamp` **propagates NaN**.
  A non-finite `glossiness` on an alpha-bearing-normal lit surface therefore ships
  `roughness = NaN` past the only NaN gate in the pipeline (`resolve_pbr`'s `is_nan`
  check, which already ran) into `DrawCommand` → `to_gpu_material()` (verified: a
  straight field copy with no sanitization, `context/mod.rs:386`) → the `GpuMaterial`
  SSBO.
- **Evidence**: gate `normal_alpha_spec_applies` checks `metalness < 0.3` and
  `env_map_scale <= 0.3` (NaN comparisons are false, so those NaNs self-block) but
  `glossiness` is **not** in the gate — it is only used in the formula, where NaN
  survives `clamp`. The `specular_strength > 1.2` arm self-blocks on NaN; the
  alpha-normal arm does not.
- **Impact**: NaN roughness on the GPU for the affected draw — NaN GGX terms poison
  the lit color, and through SVGF/TAA temporal accumulation a single NaN pixel
  contaminates history buffers (sticky, frame-persistent). Trigger requires malformed
  content, consistent with the MEDIUM precedent of #1411/#1434 (raw-NIF-scalar finite
  guards). Blast radius: the gate population is large (every Skyrim/Gamebryo-era lit
  surface with an alpha-bearing normal map and no gloss map).
- **Related**: #1434 (OPEN — same class, `NiPSysGrowFadeModifier.base_scale`); #1411
  (CLOSED — emitter scalars); the 06-09 renderer audit's REN-D22-NEW-01 created this
  code but did not flag the NaN path.
- **Suggested Fix**: In `normal_alpha_spec_roughness`, early-return `None` when
  `!glossiness.is_finite()` (one line), or sanitize `glossiness` /
  `specular_strength` to finite defaults at the `translate_material` boundary.

### SAFE-D2-NEW-04: BLAS-compaction host `reset_query_pool` not gated on `host_query_reset_supported`
- **Severity**: LOW
- **Dimension**: 2 — Vulkan Spec Compliance
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs:684-686`
- **Status**: NEW (residual of #1478 / REN-D23-NEW-01, fixed June 9 in `73a43fc8`)
- **Description**: `73a43fc8` correctly decoupled the `hostQueryReset` device feature
  from ray-query support and gated `GpuPerFrameTimers::new` on the probed flag — but
  the BLAS-compaction path still calls host-side `device.reset_query_pool` with no
  gate. The feature is now enabled **only when probed supported**, so a hypothetical
  RT-capable device reporting `hostQueryReset = false` would reach this call with the
  feature disabled (VUID-vkResetQueryPool-None-02665). The commit explicitly
  adjudicates this as universal on RT hardware ("Vulkan 1.2 core and universal on
  every RT-capable GPU") — this finding records the missing *defensive* gate, not a
  live violation on any shipping driver.
- **Impact**: None on known hardware; latent spec violation on a hypothetical device.
- **Suggested Fix**: Either require `host_query_reset_supported` in
  `is_device_suitable` whenever `ray_query_supported` is accepted, or add a
  `debug_assert!(caps.host_query_reset_supported)` + fallback (cmd-buffer
  `vkCmdResetQueryPool`) at the compaction site.

### SAFE-D7-NEW-05: `VolumetricsParams` UBO has no SPIR-V block-size lockstep pin (CameraUBO now has one)
- **Severity**: LOW
- **Dimension**: 7 — Compute Pipeline Safety / GPU-struct lockstep
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs:70-92` (struct) vs
  `crates/renderer/src/vulkan/reflect.rs:433-470` (the new
  `camera_ubo_size_matches_gpu_camera_in_every_shader` test)
- **Status**: NEW
- **Description**: The camera-relative work (`36f66493`) grew `VolumetricsParams`
  by a trailing `render_origin: [f32; 4]` (Rust + `volumetrics_inject.comp` both
  updated — currently in lockstep, append-at-end so prior offsets are stable). The
  same commit introduced `reflect::uniform_block_size_by_name` and pinned **CameraUBO**
  across all 6 declaring shaders against `size_of::<GpuCamera>()`, exactly the guard
  class for the #1447 stale-`.spv` hazard. `VolumetricsParams` — the only other
  host-mirrored UBO struct that grew in this change — got no equivalent pin;
  `validate_set_layout` checks binding shape, not block size (the struct's own doc
  comment says field layout "is the host's responsibility").
- **Impact**: None today; a future `VolumetricsParams` edit without recompiling
  `volumetrics_inject.comp` would be silent per-frame param corruption (the drift
  class is HIGH *when it fires*; the pipeline is currently dormant behind
  `VOLUMETRIC_OUTPUT_CONSUMED == false`, #928).
- **Suggested Fix**: One more entry in the reflect test family:
  `uniform_block_size_by_name(VOLUMETRICS_INJECT_COMP_SPV, "VolumetricsParams") ==
  size_of::<VolumetricsParams>()`.

---

## Regression sweep — 2026-06-01 findings (#1382–#1449)

Every CLOSED fix touching files that churned since June 1 was re-verified in current code:

| Issue | Fix verified at | Verdict |
|---|---|---|
| #1406 / #1477 (MEM-03) AllocatorResource before VulkanContext on every teardown | `byroredux/src/main.rs:431-454` — `impl Drop for App` removes the resource + takes the renderer before field drops; idempotent with CloseRequested arm; covers panic unwind | HOLDS |
| #1390 (VKC-003) TLAS resize `device_wait_idle` before freeing old allocation | `acceleration/tlas.rs:304-317` | HOLDS |
| #1386 (VKC-002) release-mode AS scratch alignment | `align_scratch_address` applied to every raw device address (`blas_static.rs:292/665`, `blas_skinned.rs:246/520`) with headroom-reserve comments | HOLDS |
| #1395 (IOR-01) `ior = 0` clamp | `triangle.frag:2338` — `float GLASS_IOR = max(mat.ior, 1e-3)` | HOLDS |
| #1392 (RT-05) `instance_custom_index` 24-bit guard in release | `tlas.rs:200-242` runtime check before `Packed24_8::new` | HOLDS |
| #1409 (NIFAL-S4) collision shape finite guards | `collision.rs:309-380` — `finite()` / `finite_vec()` applied to radii, centers, half-extents (incl. MultiSphere + ConvexList boundaries) | HOLDS |
| #1411 / #1382 (NIFAL-S3/S7) particle emitter finite guards | `systems/particle.rs:344-378` — `rate.is_finite() && rate > 0.0 && start_size.is_finite() && start_size > 0.0` wraps the spawn loop; `life.max(0.05)`; `spawn_accumulator.is_finite()` debug assert | HOLDS |
| #1398 (MEM-01) NifImportRegistry cap | LRU, default 2048, `BYRO_NIF_CACHE_MAX` override, `=0` warns | HOLDS |
| #1399 (MEM-02) MeshRegistry slot overflow | `MAX_MESH_SLOTS = 1<<24` bail at both registration sites | HOLDS |
| #1396 (SAFE-U1) `BuiltinType::from_u32` | checked match with `_ => Err(UnsupportedBuiltin)`; doc-comment now says "fully checked match" | HOLDS |
| #1432 (SAFE-U6) SAFETY-comment sweep | spot-checked all unsafe added since June 1 (water_caustic `destroy_slot`/`recreate_on_resize`, blas raw-scratch reads, skin push-constant byte view) — all carry SAFETY comments / documented unsafe-fn contracts | HOLDS |
| #1389 / #1419 (NCPS-01/02) volumetrics init-failure + latch | re-verified by the same-day concurrency audit (Dim 2 row, `tlas_written` latch + #1105 debug_assert); dispatch still dormant behind `VOLUMETRIC_OUTPUT_CONSUMED == false` and callers honor the gate | HOLDS |
| #1478 hostQueryReset decouple | probe + own-merits enable + timer self-disable verified (`device.rs`, `gpu_timers.rs:185-196`) — residual noted as SAFE-D2-NEW-04 | HOLDS |
| #647-class blend-attachment counts | `water.rs:611` 7 entries (the June 9 `40f90efc` fix), `pipeline.rs:281/726` opaque + UI both 7 entries incl. reservoir | HOLDS |

## Verified clean (dimension highlights)

- **Dim 1 (unsafe)**: ~570 unsafe occurrences; all non-Vulkan ones audited
  (`nif/stream.rs` + `header.rs` read_pod byte-casts, `core/string` in-place ASCII
  fold, `material.rs::as_bytes` repr(C)+Copy+named-pads, `cell_loader/unload.rs`
  scratch shrink, ECS query raw-pointer caching) — sound, commented. New
  `drain_dirty_into` (ECS) is safe code. No `unsafe impl Send/Sync` anywhere.
- **Dim 6 (camera-relative + RT)**: the `render_origin` cascade is internally
  consistent — TLAS stays absolute; `fragWorldPos` reconstructed absolute in the
  vertex stage; cluster/froxel/splat positions lifted to absolute; the only RT
  consumer of the *rebased* instance matrix (`getHitTriNormal`,
  `triangle.frag:506-508`) uses it solely in translation-invariant edge cross
  products. `GpuCamera` 336 B pinned semantically against all 6 declaring shaders'
  committed SPIR-V (`reflect.rs:433`), compiler-version-independent.
- **Dim 7 (compute)**: caustic temporal-EMA decay→splat chain barrier-verified by the
  concurrency audit; per-FIF fence isolation documented; motion clears the
  accumulator so no cross-viewpoint ghosting. TAA first-frame α=1.0 +
  `should_force_history_reset` tested. Skin push constants 12 B (≤128 B), reflected
  set layouts validated at pipeline creation (`validate_set_layout` wired in
  skin_compute + texture_registry).
- **Dim 8 (R1 material)**: size pin now `gpu_material_size_is_300_bytes` (renamed from
  the 260 grep-continuity name — prose in the audit skill is stale, code is right);
  per-field offset pin present; all fields flat f32/u32; intern cap (16384) + upload
  truncation (`min(MAX_MATERIALS)`) consistent; `ui.vert` no longer reads
  MaterialBuffer at all (documented bypass via `textureIndex`, #1065) — the #785
  lockstep hazard is structurally gone; per-frame `MaterialTable::clear` bounds the
  dedup map.
- **Dim 9 (IOR)**: passthrough loop bounded (`REFRACT_PASSTHRU_BUDGET = 2` iterations
  + same-texture/fallback identity check); `GLASS_RAY_BUDGET` gate at the single IOR
  entry point; Frisvad basis live for refraction spread (`triangle.frag:2400`);
  interior miss fallback still cell-ambient vs sky split (`:2606-2607`); DBG_* bit
  catalog collision-free within itself (0x1…0x1000 distinct).
- **Dim 10 (anim spawn)**: FLT_MAX sentinel gates intact at all bspline emission
  sites; `AnimationClipRegistry` case-insensitive interning intact; bone-palette
  overflow guard + regression tests present.
- **Dim 11 (NIFAL)**: `translate_material` runs `resolve_pbr()` unconditionally
  (NaN sentinels seeded then resolved + clamped); `static_meshes.rs` no-Material
  fallback constructs finite literals; the one new gap is SAFE-D11-NEW-03.
- **Dim 3/5/12**: streaming pre-parse worker results are generation-stamped and
  stale-dropped before any GPU upload (no leak path; payload drop is CPU-only);
  cxx-bridge unchanged since June 1 (near-zero surface, #1417/#1423 fixes hold);
  debug-ui teardown ordering re-verified by the concurrency audit (EguiPass taken +
  destroyed first in Drop).
- **DDS**: new `format_has_alpha` covers every alpha-capable format the loader can
  actually emit (loader only produces SRGB variants of BC1/2/3/7).

## Existing-issue overlaps observed and skipped (dedup)

| Issue | State | Relation |
|---|---|---|
| #1439 (MEM-05) | OPEN | `read_pod_vec` type-level bound — unchanged, still prose-contract. |
| #1438 (IOR-03) | OPEN | ray-budget atomicAdd overshoot — shader-side, unchanged. |
| #1434 (NIFAL-S5) | OPEN | GrowFadeModifier finite guard — same class as SAFE-D11-NEW-03, different field/site. |
| #1433 / #1427 (EGUI-04/03) | OPEN | egui subpass dependency / pending_free flush — unchanged. |
| #1426 (VKC-005) | OPEN | allocator-leak early-return skips device_wait_idle — unchanged. |
| #1404 (NCPS-04) | OPEN | R32_UINT atomic format-feature query — unchanged. |
| #1387 (RT-04) | OPEN | skin output buffer VERTEX_BUFFER flag — premise stale per concurrency-audit triage note; not duplicated. |
| #1384 (IOR-04) | OPEN | three bitfields sharing value 128u — covers the DBG_VIZ_GLASS_PASSTHRU cross-bitfield concern. |
| #1443 / #1445 / #1444 (LC-D5-04, LC-D9-02/01) | OPEN | keyframe/emitter finite-sweep gaps — adjacent to Dim 10/11, not duplicated. |
| #1484 | OPEN | stale renderer comments — covers `pipeline.rs:274` "6 color attachments" doc-rot observed during the blend-array sweep. |
| #1481 / #1482 / #1483 | OPEN | SVGF clamp scope / DBG define pins / GPU-timer Drop path — renderer-audit territory, unchanged. |

## Method notes

- Per the speculative-Vulkan-fix policy, no barrier/stage-mask changes are proposed;
  none of today's findings require RenderDoc to verify (all are CPU-side gates or
  decode-time validation).
- Scratch: `/tmp/audit/safety/` (issue snapshots, dim notes).

Next step: `/audit-publish docs/audits/AUDIT_SAFETY_2026-06-11.md`
