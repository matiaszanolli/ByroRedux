## Finding REN2-06 — Renderer Audit 2026-06-11

- **Severity**: MEDIUM (error-path Vulkan validity; would be a spec violation when triggered)
- **Dimension**: Debug Overlay & GPU Telemetry
- **Location**: `crates/renderer/src/vulkan/egui_pass.rs:194-199` (begin at `:194`, fallible `cmd_draw(...).map_err(...)?` at `:196-198`, end at `:199`); caller `crates/renderer/src/vulkan/context/draw.rs:3196-3218`
- **Status**: NEW. Validated CONFIRMED at HEAD `1e8a25ab`.

## Description

`renderer.cmd_draw(...)` is fallible and its `map_err(...)?` sits between `cmd_begin_render_pass` and `cmd_end_render_pass`. On Err, dispatch returns with the render pass open. The caller (`draw.rs:3196-3208`) logs the error and keeps recording: pending screenshot `vkCmdCopyImage` inside an active render pass (`:3216`), then `end_command_buffer` (`:3218`) → VUID-vkEndCommandBuffer-commandBuffer-00060 — and submits the invalid buffer. (The `set_textures`/`free_textures` `?`-bails at `:163`/`:173` happen before the RP begins, so only the `cmd_draw` site is inside the pass.)

## Impact

Requires an egui-ash-renderer internal allocation failure first (e.g. VRAM pressure), but then a frame that should degrade gracefully becomes validation errors / UB in release.

## Suggested Fix

Capture the `cmd_draw` Result, call `cmd_end_render_pass` unconditionally after the begin, then propagate. Pure CPU-side recording-balance fix — code-inspectable, no RenderDoc needed.

## Related

#1427, #1433 (open, distinct egui issues — batchable together).

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files — other fallible calls inside begun render passes / command-buffer scopes
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer
- [ ] **TESTS**: Regression test added for this specific fix

---
Source: `docs/audits/AUDIT_RENDERER_2026-06-11.md` · Filed by `/audit-publish`
