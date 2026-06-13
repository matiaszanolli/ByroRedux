## Finding REN2-04 — Renderer Audit 2026-06-11

- **Severity**: MEDIUM (borderline HIGH under "wrong SVGF motion vectors"; rated MEDIUM because the vectors are wrong only on the discrete jump frame and no persistent state corrupts)
- **Dimension**: cross-cutting (Dims 1, 4, 5, 6, 11; canonical write-up Dim 4)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:617,667-672,826` (`prev_view_proj` stored without its origin; `render_origin` recomputed per-frame at `:582-585`); `crates/renderer/shaders/triangle.vert:232`; `byroredux/src/render/camera.rs:93-97,156`
- **Status**: NEW — introduced by `36f66493`. Validated CONFIRMED at HEAD `1e8a25ab` (grep confirms no `prev_render_origin` exists anywhere).

## Description

`prev_view_proj` is last frame's relative matrix (origin O₁); this frame's positions are rebased by the current origin O₂. On any frame the camera crosses a 4096-unit grid line, `fragPrevClipPos = prevViewProj * (P_abs − O₂)` (`triangle.vert:232`) is off by ΔO (up to 4096 u/axis) → the motion-vector attachment is garbage for the entire screen for one frame.

The mitigating comment at `camera.rs:93-97` ("the origin only moves when … streaming already resets temporal continuity") is factually wrong: streaming never resets TAA/SVGF history — `should_force_history_reset` (`crates/renderer/src/vulkan/svgf.rs:137`) is purely `frames_since_creation < MAX_FRAMES_IN_FLIGHT`, firing only on resource recreation/resize — and the raw `floor()` snap (`camera.rs:156`) has no hysteresis, so oscillating near a grid line re-triggers every crossing.

## Impact

One-frame full-screen TAA aliasing flash + SVGF indirect-noise burst (graceful full-frame history drop via the prevUV bounds check, then ~10–20-frame re-convergence) on every grid crossing during exterior traversal. Recurrent in normal gameplay; worst when strafing along a boundary. Skinned MVs are additionally subsumed by REN2-01.

## Suggested Fix

Track `prev_render_origin` alongside `prev_view_proj` and upload the origin-corrected matrix `prev_vp · translation(O₂ − O₁)` (exact, keeps MVs valid across crossings). Cheaper fallback: force the SVGF/TAA history-reset on jump frames so the drop is at least intentional. Fix the `camera.rs:93-97` comment either way.

## Related

REN2-01, REN2-05, REN2-07 (same cascade fix branch).

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files — any other consumer of `prev_view_proj` (TAA reprojection, SVGF temporal)
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer
- [ ] **TESTS**: Regression test added for this specific fix

---
Source: `docs/audits/AUDIT_RENDERER_2026-06-11.md` · Filed by `/audit-publish`
