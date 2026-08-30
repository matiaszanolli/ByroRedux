# #3636 — REN-2026-08-30-D23-06: no regression guard pins the resize ordering that keeps the presentation descriptor off a destroyed upscale view

**Labels**: `low,renderer,test-gap,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3636 --json state`.

---

- **Severity**: LOW
- **Dimension**: FSR/Presentation
- **Location**: `crates/renderer/src/vulkan/context/resize.rs:1005-1051` (`recreate_swapchain_core`), test module at `resize.rs:1394+`
- **Status**: NEW
- **Description**: `FrameUpscaler::recreate` is an unconditional `destroy` + `Self::new`
  (`frame_upscaler.rs:1005-1024`), so every resize and every preset switch replaces the
  output `VkImage`/`VkImageView` handles. `PresentationPipeline` writes those views into its
  descriptor sets exactly once, in `create`/`write_inputs`. The only thing preventing the
  presentation descriptor from naming a destroyed view is the source ordering in
  `recreate_swapchain_core`: `presentation.take()` → `destroy` → `upscaler.recreate()` →
  `PresentationPipeline::new(..., &upscaled_views, ...)`. That ordering is load-bearing, is
  the highest-value source-provable invariant in this dimension, and has no test.
  `resize.rs`'s test module already uses exactly this static-source-landmark technique for
  three sibling invariants (#654, #2141, #2142, #2156), so the mechanism is established.
- **Evidence**:
  - `resize.rs:1005-1012` — the comment states the invariant ("Presentation descriptors
    reference the upscaler's output views, so retire presentation before replacing those
    views") but nothing asserts it.
  - `frame_upscaler.rs:1013-1024` — `unsafe { self.destroy(device, allocator) }; *self = Self::new(...)?;`
    → new image/view handles on every call, including a same-output-extent preset switch
    (Quality→Performance changes only `render`, but the outputs are recreated anyway).
  - `presentation.rs::write_inputs` is called only from `create`; there is no
    `rebind_upscaled_views` equivalent to `composite.rs::rebind_hdr_views`.
  - The existing four static-order tests in `resize.rs`'s `mod tests` cover the swapchain
    view handoff, the SSAO failure rebind, the water set-2 rebind and the upscaler-switch
    rollback — but none cover this pair.
- **Impact**: A reordering (e.g. hoisting `upscaler.recreate` above the `presentation.take()`
  so the `allocator` borrow reads more naturally) would leave every presentation descriptor
  sampling a destroyed image view on the first post-resize frame. Invisible to `cargo test`,
  and on the default render path.
- **Needs RenderDoc**: no
- **Suggested Fix**: Add a static-source test to `resize.rs::mod tests` in the style of
  `ssao_recreate_failure_rebinds_binding_7_to_the_placeholder`: assert
  `find("unsafe { presentation.destroy(&self.device) }")` < `find("upscaler.recreate(")` <
  `find("PresentationPipeline::new(")` inside `production_src()`, with a message naming the
  descriptor-vs-view lifetime as the reason.

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D23-06

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
