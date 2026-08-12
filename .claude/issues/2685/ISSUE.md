# SAFE-D10-01: EguiPass VkRenderPass leaks on the recreate_framebuffers error path

**Issue**: #2685
**Filed**: 2026-08-12 via `/audit-publish` from `/audit-suite renderer-deep`

- **Severity**: MEDIUM
- **Dimension**: 10 (egui overlay teardown)
- **Location**: [resize.rs](crates/renderer/src/vulkan/context/resize.rs) —
  `VulkanContext::recreate_swapchain_core`, the `if let Some(mut pass) = self.egui_pass.take()`
  block; [egui_pass.rs](crates/renderer/src/vulkan/egui_pass.rs) — `EguiPass` (no `Drop` impl),
  `EguiPass::destroy`, `EguiPass::recreate_framebuffers`
- **Status**: NEW
- **Description**: The resize path `take()`s `EguiPass` out of `self.egui_pass`,
  then calls `pass.recreate_framebuffers(...)?`. `EguiPass` has **no `Drop`
  impl** (grep `impl Drop for EguiPass` → no match); all of its device-owned
  state (`render_pass`, `framebuffers`) is freed only by the explicit
  `EguiPass::destroy`. On the `?` the taken `pass` is dropped without
  `destroy()`, so its `vk::RenderPass` — and any framebuffers
  `create_framebuffers` had already created before erroring — are never
  destroyed. The old framebuffers *are* safe (drained + destroyed at the top of
  `recreate_framebuffers`), so the leak is exactly one `VkRenderPass` plus the
  partial framebuffer set per failed resize, held until process exit; the
  validation layer will report live objects at `vkDestroyDevice`. As a
  secondary effect `self.egui_pass` stays `None`, silently disabling the
  overlay for the rest of the session.
- **Evidence**: `resize.rs` (format-stable arm):
  ```rust
  if let Some(mut pass) = self.egui_pass.take() {
      if pass.format() == self.swapchain_state.format.format {
          pass.recreate_framebuffers(              // <-- `?` here drops `pass`
              &self.device,                        //     without destroy()
              &self.swapchain_state.image_views,
              self.swapchain_state.extent,
          )?;
          self.egui_pass = Some(pass);
  ```
  Every sibling `take()` in the same function (`self.water`, `self.volumetrics`,
  `self.presentation`, `self.taa`) destroys the taken value **immediately**,
  with no fallible call in between — egui is the only asymmetric site.
  The format-CHANGE arm of the same block is correct (`pass.destroy(&self.device)`
  before rebuild), which is why #2475's fix did not cover this.
- **Impact**: Leak is per failed swapchain recreate (framebuffer creation OOM),
  not per frame — bounded and rare, hence MEDIUM rather than the HIGH that
  `_audit-severity`'s "missing cleanup on swapchain recreate" row would imply
  for the happy path. Blast radius: one render pass + N framebuffers, plus a
  permanently disabled debug overlay.
- **Related**: #2475 (CLOSED, format-change rebuild — the arm that *is* correct);
  Dim 3's allocator-before-device class.
- **Suggested Fix**: Capture the result instead of `?`-ing it —
  `let r = pass.recreate_framebuffers(...); if r.is_err() { pass.destroy(&self.device); } self.egui_pass = r.map(|_| pass).ok(); r?;`
  — or give `EguiPass` a `Drop` that calls `destroy` idempotently (null the
  handles in `destroy` so double-destroy is a no-op).

---


---
*Filed from [`docs/audits/AUDIT_SAFETY_2026-08-12.md`](docs/audits/AUDIT_SAFETY_2026-08-12.md) — `/audit-suite renderer-deep`, 2026-08-12. Finding ID `SAFE-D10-01`.*

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other pipelines, other AS paths)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
