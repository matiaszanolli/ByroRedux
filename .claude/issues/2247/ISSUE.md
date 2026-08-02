# REN-D20-01: A skipped debug-UI frame permanently drops that frame's egui texture delta

Severity: medium
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2247

**Dimension**: 20 (Debug UI)
**Location**: `crates/renderer/src/vulkan/context/mod.rs:3251` (`submit_egui_frame` — `self.egui_pending_output = Some((ctx, output))`, a plain overwrite); `crates/renderer/src/vulkan/context/draw.rs:2726` (`self.egui_pending_output.take()`, the sole consumption site)
**Status**: NEW

**Description**: `submit_egui_frame` stores the latest `FullOutput` by overwriting `egui_pending_output`, not accumulating. If `draw_frame` doesn't reach the consumption point on some iteration (e.g. an earlier error/early-return in that frame's `draw_frame` call, or any iteration where the egui block is skipped for another reason) before the next `submit_egui_frame` call, the dropped frame's `textures_delta` — both new texture uploads (`.set`) and pending frees (`.free`) — is silently lost forever, since nothing merges deltas across skipped frames.

**Impact**: A dropped frame's new texture upload (e.g. a newly-shown icon or an updated font atlas) never reaches the GPU, permanently darkening/blanking that part of the debug overlay; a dropped frame's texture frees never run, leaking that frame's now-orphaned egui textures for the rest of the session.

**Suggested Fix**: accumulate (merge) `textures_delta.set`/`textures_delta.free` across `submit_egui_frame` calls instead of overwriting, so a skipped frame's delta rolls into whichever frame's consumption actually happens next.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
