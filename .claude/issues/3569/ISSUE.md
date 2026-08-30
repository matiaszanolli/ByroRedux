# #3569 — REN-2026-08-30-D9-01: a failed first-sight `bind_inverses` upload is swallowed with no requeue — the slot's palette source stays UNDEFINED for the entity's whole residency

**Labels**: `medium,renderer,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3569 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: Skinning
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:2504-2513` (`draw_frame`, the `upload_pending_bind_inverses` call), interacting with `byroredux/src/app_frame.rs:569-573` (`render_one_frame`, the `#1791` requeue) and `crates/renderer/src/vulkan/scene_buffer/buffers.rs:602` (`bind_inverses_persistent`)
- **Status**: OPEN — new
- **Description**: `SkinSlotPool::drain_pending` removes first-sight `(slot, entity)` entries from the pool *irrevocably* before `draw_frame` is called. #1791 / D6-01 built exactly one recovery path for that: if `draw_frame` bails before the skin section, `ctx.skin_dispatch_ran` stays `false` and `render_one_frame` calls `requeue_pending`. That flag does not cover the case where `draw_frame` *reaches* the upload and the upload itself fails. `upload_pending_bind_inverses` returns `Result`, and the call site collapses `Err` to `0` with a `log::warn!`. `pending_capped == 0` then skips `record_pending_bind_inverse_copies` entirely, and `record_skinned_blas_refit` (which sets `skin_dispatch_ran = true`, `crates/renderer/src/vulkan/context/skinned_blas_refit.rs:63`) still runs afterwards — so the requeue branch never fires and the drained entries are dropped on the floor.
- **Evidence**:
  - `draw.rs:2504-2513`:
    ```rust
    let pending_capped = if !bind_inverse_pending_uploads.is_empty() {
        self.scene_buffers
            .upload_pending_bind_inverses(&self.device, bind_inverse_pending_uploads)
            .unwrap_or_else(|e| {
                log::warn!("Failed to upload pending bind_inverses: {e}");
                0
            })
    } else { 0 };
    ```
    `upload_pending_bind_inverses` (`scene_buffer/upload.rs:329-360`) has two fallible steps: `staging.mapped_slice_mut()?` and `staging.flush_if_needed(device)?`.
  - `skinned_blas_refit.rs:63` — `self.skin_dispatch_ran = true;` is set unconditionally at the top of `record_skinned_blas_refit`, which `draw.rs` calls at line 2597, i.e. after the upload block.
  - `app_frame.rs:569-573` — `if !ctx.skin_dispatch_ran { … requeue_pending(…) }` is the only requeue site in the tree (`grep -rn requeue_pending`).
  - `crates/core/src/ecs/resources/skin_slot_pool.rs:163-166` — `allocate` returns early for an entity already in `entity_to_slot`, so a resident entity never re-enters `pending_uploads` on its own.
  - `buffers.rs:602` — `bind_inverses_persistent` is `GpuBuffer::create_device_local_uninit(...)`: never cleared, so an unwritten slot region holds undefined bytes, not zeros.
  - `skin_palette.comp:77` — `palette[slot] = boneWorld[slot] * bindInverses[slot];` — the undefined matrix is multiplied into the palette every frame.
- **Impact**: One dropped first-sight upload permanently corrupts that entity's bone palette for its remaining lifetime in the cell — the palette feeds both `skin_vertices.comp` (so the skinned BLAS is refit against garbage world positions, dragging the TLAS AABB with it) and `triangle.vert`'s inline raster skinning. Symptom is an exploded/vanished actor plus an inflated RT cost, with only a single WARN as evidence. Trigger (host-visible map / flush failure) is rare, which is precisely why the silent-and-permanent shape matters: there is no self-healing frame. This is the same defect class #1791 was filed for, reached through the sibling branch that fix did not cover.
- **Suggested Fix**: Latch the failure on the context (e.g. `self.bind_inverse_upload_failed = true` in the `unwrap_or_else`, reset alongside `skin_dispatch_ran` at `draw.rs:1567`) and widen `app_frame.rs:569` to `if !ctx.skin_dispatch_ran || ctx.bind_inverse_upload_failed`. The entries then reappear on the next `drain_pending` and the persistent SSBO region is written a frame later. Pin it with a source-position test in the style of `skin_dispatch_ran_rollback_scope_tests` (`app_frame.rs:692`) / `skin_dispatch_ran_ordering_tests` (`draw.rs:4577`). Optionally also zero-init `bind_inverses_persistent` so a missed write degrades to a collapsed-to-origin mesh rather than undefined memory — that is a defence-in-depth change, not the fix.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D9-01

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
