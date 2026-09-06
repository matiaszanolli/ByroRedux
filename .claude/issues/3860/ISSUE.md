# #3860: TD2-2026-09-05-03: twelve hand-rolled "create image → allocate → bind → view" chains, while the buffer side of the same problem has been consolidated for a year

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD2-2026-09-05-03) via `/audit-publish`, 2026-09-05. Labels: `low,renderer,vulkan,memory,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3860 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD2-2026-09-05-03), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 2 — Logic Duplication
- **Location** (each is one ~80-line copy of the same chain):
  `crates/renderer/src/vulkan/taa.rs::TaaPipeline::create_history_image` ·
  `crates/renderer/src/vulkan/svgf.rs::SvgfPipeline::create_history_image` ·
  `crates/renderer/src/vulkan/bloom.rs::create_mip` ·
  `crates/renderer/src/vulkan/caustic.rs::CausticPipeline::create_slot` ·
  `crates/renderer/src/vulkan/water_caustic.rs::WaterCausticAccum::create_slot` ·
  `crates/renderer/src/vulkan/volumetrics.rs::VolumetricsPipeline::create_volume` ·
  `crates/renderer/src/vulkan/gbuffer.rs::Attachment::allocate` ·
  `crates/renderer/src/vulkan/exposure.rs::ExposureResource::new` ·
  `crates/renderer/src/vulkan/ssao.rs::SsaoPipeline::new_inner` ·
  `crates/renderer/src/vulkan/placeholder.rs::PlaceholderImage::create` ·
  `crates/renderer/src/vulkan/frame_upscaler.rs::FrameUpscaler::create_outputs` ·
  `crates/renderer/src/vulkan/composite.rs::CompositePipeline::new_inner`
- **Status**: NEW
- **Description**: Every screen-sized GPU image in the renderer is built by the
  same five-step sequence — `vk::ImageCreateInfo` (TYPE_2D, 1 mip, 1 layer,
  `SampleCountFlags::TYPE_1`, `OPTIMAL`, `EXCLUSIVE`, `UNDEFINED`) →
  `device.create_image` → `allocator.lock().allocate(AllocationCreateDesc {
  location: GpuOnly, linear: false, allocation_scheme: GpuAllocatorManaged })`
  → `bind_image_memory` → `create_image_view(TYPE_2D, color_subresource_single_mip())`
  — plus the same three-arm error cleanup (destroy image on alloc failure; free
  alloc *then* destroy image on bind failure; same again on view failure). This
  is copied twelve times. `crates/renderer/src/vulkan/buffer.rs` solved the
  identical problem on the buffer side (`GpuBuffer::create_vertex_buffer` /
  `create_index_buffer` / `create_host_visible` / `create_host_readback` /
  `create_device_local_uninit` / `create_device_local_buffers_batched`, plus
  `StagingPool`/`StagingGuard`); the image side never got the analogue.
- **Evidence**:
  - `taa.rs::create_history_image` and `svgf.rs::create_history_image` are
    line-for-line the same ~85-line body, differing only in `HISTORY_FORMAT`
    (a const) vs `format` (a parameter) — including the SAFETY comments,
    the `with_context(|| format!("create {name}"))` message shapes, and the
    order of `free(alloc)` before `destroy_image(image)` in every error arm.
  - 14 files call `bind_image_memory`; 12 of them are this chain (`buffer.rs`
    and `texture.rs` are the two legitimate specialisations).
  - The same defect class has now been fixed **four separate times, once per
    copy**, which is the cost this finding is really about:
    - #1163 — allocator `MutexGuard` held across an error arm that re-locks (`ssao.rs`, fixed in place);
    - #1164 — push-the-allocation-before-the-bind so the partial-state invariant is structural (`ssao.rs`);
    - #1165 — "deadlock identical to #1163" (`context/helpers.rs`);
    - #2178 / PERF-D3-03 — sub-allocation stranded on bind failure (`gbuffer.rs`), whose own comment reads *"Same shape as the sibling site in `frame_upscaler.rs::create_outputs` and the established pattern in `exposure.rs`"* — i.e. the author had to hand-check three copies.
    `svgf.rs` and `caustic.rs` each carry a comment saying *"Cf. ssao.rs for the
    #1163 separate-let pattern"* — twelve copies means twelve independent
    re-derivations of a lock-reentrancy rule.
- **Impact**: Every new render pass pays ~80 lines of ceremony and re-derives
  the cleanup ordering and the allocator-lock scope from scratch; every future
  fix to that ordering has twelve landing sites and no compiler help finding
  them. This is a Vulkan *resource-lifecycle* consolidation, not a
  render-pass/barrier one, so it is outside the
  `feedback_speculative_vulkan_fixes.md` caution — the behaviour is
  observable from `cargo test` via the existing `#[cfg(test)]` source-shape
  guards in `frame_upscaler.rs`.
- **Related**: #1163, #1164, #1165, #2178 (all CLOSED, all the same defect in
  different copies); `crates/renderer/src/vulkan/buffer.rs` (`GpuBuffer`) as the
  precedent.
- **Suggested Fix**: Add `GpuImage` next to `GpuBuffer` — either in
  `crates/renderer/src/vulkan/buffer.rs` (renaming it to a resources module) or
  a new `crates/renderer/src/vulkan/image.rs` — exposing
  `GpuImage::create_2d(device, allocator, name, extent, format, usage) -> Result<GpuImage>`
  with `image`/`view`/`allocation` fields, a `destroy(device, allocator)`, and
  the same `Drop` safety-net `GpuBuffer` has (#656). Migrate the twelve sites
  one file per commit; `gbuffer.rs` (`COLOR_ATTACHMENT | SAMPLED`) and
  `volumetrics.rs` (3D) need a `usage`/`image_type` parameter, not a second
  helper.
- **Effort**: medium (≤1 day) — decompose one file per commit

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
