# #3632 — REN-2026-08-30-D23-02: `is_fsr_dispatch_active()` promises "actually dispatching this frame", but `force_native_debug` blits while it still returns true

**Labels**: `low,renderer,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3632 --json state`.

---

- **Severity**: LOW
- **Dimension**: FSR/Presentation
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` (`VulkanContext::is_fsr_dispatch_active`, ~line 1516) vs `crates/renderer/src/vulkan/context/post_passes.rs` (`record_upscale_pass`, `force_native_debug`, line 994)
- **Status**: NEW
- **Description**: `is_fsr_dispatch_active()` is the single accessor #2518 introduced so
  that "is FSR's projection jitter in play?" has exactly one answer. Its doc states the
  cases it covers "fall back to an **unjittered** native blit". `record_upscale_pass` adds a
  third suspension case the accessor does not know about: when
  `render_debug_requires_raw_output(self.render_debug_flags, self.render_debug_mode.shader_value())`
  is true it passes `force_native_blit: true` into `FrameUpscaler::record`, which takes the
  bridge branch and returns without dispatching — while `is_fsr_dispatch_active()` stays
  `true`, so the jitter gate at `draw.rs:2039-2066` still applies the FSR sub-pixel offset
  to the projection. A raw-output debug view is therefore rendered *jittered but never
  reconstructed*, then `LINEAR`-blitted render→output. This is structurally the same
  condition #2519 identified for the dispatch-failure path and closed there with
  `new_dispatch_failure` → `signal_temporal_discontinuity`.
- **Evidence**:
  - `post_passes.rs:994-1016` computes `force_native_debug` locally and passes it only into
    `record`; it is never read at the jitter site. `grep -rn render_debug_requires_raw_output`
    returns exactly one production call site.
  - `frame_upscaler.rs:445-460` — `if force_native_blit || !self.is_fsr_dispatch_active()`
    → `record_native_blit(..., SHADER_READ_ONLY_OPTIMAL)` → `return`, with
    `dispatched_this_frame = false` and no `dispatch_failure` latch.
  - Because `dispatched_this_frame` stays false, `draw.rs:3900-3908` never calls
    `FsrTemporalState::mark_dispatch_completed`, so the jitter index freezes AND
    `reset_pending` keeps its last value (`false` after any prior successful dispatch). The
    first frame back at `RENDER_DEBUG_FINAL` therefore dispatches with `reset: false`
    against reconstruction history that is stale by the length of the debug session.
  - `crates/renderer/src/vulkan/context/render_debug.rs:9-14` — `set_render_debug_mode`
    only logs and assigns; it does not call `signal_temporal_discontinuity`, which is what
    every other history-invalidating transition in the renderer does
    (`context/mod.rs:2039`).
- **Impact**: Debug-tooling only, never on a shipping frame. Raw debug views carry a fixed
  sub-pixel offset and one to two frames of stale-history reconstruction appear on the way
  back to `Final`. It is filed because the accessor's *documented contract* is now false at
  one of its call sites, and that contract is what keeps the jitter, the DOF gate and the
  `DBG_VIZ_FSR_TEMPORAL` view from drifting apart again.
- **Needs RenderDoc**: no — entirely source-provable.
- **Suggested Fix**: Either fold the raw-output predicate into `is_fsr_dispatch_active()`
  (it already has `self.render_debug_flags` / `render_debug_mode` in scope), so the frame is
  unjittered like every other non-dispatching frame; or, if a jittered raw view is wanted,
  say so in the accessor's doc and have `set_render_debug_mode` call
  `signal_temporal_discontinuity` on any transition that crosses the
  `render_debug_requires_raw_output` boundary.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D23-02

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
