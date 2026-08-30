# #3628 — REN-2026-08-30-D20-03: the new depth-capture path's two ordering invariants are held by comments only, with no source-scan guard

**Labels**: `low,renderer,test-gap,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3628 --json state`.

---

- **Severity**: Low
- **Dimension**: Debug/Telemetry
- **Location**: `crates/renderer/src/vulkan/context/depth_capture.rs`, `crates/renderer/src/vulkan/context/draw.rs:1708` + `:3684`
- **Status**: Open (missing regression guard)
- **Description**: Both fence and layout invariants verify **clean today**, but neither is pinned. (a) `depth_capture_finish_readback()` at `draw.rs:1708` sits after the `wait_for_fences(&[in_flight[frame], in_flight[prev]], …)` at `draw.rs:1624-1636`, which waits on *both* FIF fences (`MAX_FRAMES_IN_FLIGHT == 2`), so the previous frame's copy is genuinely retired — the same discipline `screenshot_finish_readback` has, one line above. (b) `depth_capture_record_copy(cmd)` at `draw.rs:3684` sits immediately after `copy_depth_to_history(cmd)`, which is what makes its documented `DEPTH_STENCIL_READ_ONLY_OPTIMAL` precondition true. Move either call and the failure is silent-to-`cargo test`: a stale/garbage readback in case (a), a validation-layer layout error or corrupt samples in case (b).
- **Evidence**:
  - `grep -rn "depth_capture" --include="*.rs" crates byroredux | grep -i test` returns exactly one hit: the `unsafe fn` safety-doc scanner added to `frame_upscaler.rs:1328,1368` under `#3308`. There is no test that pins where either call site sits.
  - No `depth.stats` test in `byroredux/src/commands_tests.rs` (45 test fns, none reference `DepthStats` or `DepthCapture`). `analyze_depth_field` itself *is* unit-tested (`camera.rs:549,579,605`) — only the plumbing is untested.
  - The repo already uses source-scan guards for exactly this class of cross-file invariant: `egui_pass.rs::dependency_chain_tests`, `resize.rs::egui_pass_rebuilds_fully_on_swapchain_format_change`, `resize.rs::egui_framebuffer_recreate_failure_destroys_the_taken_pass`, `post_passes.rs`'s `record_post_passes_has_no_error_propagation_after_the_svgf_latch`, and `frame_upscaler.rs`'s own safety-doc scanner.
- **Impact**: A future refactor of `draw_frame`'s tail (the region has been restructured three times: `#1748`, `#2258`, `#3426`) can move either call with no test signal. The capture exists to be trusted as ground truth against `depth_resolution_at`; a silently-stale one is worse than no capture.
- **Suggested Fix**: Add a `#[cfg(test)]` source scan over `include_str!("draw.rs")` asserting (a) the byte offset of `depth_capture_finish_readback()` is after the `wait_for_fences` call, and (b) `self.depth_capture_record_copy(cmd)` follows `self.copy_depth_to_history(cmd)` with no other `self.` statement between them. Same shape as the existing `egui_pass` / `post_passes` scanners.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D20-03

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
