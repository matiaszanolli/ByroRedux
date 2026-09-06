# #3976 — REN-2026-09-06-D9-01: a failed first-sight `bind_inverses` upload still lets the SAME frame build that entity's skinned BLAS out of never-written device memory

**Labels**: critical, renderer, shaders, sync, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D9-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: CRITICAL
- **Dimension**: Skinning
- **Location**: `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs` (`VulkanContext::dispatch_skin_and_cluster`, the `upload_pending_bind_inverses` error arm), `crates/renderer/src/vulkan/context/skinned_blas_refit.rs` (`VulkanContext::record_skinned_blas_refit`), `crates/renderer/src/vulkan/scene_buffer/upload.rs` (`SceneBuffers::seed_persistent_bind_inverses_identity`)
- **Status**: NEW (the residual half of the just-closed #3569; no open issue matches — grepped `bind_inverse|first-sight|uninitial|garbage|palette` against the 151 open titles)
- **Description**: `709de0e6` fixed the *bookkeeping* half of #3569 — a failed
  `upload_pending_bind_inverses` now latches `bind_inverse_upload_failed`, and
  `app_frame.rs` requeues the drained entries so they retry next frame. It did **not**
  gate the rest of that same frame. `bind_inverse_upload_failed` is written in exactly
  two places (`= false` at the top of `draw_frame`, `= true` in the error arm) and read
  in exactly one (`app_frame.rs`'s rollback check) — no consumer inside
  `dispatch_skin_and_cluster` or `record_skinned_blas_refit` looks at it. So on the
  failing frame the engine goes on to (1) run `skin_palette.comp` over the *dense*
  `0..bone_count` range including the never-written slot, (2) dispatch
  `skin_vertices.comp` for the entity against that palette, and (3) issue the entity's
  first-sight skinned-BLAS **BUILD** over the resulting positions.
  `bind_inverses_persistent` is created by `GpuBuffer::create_device_local_uninit` and
  only **slot 0** is ever seeded (`seed_persistent_bind_inverses_identity`, #1191) — every
  other slot is raw uninitialised device memory (or, on a reused free-list slot, the
  previous tenant's matrices) until its own first-sight copy lands.
- **Evidence**:
  - Error arm returns `0`, so the `if pending_capped > 0 { … record_pending_bind_inverse_copies(…) }`
    block is skipped entirely — nothing writes the slot, and no `TRANSFER_WRITE → SHADER_READ`
    barrier is emitted for it.
  - The palette dispatch guard immediately below is
    `bone_count > 0 && !skip_skin_gpu_refresh && (bone_world_copy_recorded || pending_capped > 0)`.
    On a first-sight frame `skin_state_dirty` is true (the pending list is non-empty) so
    `skip_skin_gpu_refresh` is `false`, and the entity is in `pose_dirty` (first sight →
    `SkinSlotPool::try_mark_pose_dirty` has no prior hash → always dirty), so
    `upload_bone_worlds` writes its slot and `bone_input_upload_bytes(frame) > 0` makes
    `bone_world_copy_recorded` true. **The dispatch runs.**
  - `skin_palette.comp::main` is unconditional per slot: `palette[slot] = boneWorld[slot] * bindInverses[slot];`
  - `record_skinned_blas_refit` walks `dispatches` (built from `draw_commands` with
    `bone_offset != 0` — which the entity has, its slot was assigned pre-draw), takes the
    `needs_blas` branch, pushes into `first_sight_builds`, and the batch is recorded on
    this frame's command buffer via `build_skinned_blas_batched_on_cmd`. The only
    suppressors on that path are `failed_skin_slots` / `failed_skin_blas` (both empty on
    first sight) and `mesh.rt_capable`.
  - Recovery is *not* complete next frame: the requeue + `rollback_pending_pose_commits`
    do get the slot uploaded and the entity re-marked dirty, but `needs_blas` is now
    `false`, so the entity gets an **UPDATE-mode refit**, which by design preserves the
    BVH topology the poisoned BUILD produced. A fresh BUILD only happens after
    `should_rebuild_skinned_blas` trips `SKINNED_BLAS_REFIT_THRESHOLD` (600 frames, ~10 s)
    or the LRU sweep drops the entry.
- **Impact**: An acceleration structure built over undefined memory. `boneWorld × <arbitrary bits>`
  can yield NaN/Inf, so the BLAS' vertex positions — and therefore the AABB it contributes
  to the TLAS with an identity instance transform — are unbounded or non-finite. This is
  the `_audit-severity.md` "BLAS/TLAS build with wrong geometry" row: every ray query that
  frame (shadows, reflections, GI, water refraction, caustic splat) traverses it, and the
  degenerate topology survives ~600 frames of refits. It is exactly the #2467 failure
  class the `bones[bone_offset]` rigid fallback was introduced to close, re-entered
  through a different door. Trigger is rare — the staging buffer is a `GpuBuffer` from
  `GpuBuffer::create_host_visible`, so the `Err` comes from its `mapped_slice_mut`
  ("Buffer has no allocation" / "Buffer not mapped") or its `flush_if_needed`
  (`vkFlushMappedMemoryRanges` failure, i.e. device-lost or OOM) — but
  severity here is impact, not likelihood, and the engine's response to a transient map
  failure should not be to poison the TLAS.
- **Related**: #3569 (`709de0e6`, the bookkeeping half), #1191 / SAFE-D7-NEW-01 (the slot-0
  identity seed this needs the general case of), #2467 / REN-D9-NEW-01 (the identical
  "absolute-space skinned BLAS built from a wrong transform" class), #1796 / D6-02.
- **Suggested Fix**: Two independent options, either sufficient:
  (a) **Make the undefined case defined** — extend `seed_persistent_bind_inverses_identity`
  from slot 0 to the whole `bind_inverses_persistent` buffer (2 MB; needs a staging copy
  or `cmd_fill_buffer`-style path rather than `cmd_update_buffer`'s 64 KiB payload cap).
  A never-uploaded slot then yields `palette = boneWorld × identity`, a well-defined
  bind-pose-at-bone-world transform — visually wrong but finite and traversable, which is
  the same degrade the pool-overflow path already accepts.
  (b) **Gate the frame** — have `record_skinned_blas_refit` consult
  `bind_inverse_upload_failed` and skip the first-sight BUILD (leaving the entity to next
  frame's retry, which the #3569 requeue already guarantees). Cheaper, but only covers the
  upload-failure route, not a future one that leaves a slot unwritten.
  (a) is the structural fix; (b) is the one-line stopgap. Either should carry a source-scan
  guard in the same shape as `bind_inverse_upload_failure_latch_tests`.

---

---

# HIGH

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **FFI**: If the FFI boundary is touched, pointer lifetimes across it are sound
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
