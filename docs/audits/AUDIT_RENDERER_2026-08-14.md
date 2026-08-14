# Renderer Audit — 2026-08-14

**Scope**: `/audit-renderer --focus 1,2,8` — the ray-tracing slice, run as part of
the `rt-deep` audit-suite preset (`/audit-suite rt-deep`).

| Dimension | Area | Findings |
|---|---|---|
| 1 | Acceleration Structures (BLAS/TLAS correctness) | 1 MEDIUM · 2 LOW |
| 2 | SSBO/Index plumbing & RT ray queries (shader) | 2 MEDIUM · 2 LOW |
| 8 | Denoiser & composite | 1 MEDIUM · 2 LOW |

**Repo state**: HEAD `205744ae`, branch `main`. Dedup baseline: 2813 issues
(251 OPEN) fetched 2026-08-14.

---

## Executive Summary

**0 CRITICAL · 0 HIGH · 4 MEDIUM · 6 LOW.**

Every severity floor this slice exists to guard came back clean. Specifically:

- **AS/SSBO index contract** (CRITICAL floor) — `instance_custom_index` still
  equals the SSBO draw index at HEAD, guarded by `build_instance_map` plus the
  24-bit const-assert / `debug_assert` pair.
- **Ray self-intersection / tMin** (HIGH floor) — the 0.05 tMin convention holds
  at every ray-query site.
- **SVGF motion vectors** (HIGH floor) — reprojection reads the correct vectors;
  the stable-surface-ID disocclusion path is intact.
- **AS build → shader read barriers** (HIGH floor) — present at all four build
  sites, subject to the verification caveat below.

The one finding with real visual impact is **REN-D2-01**: the glass refraction
terminus multiplies the hit texture into the result twice, because it reads
`GpuInstance.avgAlbedo*`, which stopped meaning "material tint" at #1628 and now
means `diffuse_color × texel-mean`. Content seen *through* refractive glass
renders roughly 2–5× too dark relative to the same surface seen directly or in a
mirror. It is worth noting *why* this survived: the `--cornell` harness cannot
surface it, because `handle_avg_rgb` returns `None` for untextured handles, so
the double-multiply degenerates to identity on exactly the content the reference
scene is made of.

### Two structural themes, larger than any single finding

**1. Reference-doc drift is re-accumulating faster than it is being fixed.**
Four of the ten findings (REN-D1-02, REN-D2-02, REN-D2-03, REN-D8-03) are the
authoritative references disagreeing with the code. REN-D2-02 is *re-drift*:
#2252 corrected those same `GpuLight` rows on 2026-08-02, and `5798e467` moved
the code again on 2026-08-09 without the doc. `docs/engine/shader-pipeline.md`
and `docs/engine/memory-budget.md` are what the audit protocol designates as
authoritative and instructs auditors *not* to re-derive — so a wrong row there
propagates into every future audit rather than being caught by one.

**2. A CRITICAL-severity contract is held by duplicated code, not by a test.**
REN-D1-01: `build_instance_map` is documented as the single source of truth for
the AS↔SSBO index agreement, but only the TLAS builder reads it — the SSBO
builder re-derives the same compaction ~800 lines away in the same function.
They agree today by coincidence of correct duplication. This is the shape #419
was filed to eliminate; the fix removed the divergence without removing the
fragility, and nothing `cargo test` can see would catch its return.

### Verification caveats — read before acting on the barrier verdicts

- **No Vulkan device and no `BYRO_VALIDATION` run backed this audit.** All
  barrier and synchronisation verdicts are source-read only. Per the project's
  standing no-speculative-Vulkan-fixes rule, treat them as "no defect visible in
  source", not as "confirmed correct".
- **The BLAS eviction and budget machinery is unreachable on the 12 GB dev
  card.** It was verified through its predicates and unit tests rather than
  observed under pressure.
- **REN-D1-03 is currently unreachable** behind OPEN #2774. It is filed LOW for
  that reason and flagged as HIGH-if-reached — it matters to whoever closes
  #2774, not to anyone today.

### Suggested fix order

1. **REN-D2-01** — real wrong pixels, one-line shader fix (`rayHitAlbedo(tMat, tAlbedo)`).
2. **REN-D8-01** — real wrong pixels, exterior-only, narrow population; add the
   `indirect * albedo` term to composite's sky arm for consistency with the geometry arm.
3. **REN-D1-01** — add the `debug_assert_eq!` pin + unit test; cheap, and it
   converts a CRITICAL exposure into a caught regression.
4. **REN-D2-02 / REN-D2-03** — fold into OPEN #2781, which already touches the
   same doc tables.
5. **REN-D1-02, REN-D8-02, REN-D8-03** — doc/dead-code hygiene; REN-D8-03 is
   best closed by extending `_audit-validate.sh`'s advisory pass so the anchors
   cannot re-rot.

---

## Dimension 1



Date: 2026-08-14 · Depth: deep · Dedup baseline: `/tmp/audit/renderer/issues.json` (2813 issues, 251 OPEN)

## Scope & Coverage

### Files read in full
- `crates/renderer/src/vulkan/acceleration/mod.rs` (struct + `new` + `destroy`)
- `crates/renderer/src/vulkan/acceleration/constants.rs`
- `crates/renderer/src/vulkan/acceleration/types.rs`
- `crates/renderer/src/vulkan/acceleration/predicates.rs`
- `crates/renderer/src/vulkan/acceleration/tlas.rs`
- `crates/renderer/src/vulkan/acceleration/blas_static.rs`
- `crates/renderer/src/vulkan/acceleration/blas_skinned.rs`
- `crates/renderer/src/vulkan/acceleration/memory.rs`
- `crates/renderer/src/vulkan/acceleration/tests.rs` (test-name inventory only, 87 test fns)

### Files read in the relevant spans
- `crates/renderer/src/vulkan/context/draw.rs` — the fence-wait pair, `build_instance_map`
  call site, the `GpuInstance` SSBO builder loop, the TLAS-build block + `rt_flag` patch, the
  end-of-frame `shrink_tlas_to_fit` / `shrink_tlas_scratch_to_fit` pair
- `crates/renderer/src/vulkan/context/skinned_blas_refit.rs` — `record_skinned_blas_refit`
  (first-sight build queue, dispatch/skip gate, refit loop, closing AS barrier)
- `crates/renderer/src/vulkan/context/resources.rs` — `build_blas_for_mesh`,
  `build_blas_batched`, `build_global_blas_for_draws`
- `crates/renderer/src/vulkan/scene_buffer/constants.rs` — `MAX_INSTANCES` + its const-assert
- `crates/renderer/src/mesh.rs` — global vertex/index pool usage flags, index locality,
  `sanitize_scene_indices`
- `byroredux/src/cell_loader/unload.rs` — `finish_unload_batch` scratch-shrink call site
- `byroredux/src/app_frame.rs` — `build_global_blas_for_draws` call site
- `docs/engine/memory-budget.md` §"Acceleration Structures (BLAS / TLAS)"
- Prior reports: `docs/audits/AUDIT_RENDERER_2026-08-12.md`, `…-08-12b.md`, `…-08-07.md`

### Checklist items verified CLEAN (regression guards intact)

| Checklist item | Verdict |
|---|---|
| Vertex format `R32G32B32_SFLOAT` @ offset 0 | Clean — all four geometry sites (`build_blas`, `build_blas_batched`, `build_skinned_blas_batched_on_cmd`, `refit_skinned_blas`). Rigid strides by `size_of::<Vertex>()`; skinned strides by `SKIN_OUTPUT_STRIDE_BYTES` (#2170 split holds). `max_vertex = vertex_count.saturating_sub(1)` at all four. |
| Index type `UINT32`, `GeometryFlagsKHR::OPAQUE` | Clean at all four sites; `OPAQUE` also set on the TLAS `INSTANCES` geometry at both the size-query and build sites (documented-redundant, REN-D8-NEW-01). |
| Build-flag constants stable (#1144/#1196) | Clean. `UPDATABLE_AS_FLAGS` = `PREFER_FAST_TRACE\|ALLOW_UPDATE`, `SKINNED_BLAS_FLAGS` = `PREFER_FAST_BUILD\|ALLOW_UPDATE`, `STATIC_BLAS_FLAGS` = `PREFER_FAST_TRACE\|ALLOW_COMPACTION`. Value-pinned by `updatable_as_flags_is_fast_trace_plus_allow_update`, `skinned_blas_flags_is_fast_build_plus_allow_update`, `static_blas_flags_is_fast_trace_plus_allow_compaction`. Matches `docs/engine/memory-budget.md` §"Build flags" row-for-row. |
| `BlasEntry.built_flags` VUID-03667 pin (#1145) | Clean. `validate_refit_flags` + `validate_refit_counts` both run before the UPDATE in `refit_skinned_blas`, each with a `drop_skinned_blas` + fresh-BUILD fallback rather than a silent violation. |
| `instance_custom_index` == SSBO index (CRITICAL) | Values correct today — but the structural pin is weaker than the #419 fix implies. See **REN-D1-01**. |
| 24-bit ceiling | Clean. `const _: () = assert!(MAX_INSTANCES < (1 << 24))` in `scene_buffer/constants.rs` (`MAX_INSTANCES = 0x40000`), mirrored by the `debug_assert!` at the `Packed24_8::new` truncation site in `build_tlas_instances` (#957). `build_instance_map`'s `max_kept` forces over-cap draws to `None`, so no TLAS instance can name an unuploaded SSBO slot. |
| BUILD-vs-UPDATE keys on `last_blas_addresses` only | Clean. `decide_use_update` (empty→BUILD, gen-dirty→BUILD, length+zip compare) plus the `instance_count != built_primitive_count → use_update = false` guard (VUID-03708) and the `debug_assert_eq!` on the UPDATE arm (#1121). Padded/unused instance slots cannot break it: `ensure_tlas_state` only ever *grows* `max_instances`, the host→device copy writes exactly `instance_count` entries, and the UPDATE range uses `built_primitive_count`, which the guard has just proven equal. |
| Transform: column-major `mat4` → 3×4 row-major | Clean. `column_major_to_vk_transform` pinned by `column_major_to_vk_transform_pins_row_major_3x4_output` + `…_identity_maps_to_3x4_identity`; skinned draws correctly emit `IDENTITY_VK_TRANSFORM` (#1487), pinned by `skinned_tlas_instance_uses_identity_transform`. |
| `TRIANGLE_FACING_CULL_DISABLE` | Deliberately **gated on `draw_cmd.two_sided`** (#416), not set on all instances. The SKILL.md checklist wording ("on all instances (two-sided meshes)") reads as if it were unconditional — flagged here as an ambiguity for a future auditor, not filed as a finding. |
| Empty TLAS valid from frame 0 | Clean. `copy_size > 0` gates both buffer barriers and the copy (#317); the `primitiveCount = 0` BUILD still runs so binding 2 is always valid. |
| `SHADER_DEVICE_ADDRESS` on every address-queried buffer | Clean. Per-mesh VB/IB (`mesh.rs` upload), the global pool pair (`rt_usage` adds `ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR \| SHADER_DEVICE_ADDRESS` when `rt_enabled`), `instance_buffer_device`, and every scratch allocation. |
| LRU/shrink wiring (#1226 / #1227 / #1228) | Clean. `shrink_tlas_scratch_to_fit` uses `tlas_scratch_should_shrink` (256 KB slack) and is called at the END of `draw_frame` on `self.current_frame` *after* the increment, paired with and ordered after `shrink_tlas_to_fit`; cell-unload (`finish_unload_batch`) calls the *different* `shrink_blas_scratch_to_fit`; `rt_flag` is patched to 1.0 post-TLAS on `first_tlas_this_slot`; the three `missing_blas` cause-counters all increment and surface only through the rate-limited `log::warn!` in `build_tlas_instances`. All matches `docs/engine/memory-budget.md`. |
| #1793 documented-not-fixed gaps | Present and still documented in-place (no rigid-BLAS recovery path; `--grid` burst `frame_counter` aging). Recast, not re-reported. |
| Deferred BLAS destruction (#a476b256) | Clean. `drop_blas`, `evict_unused_blas` and `drop_skinned_blas` all push onto `pending_destroy_blas` (`DEFAULT_COUNTDOWN` = `MAX_FRAMES_IN_FLIGHT`); `pending_destroy_scratch` (#1782) is used at all three grow/shrink retire sites that can run from `about_to_wait`. No immediate `destroy_acceleration_structure` was reintroduced at any eviction/drop site. Shutdown drains synchronously (`drain_pending_destroys`, then `destroy()` drains `blas_entries` + `skinned_blas` + TLAS slots + scratch). |
| #2673 / #2674 commit-point discipline | Clean. `ensure_tlas_state` is allocate-then-swap with per-step `inspect_err` unwinding; `build_tlas` promotes `last_blas_addresses` / clears `needs_full_rebuild` / stamps `last_blas_map_gen` only *after* `cmd_build_acceleration_structures`. |
| #2481 overwrite guard | Clean. `build_blas`, `build_blas_batched` phase 7 and `build_skinned_blas_batched_on_cmd` phase 4 each call `drop_blas` / `drop_skinned_blas` before registering, pinned by `build_blas_releases_before_overwriting`, `build_blas_batched_releases_before_overwriting`, `skinned_blas_batch_releases_before_overwriting`. |
| #2460 shared-scratch peak walk | Clean. `shared_blas_scratch_peak` chains `blas_entries` and `skinned_blas`; the `refit_skinned_blas` `debug_assert!(scratch_buffer.size >= entry.build_scratch_size)` backstop is intact. |
| Global-pool LOD BLAS (`build_global_blas_for_draws`) | Clean. Global-pool indices are mesh-**local** (`sanitize_scene_indices` clamps to the mesh's own block; `cmd_draw_indexed` supplies `global_vertex_offset` as `vertexOffset`), so `vertex_byte_offset`/`index_byte_offset` subranges with `first_vertex = 0` and `max_vertex = vertex_count - 1` describe the correct triangles. Pool compaction relocating source bytes cannot invalidate an already-built static BVH (static BLAS never UPDATE). |
| Skinned build → refit → TLAS barrier chain | Source-level clean: `COMPUTE_SHADER/SHADER_WRITE → (AS_BUILD\|FRAGMENT)/SHADER_READ` before the builds, `record_scratch_serialize_barrier` (`AS_WRITE → AS_WRITE\|AS_READ`, #1790) self-emitted before every build and every refit, closing `AS_WRITE → AS_READ` before `build_tlas`, and `AS_WRITE → FRAGMENT\|COMPUTE / AS_READ` after it (#415). **Not runtime-verified** — see limits. |

### Could NOT verify (and why)
- **Barrier correctness on real hardware.** All barrier claims above are source reads only. No
  Vulkan device was driven and no `BYRO_VALIDATION=1` / sync-validation run was performed in
  this dimension, so "clean" here means "the barrier is present with the access masks the spec
  assigns", not "no hazard was observed".
- **Eviction / budget behaviour.** `blas_budget_bytes = VRAM/3` yields ~4 GB on the 12 GB dev
  card, so `evict_unused_blas`, `should_evict_mid_batch` and every `#1793` gap are unreachable
  without a forced-budget harness. Verified by reading the predicates + their unit tests only.
- **`shrink_tlas_scratch_to_fit` case-2 reachability** — analysed statically (see REN-D1-03),
  not observed.
- Dimension 2 (shader-side `instance_custom_index` consumption) is explicitly out of scope here;
  the CPU half of that contract is covered above.

---

## Findings

Severity counts: **0 CRITICAL · 0 HIGH · 1 MEDIUM · 2 LOW**

### REN-D1-01: The #419 shared instance map is honoured by the TLAS builder only — the SSBO builder still re-derives its own compaction, with nothing pinning the two together
- **Severity**: MEDIUM
- **Dimension**: AS Correctness
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` (`draw_frame` — the `build_instance_map` call site and the `GpuInstance` builder loop), `crates/renderer/src/vulkan/acceleration/predicates.rs` (`build_instance_map`), `crates/renderer/src/vulkan/acceleration/tlas.rs` (`build_tlas_instances`)
- **Status**: NEW
- **Description**: `build_instance_map` is documented — in its own doc comment and at the call
  site — as "the single source of truth the TLAS `instance_custom_index` and the SSBO position
  must agree on". Only one of the two consumers actually reads it. `build_tlas_instances` indexes
  `instance_map[i]`; the SSBO builder in `draw_frame` never touches the map and instead recomputes
  the compacted position as `gpu_instances.len()` behind its own copy of the predicate. The two
  agree today purely because the predicate text is duplicated correctly and because the SSBO
  loop's only index-affecting `continue` is the identical `mesh_registry.get(…)` reject. That is
  the exact "two independent filter predicates" shape #419 was filed to eliminate, and the fix
  removed the divergence without removing the fragility. No assertion or test pins the
  equivalence.
- **Evidence**:
  - `draw.rs`, TLAS side: `build_instance_map(draw_commands.len(), MAX_INSTANCES, |i| self.mesh_registry.get(draw_commands[i].mesh_handle).is_some())`.
  - `draw.rs`, SSBO side, ~800 lines later: `for draw_cmd in draw_commands { let Some(mesh) = self.mesh_registry.get(draw_cmd.mesh_handle) else { continue; }; let instance_idx = gpu_instances.len() as u32; … }`.
  - The other two `continue`s in that loop both sit *after* `gpu_instances.push(…)` (batch-skip and batch-extend), so they are index-neutral **today**; a fourth `continue` inserted anywhere between the `mesh_registry.get` reject and the `push` would silently shift every later SSBO entry while the TLAS custom indices stayed put.
  - Blast radius on divergence is the severity table's `SSBO index mismatch → CRITICAL` row: every RT hit reads the wrong `GpuInstance` (wrong model matrix, wrong `material_id`, wrong `surface_id`).
  - `debug_assert_eq!(gpu_instances.len(), previous_models.len())` already exists at the upload site — the analogous map-vs-SSBO assert does not.
- **Impact**: No wrong pixels today. The exposure is that a CRITICAL-severity contract is held
  only by a duplicated predicate ~800 lines apart in one function, with no `cargo test`-visible
  guard; the failure is silent (garbage material/transform on RT hits, not a crash or validation
  error).
- **Related**: #419 (CLOSED, fix intact), #957 / #1392 (24-bit truncation guard), #194, #2116;
  `AUDIT_RENDERER_2026-08-12b.md` §"The AS ↔ SSBO index contract" verified the *values* agree at
  HEAD but proposed no pin.
- **Suggested Fix**: Add `debug_assert_eq!(gpu_instances.len(), instance_map.iter().flatten().count())`
  immediately before the UI-quad append in `draw_frame` (the UI quad is pushed after the loop, so
  the counts must match exactly at that point), and a unit test that walks a synthetic
  draw-command list through both compaction rules. Cheaper and more robust than threading the map
  into the SSBO loop.

### REN-D1-02: The single-shot static-BLAS path (`build_blas` / `build_blas_for_mesh`) has no caller, but `memory-budget.md` documents it as a live call site
- **Severity**: LOW
- **Dimension**: AS Correctness
- **Location**: `crates/renderer/src/vulkan/context/resources.rs` (`build_blas_for_mesh`), `crates/renderer/src/vulkan/acceleration/blas_static.rs` (`build_blas`), `docs/engine/memory-budget.md` §"LRU eviction", `crates/renderer/src/vulkan/acceleration/mod.rs` (`pending_destroy_scratch` field doc)
- **Status**: NEW
- **Description**: `AccelerationManager::build_blas` is reachable from exactly one place,
  `VulkanContext::build_blas_for_mesh`, and that function has **zero callers** anywhere in the
  workspace — no binary, no test, no example. The entire single-shot BLAS build path is dead.
  Three pieces of documentation describe it as live, and one of them is an authoritative doc.
- **Evidence**:
  - `grep -rn --include='*.rs' "build_blas_for_mesh" .` → the definition in `resources.rs`, one
    module-index comment in `context/mod.rs`, and one doc-comment cross-reference. No call.
  - `grep -rn --include='*.rs' "\.build_blas(" .` → `resources.rs` only.
  - `docs/engine/memory-budget.md` §"LRU eviction": *"a single-shot guard inside `build_blas`
    itself … for the **ad-hoc / UI-quad / lazy-upload path** that sits outside the M40 cell-loader
    batched hot path (#915)"*. The doc side is wrong twice over: there is no caller at all, and
    `register_ui_quad` uploads the UI quad with `for_rt = false`, so it never had a BLAS.
  - `acceleration/mod.rs`'s `pending_destroy_scratch` doc names the grow-replace sites as
    *"three sites — `blas_static::build_blas`, `blas_static::build_blas_batched`, and
    `memory::shrink_blas_scratch_to_fit`"*; only two are reachable.
  - Consequences of the death: #915's eviction guard and #1782's deferred-scratch route on that
    path are unexercised; `build_blas` sets `STATIC_BLAS_FLAGS` (with `ALLOW_COMPACTION`) but runs
    no compaction pass, so it would produce uncompacted BLAS if revived; and its
    `.expect("BLAS build requires a per-mesh vertex buffer…")` would panic on a global-only mesh
    because, unlike `resources.rs::build_blas_batched`, `build_blas_for_mesh` does not filter on
    `mesh.rt_capable`.
- **Impact**: No runtime impact (dead). Two costs: an authoritative memory doc asserts a call site
  that does not exist, and ~300 LOC of unexercised AS-build code carries a revive-time panic and a
  missing compaction pass that a future "lazy BLAS upload" author would inherit silently.
- **Related**: #915 (CLOSED — the guard it added is on the dead path), #658 (CLOSED — the
  `ALLOW_COMPACTION` flag it added is on the dead path), #1141 (the same "delete the dead build
  entry point" call made for the skinned sibling `build_skinned_blas`).
- **Suggested Fix**: Either delete `build_blas_for_mesh` + `AccelerationManager::build_blas`
  (the #1141 precedent) and drop the three doc references, or — if the lazy-upload path is
  genuinely planned — add the `mesh.rt_capable` filter to `build_blas_for_mesh` and correct
  `memory-budget.md` to say the path is provisioned but unwired.

### REN-D1-03: Two latent defects sit inside `shrink_tlas_scratch_to_fit`'s live-slot realloc arm, one of which is a `draw_frame` panic — they go live the moment OPEN #2774 is "fixed" by making the arm reachable
- **Severity**: LOW (currently unreachable; would be HIGH if the arm were reached)
- **Dimension**: AS Correctness
- **Location**: `crates/renderer/src/vulkan/acceleration/memory.rs` (`shrink_tlas_scratch_to_fit`, case-2 live-slot arm), `crates/renderer/src/vulkan/acceleration/tlas.rs` (`build_tlas` scratch-address query, `ensure_tlas_state`)
- **Status**: NEW — distinct defects, blocked behind OPEN **#2774** ("`shrink_tlas_scratch_to_fit` case-2 live-slot realloc arm appears unreachable"), which covers only the reachability question
- **Description**: Independent static analysis confirms #2774's premise — the live-slot arm cannot
  fire, because `tlas_scratch_peak_bytes[slot]` and `scratch_buffers[slot]` are written in the same
  `ensure_tlas_state` block, leaving `current - peak == scratch_alignment_padding(scratch_align)`
  (≤ 255 B) while `tlas_scratch_should_shrink` needs `current > 2 × peak` **and**
  `current - peak > TLAS_SCRATCH_SLACK_BYTES` (256 KB). `ensure_tlas_state` only ever grows
  (`max_instances < instance_count`), so the recorded peak never regresses below the allocated
  capacity. The arm nonetheless contains two defects that a "make it reachable" resolution of
  #2774 would ship:
  1. **Missing alignment headroom.** The realloc target is the bare `peak`
     (`create_device_local_uninit(device, allocator, peak, …)`), unlike the BLAS sibling
     `shrink_blas_scratch_to_fit`, which uses `peak.saturating_add(scratch_alignment_padding(self.scratch_align))`
     and documents exactly why. `build_tlas` then rounds the buffer's device address up via
     `align_scratch_address` before submitting — on a driver whose `GpuOnly` addresses are not
     already `minAccelerationStructureScratchOffsetAlignment`-aligned, the build's scratch range
     runs past the allocation by up to `align - 1` bytes. It is **not** self-correcting on this
     path: `scratch_needs_growth` is consulted only inside `ensure_tlas_state`'s `need_new_tlas`
     block, which may not run for many frames.
  2. **Destroy-then-allocate.** The arm takes and destroys the old buffer *before* attempting the
     replacement; on `Err` it logs a warn and leaves `scratch_buffers[slot] = None` **with
     `tlas[slot]` still `Some`**. The next `build_tlas` for that slot finds
     `max_instances >= instance_count`, so `ensure_tlas_state` returns early without allocating
     scratch, and `build_tlas` reaches
     `self.scratch_buffers[frame_index].as_ref().unwrap()` → panic inside an open command-buffer
     recording. The arm's own comment ("the next build's `scratch_needs_growth(None, …)` arm will
     re-allocate. Degraded but correct") states the case-1 behaviour, which is correct only
     because case 1 also leaves `tlas[slot] == None`. This is the exact hazard #2673 called out
     and fixed in `ensure_tlas_state` — the sibling site was not converted.
- **Evidence**:
  - `memory.rs`, case 2: `if let Some(mut old) = self.scratch_buffers[slot_index].take() { old.destroy(…); } match GpuBuffer::create_device_local_uninit(device, allocator, peak, …) { Ok(new_buf) => …, Err(e) => { log::warn!(…); true } }`.
  - `memory.rs`, BLAS sibling for contrast: `let target = peak.saturating_add(scratch_alignment_padding(self.scratch_align));`.
  - `tlas.rs`, `ensure_tlas_state` scratch allocation: `let scratch_size = sizes.build_scratch_size + scratch_alignment_padding(self.scratch_align);` versus the peak record `self.tlas_scratch_peak_bytes[frame_index] = sizes.build_scratch_size;` (unpadded).
  - `tlas.rs`, `build_tlas`: the unconditional `self.scratch_buffers[frame_index].as_ref().unwrap()`, with `ensure_tlas_state`'s scratch allocation nested inside `if need_new_tlas`.
  - `tlas.rs`, #2673's own comment already names this failure mode for its own site: *"had a later frame found the (now smaller) instance count fitting an existing TLAS, `build_tlas`'s `scratch_buffers[..].unwrap()` would have panicked on the missing scratch."*
  - `AUDIT_RENDERER_2026-08-12b.md` asserts the padding question is "self-correcting"; that holds for the BLAS path (which has a growth check on every build) but not for this one.
- **Impact**: None today. If #2774 is resolved by recalibrating the predicate rather than deleting
  the arm, defect 2 becomes a hard process abort mid-`draw_frame` under the exact VRAM-pressure
  regime the shrink exists to relieve, and defect 1 becomes a latent AS build-scratch overrun on
  a misaligning driver.
- **Related**: #2774 (OPEN, reachability), #2673 (CLOSED — allocate-then-swap, applied to
  `ensure_tlas_state` only), #1386 / #659 (scratch alignment padding), #1226 (TLAS-calibrated
  slack), #2460.
- **Suggested Fix**: Whoever closes #2774 should decide first: if the arm is deleted, both
  defects go with it. If it is kept, mirror `shrink_blas_scratch_to_fit` — add
  `scratch_alignment_padding` to the target and allocate into a local before retiring the old
  buffer — and make `build_tlas` tolerate a missing scratch (allocate on demand or bail with
  `Err`) rather than `unwrap`.

---

## Notes carried, not filed

- **Duplicate LRU stamp pass in `build_tlas`.** `build_tlas_instances` already stamps
  `last_used_frame` on every eligible draw's static/skinned entry; `build_tlas` then walks
  `draw_commands` a second time doing the same. Already **OPEN as #2769** — skipped per dedup rule.
- **Stale monolith-era comments in the acceleration module.** Already **OPEN as #2773** — skipped.
- **`evict_unused_blas`'s `let _ = (device, allocator);`** — parameters retained deliberately
  (#2692) for call-site stability; noise, not filed.
- **SKILL.md Dim-1 checklist wording** on `TRIANGLE_FACING_CULL_DISABLE` reads as unconditional
  while the code correctly gates on `two_sided` (#416). Ambiguous rather than wrong; noted in the
  coverage table so a future auditor does not "restore" the unconditional form.

---

## Dimension 2



Date: 2026-08-14 · Preset: `rt-deep` · Depth: deep

## Scope & Coverage

### Files actually read (full or targeted)

Shader side:
- `crates/renderer/shaders/triangle.frag` (main, RT-LOD/gating block, window portal,
  glass IOR + refraction passthru loop, metal reflection, ReSTIR temporal/spatial/write-back,
  bounded GI path, effect-shader branch, BC1 alpha pin, mesh-ID write)
- `crates/renderer/shaders/water.frag` (`traceWaterRay`, `foamShoreline`, `absorbWaterColumn`,
  reflection/refraction ray setup, the water-side caustic splat)
- `crates/renderer/shaders/include/bindings.glsl` (full)
- `crates/renderer/shaders/include/raytrace.glsl` (full — `traceReflection`)
- `crates/renderer/shaders/include/ray_hit.glsl` (full)
- `crates/renderer/shaders/include/lighting.glsl` (full)
- `crates/renderer/shaders/include/shadow_common.glsl` (full)
- `crates/renderer/shaders/include/shadow_transport.glsl` (full)
- `crates/renderer/shaders/include/material_sampling.glsl` (full)
- `crates/renderer/shaders/include/math_common.glsl` (full)
- `crates/renderer/shaders/include/clusters.glsl` (full)
- `crates/renderer/shaders/include/shader_constants.glsl` (full, generated)

Rust upload/agreement side:
- `crates/renderer/src/vulkan/scene_buffer/buffers.rs` (`build_scene_descriptor_bindings`,
  buffer sizing)
- `crates/renderer/src/vulkan/scene_buffer/constants.rs` (capacity consts + the 24-bit
  const-assert)
- `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` (`GpuInstance`, `GpuLight`, `GpuCamera`)
- `crates/renderer/src/vulkan/scene_buffer/upload.rs` (`upload_terrain_tiles`)
- `crates/renderer/src/vulkan/context/draw.rs` (`GpuInstance` fill, `gi_albedo`,
  `skinned_vertex_address`, `screen`/camera upload, terrain-tile flag packing)
- `crates/renderer/src/vulkan/context/geometry_pass.rs` (`first_instance` emission)
- `crates/renderer/src/vulkan/acceleration/tlas.rs` (`build_tlas_instances` —
  `instance_custom_index_and_mask`)
- `crates/renderer/src/vulkan/context/resources.rs` (`allocate_terrain_tile`)
- `crates/renderer/src/vulkan/context/resize.rs` (`set_upscaler_mode`, reservoir recreate)
- `crates/renderer/src/vulkan/restir.rs` (`ReservoirBuffers`, `RESERVOIR_STRIDE`)
- `crates/renderer/src/mesh.rs` (`sanitize_scene_indices`, `global_vertex_offset` /
  `global_index_offset` semantics)
- `crates/renderer/src/texture_registry.rs` (`handle_avg_rgb`)
- `docs/engine/shader-pipeline.md` (GPU-struct tables, Set-1 descriptor table, capacity table)

### Checklist items verified CLEAN

- **`instance_custom_index` == SSBO index.** `build_tlas_instances` packs `ssbo_idx` from the
  shared `instance_map`, with a `debug_assert!` mirroring the `MAX_INSTANCES < (1 << 24)`
  const-assert in `scene_buffer/constants.rs`. Every hit site (`traceReflection`,
  `traceShadowTransmittance`'s two loops, the refraction passthru loop, the GI path loop,
  `traceWaterRay`) reads `instances[customIdx]` then `materials[inst.materialId]` — no site
  uses `gl_InstanceID`/`gl_InstanceIndex` for a ray hit. `fragInstanceIndex` (raster) is the
  same index space (`first_instance` in `geometry_pass.rs`), so the `selfInstance` comparisons
  in `traceReflection` and the `terminusOnSelf` guard are well-typed.
- **Vertex/index SSBO offsets agree with the Rust upload.** `GpuInstance.vertex_offset` /
  `index_offset` are `mesh.global_vertex_offset` / `global_index_offset` (vertex- and
  index-counted, not bytes), and the global index pool stores mesh-LOCAL indices
  (`sanitize_scene_indices` clamps to `vertex_count`). `getHitUV`, `getHitVertexAlpha`,
  `getHitTriWorldPositions`, `getRayHitTangentFrame` all use
  `(vOff + i) * VERTEX_STRIDE_FLOATS + <field offset>` with the generated constants. The
  skinned branch's bare `i0 * SKIN_OUTPUT_STRIDE_FLOATS` (no `+ vOff`) is correct given
  mesh-local indices, and `skin_slot_backs_mesh` gates a stale slot's address out.
- **Vertex-lane safety.** No RT hit-fetch site reads float lanes 12–15 or 20–21 (the
  non-IEEE bone-index / splat-unorm lanes); `rt_hit_shaders_have_no_unsafe_vertex_data_reads`
  still backs this.
- **Set-1 binding numbers** in `bindings.glsl` (0/1/2/4/5/6/7/8/9/10/11/13/14/15/16/17) match
  `build_scene_descriptor_bindings` one-for-one.
- **Shadow rays.** `traceShadowBinary` uses `OpaqueEXT | TerminateOnFirstHitEXT` and returns a
  binary `CommittedIntersectionNone` test. The live direct-shadow path uses
  `traceLightTransmittance` → `traceShadowTransmittance`, whose origins come from
  `offsetRayOrigin(fragWorldPos, shadowOffsetNormal)` with the **geometric** normal (not the
  normal-mapped `N`), per the #1017/RT-bias contract. Disk/cone jitter is correct: point/spot
  jitter a concentric disk in the plane `⊥ L` at the light position (`buildOrthoBasis(L,…)`);
  directional jitters a tangent-plane disk scaled by `skyTint.w` and renormalises.
  `rayDist = length(jitteredTarget - rayOrigin) - 0.1`, floored by `max(rayDist, 0.01)`.
- **tMin convention.** 0.05 at `traceReflection`, the window portal, the refraction passthru
  loop (**every** iteration — #2462 fix intact, `rayTMin` is never reset to 0.0), the GI path
  loop, `traceWaterRay`, `foamShoreline`, and the water caustic floor ray.
  `traceShadowTransmittance` uses tMin 0.0 by design, paired with Wächter–Binder
  `offsetRayOrigin` at the origin and `advanceShadowRayPastHit` between layers.
- **Segment bookkeeping.** `traceReflection` (`remaining -= advance`), the refraction loop
  (`refrRemaining` — #2482 fix intact), `traceShadowTransmittance`, and `traceWaterRay` all
  decrement their shared reach; `origin + direction * travelled` reconstructs the hit
  position consistently in each.
- **Glass / IOR.** Frisvad `buildOrthoBasis` at the roughness-spread site (#820 intact);
  window-portal demote (`isWindow = false` after an interior hit, #789) intact;
  `hitIsGlass` keyed on `materialKind == MATERIAL_KIND_GLASS`, not texture equality (#2692);
  interior miss falls to `sceneFlags.yzw` alone on both the reflection miss and the refraction
  escape (#1125); `GLASS_RAY_BUDGET`/`GLASS_RAY_COST` wired via `rayBudget.glassRayLimit` with
  the documented unconditional-overshoot `atomicAdd` (#1438) and no CPU reader;
  `DBG_VIZ_GLASS_PASSTHRU` still wired at `diagPassthru`/`diagSelfTerminus` plus the viz
  branch.
- **Thin-glass gate (#883f57cd).** `glassIORAllowed = isGlass && !isThinGlass &&
  reflectionGlassRayEnabled && !isWindow && rtLOD < RT_LOD_IOR` — exact, unchanged.
- **RT gating.** `rtEnabled = sceneFlags.x > 0.5 && !compileDisableAllRays &&
  !runtimeDisableAllRays`; every ray query in `triangle.frag` sits under
  `directShadowRayEnabled` / `giRayEnabled` / `reflectionGlassRayEnabled`. `water.frag` gates
  `traceWaterRay`, `foamShoreline`, and the caustic block on `sceneFlags.x` (#1561). TLAS is
  Set 1 / Binding 2 at every site.
- **ReSTIR-DI (#d523b9b3 / #883f57cd).** `SPATIAL_NORMAL_COS = 0.906` cone test present and
  applied to `octDecode(unpackSnorm2x16(rn.pad0))` **before** combining; the reservoir writes
  `octEncode(normalize(fragNormalEffective))` (geometric, not shading `N`) into `pad0`;
  reservoir stays 32 B (`RESERVOIR_STRIDE`); surface tag is
  `inst.surfaceId & RESERVOIR_SURFACE_MASK`, not `fragInstanceIndex + 1`. Reservoir indexing
  (`gl_FragCoord.y * uint(screen.x) + gl_FragCoord.x`) agrees with the buffer's extent:
  `screen` is uploaded from `frame_extents.render`, and `ReservoirBuffers::new` /
  `recreate_on_resize` use the same `frame_extents.render`; the FSR quality-mode switch routes
  through `recreate_swapchain`, so the two cannot desync. The 10-bit light lane
  (`RESERVOIR_LIGHT_MASK = 0x3FF`) covers `MAX_LIGHTS = 512` with 1023 as the invalid
  sentinel, and every read re-gates on `rpLightIndex < lightCount`.
- **BC1 punch-through (#ae285062).** `texColor.a = 1.0` when
  `(inst.flags & INSTANCE_FLAG_DIFFUSE_ALPHA) == 0u && mat.alphaThreshold == 0.0`, applied
  before the `materialAlpha` multiply; `rayHitHasCoverage` applies the same rule on the RT
  side; the CPU bit comes from `handle_has_alpha` in `draw.rs`.
- **Noise determinism.** `interleavedGradientNoise` and `hash2_pixel_frame` are seeded from
  `cameraPos.w` (`frame_counter & 0xFFFFFF` — exactly representable in f32), never a true RNG.
- **Terrain tile index.** `(flags >> 16) & 0xFFFF` cannot exceed `MAX_TERRAIN_TILES`: slots
  come from `allocate_terrain_tile`'s bounded free list, and the SSBO is allocated at the full
  `MAX_TERRAIN_TILES` regardless of live count.
- **Cluster indices.** `getClusterIndex` clamps tile/slice; `clusterLightIndices[cluster.offset
  + ci]` is bounded by the builder's `cluster.count`.

### Could NOT verify (and why)

- **Runtime confirmation of any of the below.** No Vulkan device was driven; nothing here was
  reproduced in a capture. Findings are static-analysis only.
- **Whether `getRayHitTangentFrame`'s bind-pose normal/tangent read visibly misaligns skinned
  actors' secondary-ray shading.** This is the documented residual limitation of #2219
  (`skin_vertices.comp` writes position only) — it needs a RenderDoc capture of a skinned actor
  seen through glass/in a mirror to quantify. Not re-reported.
- **Whether the alpha-blend fragment's unconditional `reservoirsCurr[pixelIdx] = rc` write
  measurably costs the opaque surface behind it any soft-shadow history.** The surface-ID +
  normal-cone guards make it *safe*, so the only question is convergence rate behind
  transparent overlays — unanswerable without a frame capture, and not a correctness defect.
- **Per-mesh raster fallback (`global_bound == false` in `geometry_pass.rs`) vs. RT hit
  lookups**, which always resolve against the global vertex/index SSBOs. Whether the fallback
  can be entered while RT is live is a Dimension 1/5 lifecycle question, not an index-plumbing
  one; left to those dimensions.

---

## Findings

### REN-D2-01: Glass refraction terminus multiplies the hit texture in twice — reads `avgAlbedo`, which stopped being the material tint at #1628
- **Severity**: MEDIUM
- **Dimension**: SSBO/Indexing
- **Location**: `crates/renderer/shaders/triangle.frag` — the IOR refraction terminus
  (`tInst` / `tAlbedo` / `tColor` inside the `refractionResolved` branch); field source:
  `gi_albedo` in `crates/renderer/src/vulkan/context/draw.rs`; correct sibling:
  `rayHitAlbedo` in `crates/renderer/shaders/include/ray_hit.glsl`
- **Status**: NEW
- **Description**: The refraction terminus is the only secondary-ray hit site that derives its
  surface colour from `GpuInstance.avgAlbedo*` instead of the shared `rayHitAlbedo(mat,
  baseRgb)` helper. It samples the hit's diffuse texture (`textureLod(textures[tInst
  .textureIndex], tUV, refrMip)`) and then multiplies by `tInst.avgAlbedoR/G/B`. Since #1628
  (`93add433`, 2026-06-15) `avg_albedo_*` is no longer the material tint: `draw.rs` uploads
  `draw_cmd.avg_albedo[i] * handle_avg_rgb(texture_handle)[i]` — the material `diffuse_color`
  **times the diffuse texture's mean texel colour**. The refraction site (`f1b6e1e9`,
  2026-06-05) predates that change by ten days and was never revisited, so the texture now
  enters the product twice: once as the sampled texel, once as its own frame-wide mean.
- **Evidence**:
  - `draw.rs`: `let gi_albedo = match self.texture_registry.handle_avg_rgb(
    draw_cmd.texture_handle) { Some(mean) => [draw_cmd.avg_albedo[0] * mean[0], …], None =>
    draw_cmd.avg_albedo }`, then `avg_albedo_r: gi_albedo[0], …`.
  - `triangle.frag`: `vec3 tColor = tAlbedo * vec3(tInst.avgAlbedoR, tInst.avgAlbedoG,
    tInst.avgAlbedoB);`, guarded by a comment that still asserts "multiply by the hit's
    canonical avgAlbedo (the material diffuse_color) … For textured content avgAlbedo is the
    white tint, so detail is preserved" — both clauses were true before #1628 and are false
    now.
  - Every other terminus uses `rayHitAlbedo(mat, baseRgb) = max(baseRgb * vec3(mat.diffuseR,
    mat.diffuseG, mat.diffuseB), vec3(0.0))`: `traceReflection` (`hitColor`), the GI path loop
    (`hitAlbedo`), `traceWaterRay`, and `traceShadowTransmittance`'s glass tint.
  - `bindings.glsl`'s own field comment — "offset 96 — kept for `caustic_splat.comp` (set 0
    reads, not migrated)" — no longer describes the readership; `triangle.frag` reads it too.
- **Impact**: Every surface seen *through* refractive glass renders darker than the same
  surface seen directly or in a mirror, by that surface's own mean texel luminance (typically
  0.2–0.5 for Bethesda diffuse maps, i.e. roughly 2–5×). Untextured / vertex-coloured content
  (Cornell walls, the `--cornell` harness) is unaffected because `handle_avg_rgb` returns
  `None` for fallback handles, which is exactly why the Cornell probe cannot surface it.
  Blast radius: all games, every `MATERIAL_KIND_GLASS` draw that resolves a textured terminus.
  Visual-only — no index goes out of range.
- **Related**: #1628 (introduced the semantic change), #789 / `f1b6e1e9` (introduced the
  read), #804 (removed the `GpuMaterial` copy that would otherwise have been the natural
  source), #1098 / #1230 (the still-open-in-spirit "migrate `avg_albedo` off `GpuInstance`"
  thread).
- **Suggested Fix**: Replace `tAlbedo * vec3(tInst.avgAlbedo*)` with `rayHitAlbedo(tMat,
  tAlbedo)` so the terminus uses the same texture × `mat.diffuse*` rule as every sibling path,
  and correct the stale comment block. If the texel-mean folding is wanted for refraction as
  well, it must replace — not multiply — the texture sample. Also tighten `bindings.glsl`'s
  `avgAlbedoR` comment to name its real readers.

---

### REN-D2-02: `shader-pipeline.md`'s `GpuLight` table documents a `shadow_policy` field and a `decodeShadowPolicy` helper that no longer exist — the live `params.z` is the ray-query cull mask
- **Severity**: MEDIUM
- **Dimension**: Ray Queries
- **Location**: `docs/engine/shader-pipeline.md` (`### GpuLight — 64 bytes, SSBO (Set 1,
  Binding 0)` table, rows at offsets 56 and 60–63, plus the `type` row); live contract:
  `GpuLight::params` in `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs`,
  `decodeVisibilityMask` / `visibilityOpaqueMask` / `visibilityMaskNeedsTrace` in
  `crates/renderer/shaders/include/shadow_common.glsl`
- **Status**: NEW (re-drift after #2252's fix — the doc side is now wrong in a *different* way)
- **Description**: The audit-designated authoritative GPU-layout doc describes offset 56 as
  `shadow_policy` "`SHADOW_POLICY_*` encoded as f32 — see `decodeShadowPolicy` in
  `shadow_common.glsl`", and offsets 60–63 as *(reserved)*. Neither is true. `5798e467`
  (2026-08-09, "Refactor visibility layers and adaptive ray budget management") replaced the
  shadow-policy encoding with the `VisibilityMask` bitfield: `params.z` is now the explicit
  visibility-layer mask that `decodeVisibilityMask` turns into the **`cullMask` argument of
  every `rayQueryInitializeEXT` on the direct-shadow path**, and `params.w` carries the
  `AttenuationModel` discriminant that `pointSpotAtten` branches on. No symbol named
  `decodeShadowPolicy` or `SHADOW_POLICY_*` exists anywhere in `crates/renderer/src` or
  `crates/renderer/shaders` — the only surviving `SHADOW_POLICY_NONE` mentions are two prose
  comments. The doc's `type` row ("0 = point, 1 = spot, 2 = directional") also omits type 3
  (ambient fill), which `giLightSample` explicitly rejects with `if (lightType > 2.5) return
  false;` and which `bindings.glsl` does document.
- **Evidence**:
  - `gpu_types.rs`: "`x` = legacy attenuation exponent; `y` = finite luminous-source radius;
    `z` = explicit `VisibilityMask` bits encoded as an exact f32 integer; `w` =
    `AttenuationModel` discriminant encoded as f32."
  - `shadow_common.glsl`: `uint decodeVisibilityMask(float encodedMask) { return
    uint(max(floor(encodedMask + 0.5), 0.0)) & VISIBILITY_MASK_FULL; }` — and
    `traceShadowTransmittance` / `traceShadowBinary` take that value as the `cullMask`.
  - `grep -rn "decodeShadowPolicy\|SHADOW_POLICY" crates/renderer/shaders/ crates/renderer/src/`
    returns only comment text (`triangle.frag` and `restir.rs`) plus the doc line itself.
  - `git log -S` confirms the ordering: `d2333818` (2026-08-02) fixed #2252 by writing the
    shadow-policy rows; `5798e467` (2026-08-09) changed the code without touching the doc.
- **Impact**: The doc is the stated reference for anyone touching a ray-query cull mask. A
  reader following it would look for a non-existent decoder, treat `params.w` as free padding
  (it is the live attenuation-model selector — writing there changes every point/spot light's
  falloff curve), and miss that `params.z` is a *layer bitfield* whose bits must line up with
  `shadow_mask_for_instance`'s TLAS-side buckets in `acceleration/tlas.rs`. Not a runtime
  defect; a wrong entry in the contract that the severity guidance explicitly says not to
  treat as a typo.
- **Related**: #2252 (the previous fix of these same rows), `5798e467`, #2781 (OPEN — the
  sibling drift on the binding-11 row of the same doc).
- **Suggested Fix**: Rewrite the offset-56/60 rows as `visibility_mask` (`VISIBILITY_LAYER_*`
  bits, consumed as the ray-query `cullMask` via `decodeVisibilityMask`) and
  `attenuation_model` (`ATTENUATION_MODEL_*`), and add type 3 (ambient fill, never a GI/shadow
  candidate) to the `type` row.

---

### REN-D2-03: `shader-pipeline.md`'s Set-1 descriptor table stops at binding 17 and its `material_kind` table omits kind 103
- **Severity**: LOW
- **Dimension**: SSBO/Indexing
- **Location**: `docs/engine/shader-pipeline.md` (`## Descriptor Sets` table; `**material_kind**
  (offset 88)` table); live contract: `build_scene_descriptor_bindings` in
  `crates/renderer/src/vulkan/scene_buffer/buffers.rs`, `MATERIAL_KIND_FIRE_REFRACTION` in
  `crates/renderer/shaders/include/shader_constants.glsl`
- **Status**: NEW (sibling of OPEN #2781, which covers a different row of the same table)
- **Description**: Two omissions in the tables this dimension audits against.
  (a) `build_scene_descriptor_bindings` declares **binding 18** — the previous-frame rigid
  instance model matrices, `STORAGE_BUFFER`, vertex stage, whose entries deliberately align
  index-for-index with binding 4 so `gl_InstanceIndex` addresses both. It landed in `33d9a468`
  (2026-07-22) and has never appeared in the doc's Set-1 table, which ends at 17. That is
  precisely the "does the shader read the offsets the Rust upload writes" question this
  dimension exists to answer, and the doc silently claims the answer for a binding it doesn't
  list.
  (b) The `material_kind` table lists 0–19, 100, 101, 102 but not `103`
  (`MATERIAL_KIND_FIRE_REFRACTION`), even though it is a live generated constant that
  `shadow_transport.glsl` branches on (`effectCard`, the skip that keeps fire proxies from
  casting shadows, #2224) and that `triangle.frag` uses to reinterpret `mat.ior` as a 0–1
  distortion scalar (#2232).
- **Evidence**:
  - `buffers.rs`: "// Binding 18: previous rigid-instance model matrices (vertex shader).
    Entries align with binding 4's current-frame instance array after sorting/batching, so
    `gl_InstanceIndex` addresses both…" followed by the `.binding(18)` push. The doc's table's
    last row is `| 1 | 17 | STORAGE_BUFFER | ReSTIR reservoir buffer (previous frame) |`.
  - `shader_constants.glsl`: `#define MATERIAL_KIND_FIRE_REFRACTION 103u`;
    `shadow_transport.glsl`: `bool effectCard = hitMat.materialKind ==
    MATERIAL_KIND_EFFECT_SHADER || hitMat.materialKind == MATERIAL_KIND_FIRE_REFRACTION;`.
    `grep -n "FIRE_REFRACTION\|103" docs/engine/shader-pipeline.md` returns nothing.
- **Impact**: Documentation only. Cost is paid by future readers and by audits that use the
  table as the completeness reference for Set 1 — an undocumented binding is one nothing
  checks for lockstep, and #2748 already shows this family of guard being presence-only.
- **Related**: #2781 (OPEN, binding-11 row of the same table), #1948 / #1915 (CLOSED, the
  previous round of Set-1 table catch-up for bindings 15/16/17), #2224, #2232.
- **Suggested Fix**: Add the binding-18 row (noting the index-alignment contract with binding
  4) and the `103 | MATERIAL_KIND_FIRE_REFRACTION` row. Worth folding into #2781's fix so the
  table is corrected in one pass.

---

### REN-D2-04: `traceReflection`'s `hitBase` is written and never read
- **Severity**: LOW
- **Dimension**: Ray Queries
- **Location**: `traceReflection` in `crates/renderer/shaders/include/raytrace.glsl`
- **Status**: NEW
- **Description**: `vec4 hitBase = vec4(0.0);` is declared alongside the other committed-hit
  carry-outs (`hitInstanceIdx`, `hitPrimitiveIdx`, `hitBary`, `hitUV`) and assigned
  `hitBase = candidateBase;` when the loop commits, but nothing after the loop reads it — the
  committed surface is re-sampled from scratch as `hitBaseRgb = sampleRayHitBase(hitInst,
  hitMat, hitUV, mipBias).rgb` because the coverage probe deliberately samples at LOD 0 while
  the shading sample needs the roughness mip bias. The dead local is a trap for the next
  reader, who may reasonably assume the coverage sample is being reused and "optimise away"
  the second fetch, silently dropping the roughness-scaled blur that makes rough-metal
  reflections noise-free.
- **Evidence**: `grep -n "hitBase" crates/renderer/shaders/include/raytrace.glsl` → line 72
  (declaration), 106 (assignment), and then only `hitBaseRgb` at 157/158/166. The three
  siblings that *are* read (`hitPrimitiveIdx`, `hitBary`, `hitUV`) sit in the same block.
- **Impact**: None at runtime — glslang dead-code-eliminates the local. Maintenance hazard
  only.
- **Related**: #1029 (the `traceReflection` return contract this block feeds), #1017.
- **Suggested Fix**: Delete `hitBase` and the `hitBase = candidateBase;` assignment, and note
  at the `sampleRayHitBase` call why the coverage probe's LOD-0 sample cannot be reused.

---

## Summary

| ID | Severity | Dimension | Title |
|---|---|---|---|
| REN-D2-01 | MEDIUM | SSBO/Indexing | Refraction terminus double-counts the hit texture via `avgAlbedo` |
| REN-D2-02 | MEDIUM | Ray Queries | `shader-pipeline.md` `GpuLight` rows describe a removed `shadow_policy`; live `params.z` is the ray-query cull mask |
| REN-D2-03 | LOW | SSBO/Indexing | Set-1 descriptor table omits binding 18; `material_kind` table omits kind 103 |
| REN-D2-04 | LOW | Ray Queries | Dead `hitBase` carry-out in `traceReflection` |

No CRITICAL or HIGH findings. The two severity floors this dimension guards —
SSBO index mismatch (CRITICAL) and ray self-intersection / wrong tMin (HIGH) —
both came back clean, and all seven named regression guards (#820, #789, #1438,
#883f57cd thin-glass + ReSTIR surface ID, #d523b9b3 spatial normal cone,
#ae285062 BC1 punch-through, #1017/#2462/#2482 tMin + reach) are intact.

---

## Dimension 8



Run: `rt-deep` suite, 2026-08-14. Repo `/mnt/data/src/gamebyro-redux` @ `main` (205744ae).
Dedup baseline: `/tmp/audit/renderer/issues.json` (2813 issues, 251 OPEN) +
`docs/audits/AUDIT_RENDERER_2026-08-12.md`, `…-08-12b.md`, `…-08-07.md`.

## Scope & Coverage

### Files read in full
- `crates/renderer/shaders/svgf_temporal.comp`
- `crates/renderer/shaders/svgf_atrous.comp`
- `crates/renderer/shaders/composite.frag`
- `crates/renderer/shaders/presentation.frag`
- `crates/renderer/src/vulkan/context/post_passes.rs`
- `crates/renderer/src/vulkan/svgf.rs` (module header, α/reset helpers,
  `write_descriptor_sets`, `create_atrous_resources`, `write_atrous_descriptor_sets`,
  `indirect_view`, `initialize_layouts`, `upload_params`, `dispatch`,
  `mark_frame_completed`, `recreate_on_resize`)
- `crates/renderer/src/vulkan/composite.rs` (`CompositeParams`, `HDR_FORMAT`,
  render-pass + subpass deps, framebuffers, samplers, descriptor layout/writes,
  blend state, `dispatch`, `recreate_on_resize`, `fall_back_to_raw_hdr`)

### Files read partially (cross-contract only)
- `crates/renderer/shaders/triangle.frag` — `outMotion` / `outMeshID` /
  `outRawIndirect` / `outAlbedo` / `auxiliaryAlpha` / `indirectLight` sites
- `crates/renderer/shaders/taa.comp` — alpha-lane forwarding only
- `crates/renderer/src/vulkan/pipeline.rs` — `blend_gbuffer_attachments`,
  `coverage_alpha_factors`
- `crates/renderer/src/vulkan/taa.rs`, `gbuffer.rs`, `context/draw.rs`,
  `context/mod.rs`, `context/resize.rs` — only at the sites Dim 8 must agree with
- `docs/engine/shader-pipeline.md`

### Checklist items verified clean (recast as intact regression guards)

| Item | Status |
|---|---|
| SVGF history ping-pong reads prev / writes current | Intact — one `prev = (f + 1) % MAX_FRAMES_IN_FLIGHT` in `write_descriptor_sets` drives bindings 3/4/5/10 together; `MAX_FRAMES_IN_FLIGHT >= 2` compile assert still present |
| Reprojection motion-vector convention | Intact — `triangle.frag` writes `outMotion = (currNDC - prevNDC) * 0.5`; `svgf_temporal.comp` consumes `prevUV = uv - motion`. Algebraically consistent with `NDC = UV*2-1` |
| Mesh-ID disocclusion rejection | Present, but see **Existing: #2767** (namespace collision) — not re-reported |
| Stable surface ID (`883f57cd`) | Intact — `triangle.frag`: `meshIdBase = alphaBlendFrag ? sortedInstanceId : stableSurfaceId`, `outMeshID = meshIdBase \| (alphaBlendFrag ? 0x80000000u : 0u)`; `surface_id = draw_cmd.entity_id.wrapping_add(1)` in `draw.rs` |
| Aux-MRT alpha lanes not hardcoded to 1.0 (#883f57cd) | Intact — `float auxiliaryAlpha = isAlphaBlend ? finalAlpha : 1.0;` written into both `outRawIndirect.a` and `outAlbedo.a`; the emissive/effect early-outs deliberately write `0.0` to ride `auxiliary_blend`'s `SRC_ALPHA/ONE_MINUS_SRC_ALPHA` dst-preserve, per their own inline rationale |
| Blend α clamped; first frame uses current | Intact — `alphaC = max(floorC, 1/(histAge+1))`, `params.z` reset gate driven by `should_force_history_reset(frames_since_creation[frame])` |
| Firefly clamp hoisted ahead of `hasHistory` (`48906670` / REG-07 / #1481) | **Intact** — the 3×3 spatial mean+3σ block sits above `if (hasHistory)`; the no-history store below it consumes the already-clamped `currInd`/`currLum` |
| Dispatch covers exactly the image | Intact — `width.div_ceil(8)` / `height.div_ceil(8)` against `WORKGROUP_X/Y == 8`; both shaders bounds-check `p` |
| À-trous ping-pong / final slot | Intact — `ATROUS_ITERATIONS = 3` with the odd-count compile assert; `indirect_view()` returns `atrous_color[frame*2 + atrous_final_pp()]` = slot 0; WAR between iterations covered by the per-iteration COMPUTE→COMPUTE barrier |
| ACES lives in `presentation.frag`, not `composite.frag` | **Intact** — `grep aces composite.frag` = 0 hits; `presentation.frag::aces()` applied to `graded * params.exposure` |
| Composite emits linear HDR to an offscreen image, not the swapchain | Intact — attachment 0 is `HDR_FORMAT` `scene_image_views[i]`, `final_layout = SHADER_READ_ONLY_OPTIMAL`; presentation owns `PRESENT_SRC_KHR` |
| Bloom added upstream of tone-map | Intact — `combined += bloom * BLOOM_INTENSITY` in `composite.frag`, unconditional (post-#2233) |
| Caustic added to `direct`, never to the SVGF-denoised indirect | **Intact** — `combined = direct + indirect * albedo + caustic;` with `caustic = albedo * causticRadiance`, a separate summand; #2508's `caustic_flags.x` alias gate and #1575's promote-to-float-before-add both present |
| Per-frame history descriptor swap survives resize | Intact — `SvgfPipeline::recreate_on_resize` rewrites both `write_descriptor_sets` and `write_atrous_descriptor_sets`, zeroes `frames_since_creation`/`dispatched_this_frame`, and self-chains `initialize_layouts` (#1031) |
| `record_post_passes` ordering vs `shader-pipeline.md` | **Matches** — `svgf → caustic_splat → volumetrics → taa → ssao → bloom → composite → upscale → presentation` |
| Committed SPIR-V vs GLSL (the #1950 / #2217 hazard) | **In sync** — recompiled `composite.frag`, `composite.vert`, `presentation.frag`, `svgf_temporal.comp`, `svgf_atrous.comp` with `glslangValidator -V -I.`; all five byte-identical to the committed `.spv` |
| Both-slots fence wait backing SVGF's prev-slot G-buffer reads (#282) | Intact (`draw.rs`, `wait_for_fences(&[in_flight[frame], in_flight[prev]], …)`) — this is also what makes composite's single shared `depth_view` safe |
| TAA→composite alpha contract | Intact — `taa.comp` forwards `currA` on all three `imageStore` sites, TAA output is `R16G16B16A16_SFLOAT`, so `direct4.a` (the #2466 coverage lane) survives the rebind to TAA output |
| `svgf_temporal` NaN/Inf history pre-filter (#903) | Intact, ahead of the weighted sum |
| SSAO modulates indirect only | Verified, but the consumer is `triangle.frag` (`combinedAO`), **not** `composite.frag` — composite has no AO binding. See **Existing: #2798** |

### Items I could not verify, and why
- **Barrier/layout correctness** of the SVGF→composite and à-trous chains beyond
  reading the masks. Per the standing no-speculative-Vulkan rule these would need
  `BYRO_VALIDATION=1` + RenderDoc; nothing here looked wrong, so no finding is
  raised. Confirming signal would be a sync-val `READ_AFTER_WRITE` on
  `svgf_indirect_*` / `svgf_atrous_*` / the composite scene image.
- **Reachability of `outMoments` μ₂ half-float overflow.** `moments.g = currLum²`
  in `R16G16B16A16_SFLOAT` saturates to `+Inf` once demodulated indirect luminance
  exceeds 256, and `svgf_temporal`'s own `isinf(sMom)` guard would then drop that
  pixel's history every frame. I could not demonstrate indirect luminance >256 on
  any content path (`indirectLight` has no absolute clamp, but the 3×3 firefly
  clamp bounds outliers and the module doc only claims "100+"), so this is
  recorded as an observation, not a finding.
- **Whether the `preResolveDither()` blue-noise injection (amplitude `1.0/1024`,
  re-randomised per frame index) degrades FSR temporal accumulation.** It is
  applied in linear HDR *upstream* of the upscaler; measuring the effect needs the
  `scripts/fsr-bench-matrix.sh` SSIM matrix, which is out of scope here. No finding.

### Skill-checklist text that is itself stale (not a code defect)
Already recorded as **P-2** in `AUDIT_RENDERER_2026-08-12b.md` and re-confirmed:
Dimension 8's "Fog applied to direct only, not indirect" does not describe the
code — `composite.frag` applies `combined = combined * vol.a + vol.rgb` to the
whole composite (Frostbite §5.3), which is correct; what is true is that fog is
applied *downstream of SVGF*, so it never enters denoiser history (#428).
Likewise the glass accumulator (binding 5) is a `usampler2DArray`, not a
`usampler2D`. `docs/engine/shader-pipeline.md` is right on both points.

### Candidates raised and then disproved (not reported)
- *`svgf_atrous.comp` has no `ALPHA_BLEND_NO_HISTORY` early-out for the centre
  pixel, unlike `svgf_temporal.comp`.* Disproved as a separate defect: once
  #2767's tap predicate is fixed, an alpha-blended centre only accepts taps from
  its own draw, and spatially smoothing those pixels' unaccumulated 1-SPP indirect
  is the only filtering they get. Subsumed by #2767.
- *`fall_back_to_raw_hdr` could be silently undone by a later resize, freezing the
  image.* Disproved: `context/resize.rs` clears `taa_failed`/`svgf_failed` in the
  same path that rebinds, and the TAA-init-failure arms destroy the pipeline
  before the rebind guard runs.
- *Composite's swapchain alpha carries the coverage lane.* Harmless —
  `swapchain.rs` uses `CompositeAlphaFlagsKHR::OPAQUE`.
- *`record_bloom_pass`'s claim that its `if let Some(bloom)` guard is runtime-dead
  is false.* Disproved: `VulkanContext::new` really does hard-fail via
  `anyhow::anyhow!("Bloom pipeline failed to initialize — composite requires the
  bloom output view for binding 7 (M58)…")` at the `bloom_views` match. Only the
  **line anchor** in the doc is wrong — folded into REN-D8-03.

---

## Findings

### REN-D8-01: Composite's `is_sky` branch composites `direct` behind the sky but still drops `indirect * albedo`
- **Severity**: MEDIUM
- **Dimension**: Denoiser/Composite
- **Location**: `crates/renderer/shaders/composite.frag` — the `if (is_sky)` arm
  of `main()` (`combined = compute_sky(dir) * (1.0 - coverage) + direct;`),
  against the sibling `else` arm (`combined = direct + indirect * albedo + caustic;`)
- **Status**: NEW (residual half of **#2466** / REN-D8-N01, which is CLOSED and
  whose fix is present and correct as far as it goes)
- **Description**: #2466 established that an alpha-blended fragment with nothing
  opaque behind it leaves depth at the cleared `1.0` — blend pipelines run
  `depth_write_enable(false)` — so composite classifies the pixel as sky. The fix
  restored the pixel's **direct** term by weighting the procedural sky against the
  `direct4.a` coverage lane. The **indirect** term was not restored: the sky arm
  never reads `indirectTex` or `albedoTex`, so the same fragment's
  albedo-demodulated GI is still discarded. The identical surface drawn one pixel
  to the side — over opaque geometry — gets `indirect * albedo` added. The result
  is an exterior-only brightness discontinuity along the silhouette where an
  alpha-blended draw crosses the horizon.
- **Evidence**:
  ```glsl
  vec3 combined;
  if (is_sky) {
      vec3 dir = screen_to_world_dir(fragUV);
      float coverage = clamp(direct4.a, 0.0, 1.0);
      combined = compute_sky(dir) * (1.0 - coverage) + direct;   // no indirect term
  } else {
      vec3 indirect = texture(indirectTex, fragUV).rgb;
      vec3 albedo   = texture(albedoTex, fragUV).rgb;
      ...
      combined = direct + indirect * albedo + caustic;
  }
  ```
  The dropped term is genuinely non-zero at such a pixel. `triangle.frag`'s tail
  writes `outRawIndirect = vec4(indirectLight, auxiliaryAlpha)` and
  `outAlbedo = vec4(albedo, auxiliaryAlpha)`, and `pipeline.rs::blend_gbuffer_attachments`
  gives attachments 4 and 5 `auxiliary_blend` (`SRC_ALPHA`/`ONE_MINUS_SRC_ALPHA`)
  over the zero clear, so both lanes hold coverage-weighted content.
  `svgf_temporal.comp`'s bit-31 early-out passes that value straight through
  (`imageStore(outIndirect, p, vec4(currInd, 1.0))`), and `svgf_atrous.comp`
  filters rather than discards it, so `indirectTex` carries it into composite.
- **Impact**: Exterior only. Narrower than #2466's blast radius, because the draws
  that most often silhouette against sky take early-outs that zero
  `outRawIndirect` first — the `MAT_FLAG_EFFECT_SOFT` / effect-shader arm, the
  `MATERIAL_KIND_NO_LIGHTING` arm, and both glass exits all write
  `outRawIndirect = vec4(0.0)`. What remains affected is **lit** alpha-blended
  geometry reaching the general tail: cloth banners, hanging signs, lit
  alpha-blended decals and card geometry on a skyline. Those render with ambient +
  RT GI missing against sky and present against a wall.
- **Related**: #2466 (REN-D8-N01, the direct half), #2233 (REN-D8-02, the
  bloom/fog half of the same branch), #676 / DEN-11 (the `direct4.a` lane),
  `pipeline.rs::coverage_alpha_factors`.
- **Suggested Fix**: Read `indirectTex`/`albedoTex` unconditionally (they are
  already bound and cheap) and add `indirect * albedo` to the sky arm's
  `combined`, exactly as the geometry arm does. Note while doing so that the
  demodulated reassembly `indirect * albedo` is not linear in the blend operator
  — over a zero clear the product is `coverage²·(I·A)` — so if the sky arm is
  ever made exact rather than consistent, the premultiply must be divided out
  once, not twice. Consistency with the geometry arm is the smaller and safer
  change and is what this finding asks for.

---

### REN-D8-02: `should_force_history_reset`'s doc block is attached to `advance_completed_frames`
- **Severity**: LOW
- **Dimension**: Denoiser/Composite
- **Location**: `crates/renderer/src/vulkan/svgf.rs` — the `///` run ending
  "…this extraction is the regression guard the audit asked for." immediately
  above `advance_completed_frames`; `should_force_history_reset` follows it
  undocumented
- **Status**: NEW
- **Description**: A blank line between the two doc blocks was lost, so the
  paragraph written for `should_force_history_reset` ("Should the temporal pass
  force a full history reset on this frame? … Pinned as a pure helper (#648 /
  RP-2)…") is concatenated onto the doc for `advance_completed_frames`.
  `advance_completed_frames`'s rustdoc therefore *opens* by describing a different
  function's contract, and `should_force_history_reset` — the helper `#648`
  extracted specifically so the reset policy would be discoverable and
  test-pinned — has no doc comment at all.
- **Evidence**: `svgf.rs`, contiguous `///` lines with no separator:
  ```rust
  /// extraction is the regression guard the audit asked for.
  /// Advance the per-FIF history age for whichever slots were dispatched, and
  /// clear their latches.
  ```
  followed by `pub(super) fn advance_completed_frames(…)`, then a bare
  `pub(super) fn should_force_history_reset(frames_since_creation: u32) -> bool`.
  The damage is load-bearing rather than cosmetic: `SvgfPipeline::upload_params`
  says "See `should_force_history_reset`'s doc for the cross-link", and
  `CompositePipeline`-side readers following that pointer land on a function that
  no longer carries it.
- **Impact**: Documentation only — no behavioural change. `cargo doc` renders the
  reset-policy rationale under the wrong symbol and leaves the #648 regression
  guard undocumented, which is exactly the discoverability that extraction bought.
- **Related**: #648 / RP-2, #2146, #917 / REN-D10-NEW-03.
- **Suggested Fix**: Insert the missing blank line so the first block reattaches
  to `should_force_history_reset` (move it below `advance_completed_frames`, or
  move the function above its doc).

---

### REN-D8-03: Cross-file `file:NN` anchors across the denoiser/composite sources have rotted en masse
- **Severity**: LOW
- **Dimension**: Denoiser/Composite
- **Location**: `crates/renderer/shaders/svgf_temporal.comp`,
  `crates/renderer/shaders/svgf_atrous.comp`,
  `crates/renderer/shaders/composite.frag`,
  `crates/renderer/src/vulkan/svgf.rs`,
  `crates/renderer/src/vulkan/context/post_passes.rs`
- **Status**: NEW (same shape as #2773, #2757, #2510, #2755 — none of which covers
  these sites)
- **Description**: The Dim-8 sources navigate almost entirely by bare line
  numbers, and every anchor I checked now points somewhere unrelated. Several
  point at code of the *opposite* kind, which is worse than a dangling pointer:
  a reader following `triangle.frag:267` for the octahedral-encode contract lands
  in the decal-index array and can reasonably conclude the encodings differ.
- **Evidence** (each verified against the live tree today):

  | Comment site | Cited anchor | What actually lives there |
  |---|---|---|
  | `svgf_temporal.comp` header (bindings 9/10) | `triangle.frag:644` — "reads the RG16_SNORM `outNormal` G-buffer attachment" | the `DBG_VIZ_FSR_TEMPORAL` jitter-visualisation block |
  | `svgf_temporal.comp::octDecode` | `triangle.frag:267` / `caustic_splat.comp:91` — "matches" | the `materialDecals[4]` array / a deferred-work note above the `GpuInstance` mirror |
  | `svgf_atrous.comp::octDecode` | `triangle.frag:267` (same) | as above |
  | `svgf_temporal.comp` #675 / #904 comments | "the early-out at line 93", "a plain weighted blend at line 152-153", "the `histAge` weighted-average at line 156", "the early-out near line ~97" | the early-out is the `currID == 0u \|\| (currID & 0x80000000u)` test; the blend and the `histAge +=` accumulation are ~30 lines below the cited numbers |
  | `svgf.rs` (`should_force_history_reset` doc) | `svgf_temporal.comp:81` — "`1.0 = reset history`" | `float currLum2 = currLum * currLum;`. The reset read is `params.z < 0.5` inside `reprojectOk` |
  | `svgf.rs::dispatch` | `draw.rs:170-181` — the both-slots `wait_for_fences` (#282) | a `skinnedVertexAddress` doc block. **The fence wait itself is intact**, ~1300 lines later |
  | `svgf.rs::dispatch` | `taa.rs:789`, `caustic.rs:816`, `volumetrics.rs:846` — "sibling barriers" | a `recreate_history` doc comment, a `dispatch` signature, a descriptor-binding literal — none is a barrier |
  | `composite.frag` binding 8 | `composite.rs:360` — "the existing integer-format-sampling rule" | HDR image sub-allocation. The rule is the `nearest_sampler` field doc / its `create_sampler` call |
  | `composite.frag::compute_sky` | "the sky-lower mix at L107", "the disc faded correctly (line 222)" | both are inside comment prose; the real `mix(horizon, params.sky_lower.xyz, below)` and the `sky += disc_color * …` sites are far below |
  | `post_passes.rs::record_bloom_pass` | `context/mod.rs:1715-1717` — the `rebind_hdr_views` call | a `GBufferFormats` struct literal. The rebind is ~1080 lines later |
  | `post_passes.rs::record_bloom_pass` | `context/mod.rs:1958-1967` — the bloom hard-fail | a neutral-texture upload. The hard-fail (`anyhow::anyhow!("Bloom pipeline failed to initialize — composite requires the bloom output view for binding 7 (M58)…")`) is real but ~720 lines later |
  | `post_passes.rs::record_volumetrics_pass` | `caustic.rs:627` / `draw.rs:1648` — the TLAS gate it mirrors | a view-creation error path / an index-buffer upload |
  | `post_passes.rs::record_volumetrics_pass` | `draw_frame, ~line 2960` — cluster_cull's trailing barrier | `upload_previous_models` |

- **Impact**: Documentation only, but concentrated on the load-bearing
  cross-pass contracts of this dimension: the mesh-ID/normal encoding shared by
  three shaders, the fence that makes the shared depth view and prev-slot
  G-buffer reads legal, and the bloom/TAA descriptor rewiring. Anyone auditing or
  refactoring here is routed to the wrong code by roughly a dozen pointers, and
  two of them (`triangle.frag:267`, `draw.rs:170-181`) invite the false conclusion
  that a real invariant is absent. The repo already ruled on this class in
  **#1040** ("Audit-skill anchor rot — switch bare line numbers to symbol-based
  anchors", CLOSED) and the audit protocol mandates symbols over line numbers;
  the renderer shaders never got the sweep.
- **Related**: #1040, #2773, #2757, #2510, #2755, `_audit-common.md`
  "Path-Reference Convention" and `audit-renderer/SKILL.md` "Symbols, not line
  numbers".
- **Suggested Fix**: Replace each `file:NN` with the symbol it means
  (`triangle.frag`'s `octEncode`/`outNormal` write, `svgf_temporal.comp`'s
  `octDecode`, `draw.rs`'s `wait_for_fences` on `in_flight[frame]`/`in_flight[prev]`,
  `composite.rs`'s `nearest_sampler`, `context/mod.rs`'s `rebind_hdr_views` and the
  `bloom_views` match, `caustic.rs`'s `tlas_handle` gate). Extending
  `.claude/commands/_audit-validate.sh`'s advisory pass to flag `\w+\.(rs|comp|frag|vert):\d+`
  inside `crates/renderer/` would keep it from re-accumulating.

---

