# #3663 — PERF-D4-2026-08-30-01: the instance / previous-model / indirect dirty gates are defeated by the per-frame depth re-sort, so the documented steady-state saving only materialises with a parked camera

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D4-2026-08-30-01`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: medium,performance,renderer,pipeline,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3663

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: MEDIUM
- **Dimension**: SSBO Sizing & Upload
- **Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs:557-566` (`upload_instances`
  gate), `:612-616` (`upload_previous_models`), `:730-736` (`upload_indirect_draws`);
  `byroredux/src/render/mod.rs:519-530` (the opaque `draw_sort_key` arm) and `:545-571`
  (`sort_draw_commands`); `byroredux/src/render/static_meshes.rs:409` (`sort_depth`
  computation); `crates/renderer/src/vulkan/context/draw.rs:2801-2806`, `:3056`, `:3311-3319`
  (instances built in sorted order, then uploaded)
- **Status**: NEW
- **Description**: The three sibling dirty gates skip the copy + flush when the current
  frame's slice hashes byte-identical to the last one written into this frame-in-flight slot.
  Their docstrings justify the win as *"static interiors produce byte-identical slices each
  frame"*. That premise does not hold whenever the camera moves. `gpu_instances` is built by
  walking `draw_commands` **after** `sort_draw_commands`, and the opaque sort key's
  penultimate component is `cmd.sort_depth` — a full-precision `f32`-to-sortable-`u32`
  reinterpretation of clip-space `w`, recomputed per draw per frame. Any camera translation or
  rotation that inverts the depth order of two draws sharing a `mesh_handle` permutes their
  `GpuInstance` entries. The bytes are the same multiset; the *slice* is not, so the hash
  differs and all three gates miss.

  Instanced batching guarantees the collision case is the common one: the whole point of
  grouping on `mesh_handle` is that real cells place the same mesh many times, and those are
  exactly the draws whose relative order `sort_depth` arbitrates. The per-instance payload is
  otherwise stable under camera motion — `GpuInstance.model` is render-origin-relative and the
  origin only re-snaps on a cell-grid crossing (`RENDER_ORIGIN_SNAP`), and
  `texture_index` / `material_id` / `flags` do not depend on the view.

  The codebase already documents the reordering, in the field that exists to work around it:
  `GpuInstance.surface_id` is *"Stable draw identity used by temporal direct-shadow reservoirs.
  Unlike the per-frame instance-buffer index, this follows the ECS entity when **depth sorting**
  or animated actors **reorder draw commands**"* (`gpu_types.rs:157-160`), pinned by
  `restir_history_uses_stable_surface_id_not_instance_order`. So one part of the renderer treats
  per-frame instance reordering as a given while another part's optimisation assumes it away.
- **Evidence**:
  ```rust
  // byroredux/src/render/mod.rs:519-530 — opaque arm of draw_sort_key
  (rt_only, 0u8, 0u8, cmd.render_layer as u32, cmd.two_sided as u32, 0, 0,
   pack_depth_state(cmd) as u32,
   cmd.mesh_handle, // group identical meshes
   cmd.sort_depth,  // front-to-back within group   <-- view-dependent, recomputed per frame
   cmd.entity_id)
  ```
  ```rust
  // byroredux/src/render/static_meshes.rs:409
  let sort_depth = f32_sortable_u32(clip.w);   // f32 bit pattern, no quantisation
  ```
  ```rust
  // crates/renderer/src/vulkan/scene_buffer/upload.rs:566
  let hash = hash_instance_slice(&instances[..count]);
  if self.last_uploaded_instance_hash[frame_index] == Some(hash) { return Ok(()); }
  ```
- **Impact**: In gameplay the gates are not merely ineffective, they are net negative: a miss
  pays the full `FxHasher` pass over the slice **and** the memcpy + flush it was meant to
  avoid. At the docstring's own MedTek reference workload (7 359 draws) the per-frame read
  overhead is `7359 × (160 + 64 + 20) B` ≈ 1.80 MB hashed, on top of the same 1.80 MB copied —
  ~108 MB/s each at 60 fps. The documented ~54 MB/s saving is realised only while the camera is
  completely still (menus, `--bench-hold`, a parked bench camera), which is also the only state
  the gates were ever observed in. Nothing renders incorrectly.
- **Related**: #1134 (`upload_instances` gate), #878 (`upload_materials` gate), #1809
  (`upload_indirect_draws` gate), #2036 (`upload_lights` gate), #2692 (the 112→128 B figure
  correction in the same docstring, now itself 160 B). Distinct from #3246 (animated material
  float bits in the *material* dedup key — a different gate and a different mechanism).
  `PERF-D2-2026-08-30-01/-02` (this session) cover the sort's *cost*, not its effect on the
  upload gates.
- **Suggested Fix**: Quantise the opaque tiebreaker — replace `cmd.sort_depth` at slot 9 with a
  coarse bucket (e.g. the top 8–10 bits of the sortable `u32`) so sub-bucket camera motion no
  longer permutes the slice while front-to-back early-Z ordering is preserved at the granularity
  that actually matters. Failing that, correct the three docstrings so the next reader does not
  budget for a saving that only exists at rest.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
