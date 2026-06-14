# Renderer Audit — 2026-06-14 (Dimension 11 only: TAA / M37.5)

**Scope**: single-dimension run via `/audit-renderer 11` — **deep** delta re-audit of TAA (Halton(2,3) jitter + YCoCg variance-clamp resolve, M37.5) against the 2026-05-26 DIM11 baseline. The other 22 renderer dimensions were not run today; see the 2026-06-11 sweep (`AUDIT_RENDERER_2026-06-11.md`) for the most recent broader-pipeline state. This run targeted the heavy TAA-path churn since the last DIM11 sweep.

## Executive Summary

**Zero correctness findings (no CRITICAL / HIGH / MEDIUM).** Every high-risk change introduced in the TAA path since 2026-05-26 is implemented correctly and test-covered:

- **Origin-corrected `prev_view_proj`** (#1489, the single highest-risk change — a mismatch is a full-screen 1-frame smear on every 4096-grid crossing) is mathematically correct (`M = prev_vp · translation(O₂−O₁)`) and pinned by a Markarth-scale unit test.
- **Stochastic DoF-via-TAA** (`400fa68f`) adds **zero** changes to `taa.comp` and **zero** new fields to the TAA params UBO — it is pure CPU-side aperture-disk camera-position jitter (Halton(5,7), coprime to TAA's 2/3) routed through `GpuCamera.dof_params`, accumulated by the existing resolve. `GpuCamera` (which carries `prev_view_proj`) grew 304→320→336 B and is pinned by `gpu_camera_is_336_bytes` **and** the cross-shader SPIR-V reflection guard, so the TAA reprojection input cannot silently drift.
- **Parked-camera α** (`1/(static_frames+1)`, capped ~1/256) and the **moving-pixel α-floor at 0.1** (#1497) are correctly gated on *per-pixel* stillness (`pixelStatic = cameraStatic && dot(motion,motion) < 1e-8`, #1479) — a walking actor under a parked camera does not soft-blur.
- **TAA/bloom UBO fold** (#1397) into the pre-render-pass bulk `HOST→…|COMPUTE_SHADER` barrier is correct; the execution dependency transitively covers the post-render-pass TAA dispatch.
- Prior **F-11.01** (no explicit history-validity flag on swapchain recreate) is **re-verified STALE-ON-PREMISE, NOT regressed**.

Output: 2 **LOW** (one dormant DoF degenerate-input guard gap, one GpuCamera doc-rot) + 2 **INFO** interaction notes on the dormant DoF feature.

| Severity | NEW | CARRYOVER | FIXED-VERIFIED | STALE-ON-PREMISE | INFO |
|----------|-----|-----------|----------------|------------------|------|
| Critical | 0   | 0         | 0              | 0                | 0    |
| High     | 0   | 0         | 0              | 0                | 0    |
| Medium   | 0   | 0         | 0              | 0                | 0    |
| Low      | 2   | 0         | 0              | 1 (prior F-11.01)| 2    |
| **Total**| **2** | **0**   | **0**          | **1**            | **2**|

## RT Pipeline Assessment

Not in scope for Dim 11. TAA consumes the post-geometry rasterized HDR + motion + mesh-id and is downstream of all RT consumers (shadow / reflection / GI ray queries). No interaction with BLAS/TLAS lifecycle.

## Rasterization Assessment (TAA delta)

- **Jitter generators coprime by design**: TAA Halton(2,3) at `context/draw.rs:540-558` (16-frame period, ÷render-extent → NDC pixel units); DoF Halton(5,7) at `draw.rs:590-591` (32-frame period). Bases 5/7 coprime to 2/3 avoid correlated low-discrepancy gaps.
- **Un-jittered VP drives reprojection**: `camera_static` and `prev_view_proj` use the jitter-free `view_proj` (TAA sub-pixel jitter is injected later in the vertex shader). Under DoF the lens-jittered VP becomes `effective_vp` (the intended bokeh motion source), but `prev_view_proj` remains un-jittered + origin-corrected.
- **History stays in `GENERAL`** across the frame (`taa.rs:730-759`); the only per-frame barrier is a GENERAL→GENERAL WAR guard (`COMPUTE→COMPUTE`), with the over-specified `FRAGMENT_SHADER` src stage correctly dropped (per-FIF fence covers it, REN-D11-NEW-05).
- **Composite source selection**: `composite.rebind_hdr_views` rewires composite binding 0 to the TAA output when TAA is present; on dispatch failure `composite.fall_back_to_raw_hdr` rebinds to raw HDR (#479) so the screen keeps updating. Bloom intentionally reads the raw *pre-TAA* HDR (`draw.rs:3132-3160`).

## Checklist Status

| # | Item | Status | Evidence |
|---|------|--------|----------|
| 1 | Halton(2,3) advance + NDC pixel-unit jitter | **PASS** | `context/draw.rs:540-558`; DoF Halton(5,7) coprime `:590-591` |
| 2 | Camera UBO un-jittered + origin-corrected prev projection | **PASS** | `draw.rs:629-633`; `camera_static`/motion use jitter-free vp `:667-675` |
| 3 | Per-FIF history slots, no aliasing/WAR | **PASS** | `taa.rs` `history[frame]`, ping-pong `prev=(f+1)%FIF` `:539`, FIF≥2 static-assert `:52` |
| 4 | Motion-vector reprojection (dilation, not point) | **PASS** | `taa.comp:115-129` 5-tap MV-max + Catmull-Rom 9-tap history `:50-74` |
| 5 | YCoCg 3×3 neighborhood clamp | **PASS** | `taa.comp:170-200` (γ=1.5); chroma-only clamp for `pixelStatic` `:239-251` |
| 6 | Mesh-ID disocclusion, bit-31 masked | **PASS** | `taa.comp:137-147` (`& 0x7FFFFFFF`); alpha-blend bit-31 path `:156-168` |
| 7 | First-frame / force-reset → α=1.0, no garbage read | **PASS** | `taa.comp:96-99`; `taa.rs:666`; `mark_frame_completed` submit-success-gated `:811-814` |
| 8 | History layout GENERAL, no UNDEFINED/frame | **PASS** | `taa.rs:730-759` GENERAL→GENERAL WAR barrier only |
| 9 | Descriptor bindings (7) match docstring layout | **PASS** | `taa.comp:21-31`; `taa.rs` layout |
| 10 | SPIR-V reflection (`validate_set_layout`) fires for taa.comp | **PASS** | `taa.rs:322-332` `.expect()` hard-fail on drift (#427) |
| 11 | Composite samples TAA output when on | **PASS** | `composite.rebind_hdr_views` (`context/mod.rs:1715-1717`); `draw.rs:3140-3147` |
| 12 | TAA off → composite raw HDR, dispatch skipped | **PASS** | `Option<Taa>` guard `draw.rs:3078-3097`; #479 fallback |
| 13 | Parked 1/N vs exponential; #1497 floor gated on per-pixel motion (#1479) | **PASS** | `taa.comp:235,266`; `taa.rs:677-688` |
| 14 | DoF aperture jitter doesn't corrupt clamp/MV/struct; bounded | **PASS** (+INFO) | no `taa.comp` change; bounded disk sample; F-11.D1, INFO-11.A/B |
| 15 | Origin-corrected prev_view_proj across snaps | **PASS** | `draw.rs:3470-3484` + Markarth test `:3514-3530` |
| 16 | TAA/bloom UBO fold into pre-render-pass bulk barrier | **PASS** | upload `draw.rs:2216`; bulk barrier `:2243-2257` (before render pass `:2276`) |
| 17 | repr(C) TAA params + GpuCamera vs shader, no drift | **PASS** (+LOW) | `TaaParams` 32 B unchanged (`taa.rs:63-72` ↔ `taa.comp:28-31`); `GpuCamera` 336 B pinned + SPIR-V guard; doc-rot F-11.D2 |
| 18 | No per-game branching in TAA path | **PASS** | game-agnostic; no `GameKind` in `taa.comp`/`taa.rs` |

## Findings

### F-11.D1: DoF look-at degenerates when `focus_dist → 0` (dormant)

- **Severity**: LOW
- **Dimension**: TAA
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:589-615`
- **Status**: NEW
- **Description**: Under DoF (`dof.aperture > 0.0`) the jittered view is `look_at_rh(jittered_eye − origin, focal_pt − origin, up)` with `focal_pt = pos + dof.focus_dist · fwd` and `jittered_eye = pos + lens_u·right + lens_v·up`. As `focus_dist → 0`, the eye→center vector collapses to `focus_dist·fwd − lens`, losing its forward component and pointing along the (perpendicular) lens offset — a degenerate/sideways view basis; if the disk sample is also ~0 (eye≈center) `look_at_rh` normalizes a near-zero direction → NaN, which TAA would propagate through the history blend.
- **Evidence**: `focal_pt = pos + dof.focus_dist * fwd` (`:603`); `look_at_rh(jittered_eye - render_origin, focal_pt - render_origin, up)` (`:608-612`). No `focus_dist > ε` guard.
- **Impact**: None today — **dormant**. `DofView::default()` ships `aperture = 0.0` (`context/mod.rs:754`) so the branch is skipped, and there is no console / runtime mutation path that sets `aperture` or `focus_dist`. The hazard only materializes once a DoF console command is wired (the obvious next feature step) and a user sets `focus_dist = 0`.
- **Suggested Fix**: Clamp `focus_dist.max(0.01)` at the `effective_vp` build site, or guard the DoF branch on `dof.focus_dist > ε`. ~2 LOC, alongside the eventual console wiring.

### F-11.D2: GpuCamera doc-comment names a stale size + non-existent pin test

- **Severity**: LOW
- **Dimension**: TAA (struct drift / doc-rot)
- **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:171-176`
- **Status**: NEW
- **Description**: The `GpuCamera` doc header says "**320 bytes**" pinned by `gpu_camera_is_320_bytes`. The struct is actually **336 B** (320 + the `render_origin` vec4 from #markarth-precision / #1492) and the live pin is `gpu_camera_is_336_bytes`. Both the byte count and the test name are stale; the #1484 doc-rot pass (`6400e78b`) missed it.
- **Evidence**: stale doc `gpu_types.rs:173`; live `fn gpu_camera_is_336_bytes()` asserting `size_of::<GpuCamera>() == 336` at `scene_buffer/gpu_instance_layout_tests.rs:56-60`; the cross-shader SPIR-V guard `camera_ubo_size_matches_gpu_camera_in_every_shader` (`reflect.rs:433-435`) uses `size_of::<GpuCamera>()` dynamically (auto-tracks).
- **Impact**: Doc-only. **Correctness of `prev_view_proj` (TAA's reprojection input) is fully guarded** by the 336 B size test + the SPIR-V block-size reflection guard. The only cost is a maintainer following the comment to a test that does not exist.
- **Suggested Fix**: Update `gpu_types.rs:171,173` to "336 bytes" / `gpu_camera_is_336_bytes`. ~2-line doc edit.

### INFO-11.A: DoF suppresses parked-camera 1/N convergence

- **Severity**: LOW (INFO — interaction note)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:589-618`, `:666-675`
- **Status**: INFO
- **Note**: `camera_static` is computed from `vp = effective_vp`. Under DoF the lens-jittered VP changes every frame, so `camera_static` is always `false` while `aperture > 0.0`. The progressive 1/N accumulation (`taa.rs:677-688`) **and** the direct/GI Monte-Carlo convergence riding on `dof_params.w`/`camera_static` in `triangle.frag` are therefore suppressed whenever the lens jitters. Arguably correct (a jittering aperture is not a static camera; the off-focus channel re-converges via the 0.1 moving-pixel α blend), but "enable DoF" silently disables the parked-camera convergence optimization. Dormant today. Flag for the sweep that activates DoF.

### INFO-11.B: DoF bokeh radius is bounded by the YCoCg variance clamp

- **Severity**: LOW (INFO — design limitation)
- **Location**: `crates/renderer/shaders/taa.comp:170-251`
- **Status**: INFO
- **Note**: Off-focus surfaces produce bokeh by accumulating frame-to-frame aperture parallax through TAA history. For parallax > ~1 px, the reprojected history samples a different-colored region that the 3×3 YCoCg variance clamp (γ=1.5) partially rejects, pulling defocused history back toward the current neighborhood — so the achievable bokeh radius is bounded by the clamp window and strong-defocus bokeh will be under-blurred vs a true thin-lens. Inherent trade-off of the zero-extra-pass "DoF through TAA" approach, not a correctness bug; visual-quality only and dormant. No action unless DoF is activated and bokeh is judged insufficient.

### Prior F-11.01 (history-validity flag) — re-verified STALE-ON-PREMISE, NOT regressed

- **Severity**: MEDIUM (as originally proposed) → STALE
- **Location**: `crates/renderer/src/vulkan/taa.rs`, `context/resize.rs`
- **Status**: STALE-ON-PREMISE (carried from 2026-05-26, re-confirmed not regressed)
- **Note**: The requested mechanism is intact: `signal_history_reset` (`taa.rs:608`), `recreate_on_resize` (`taa.rs:826`, called from `resize.rs:603`, resets counter `:860`, walks slots to GENERAL), `should_force_history_reset(c) := c < MAX_FRAMES_IN_FLIGHT` (`taa.rs:110`/`:666`), and `mark_frame_completed` (`taa.rs:811`, advances the counter **only on submit success** so a record-failure frame doesn't falsely consume the reset window; called `draw.rs:3360`). Not filed.

## Prioritized Fix Order

1. **F-11.D2** (LOW, ~2 lines) — fix the GpuCamera doc-rot now; trivial and keeps the struct-pin documentation honest.
2. **F-11.D1** (LOW, ~2 lines) — fold the `focus_dist` floor into whatever commit wires a DoF console command; harmless to land earlier as defense-in-depth.
3. **INFO-11.A / INFO-11.B** — no action until DoF is activated; revisit in the DoF-activation sweep.

## Cross-cutting notes

- **Shader Struct Sync** (`feedback_shader_struct_sync.md`): `taa.comp` redeclares no shared `Gpu*` struct; the `GpuInstance` 5-shader lockstep contract does not gate TAA. `TaaParams` (32 B, 2×vec4) is private and unchanged by the DoF work. The shared struct in the reprojection path is `GpuCamera` (carries `prev_view_proj`), pinned by `gpu_camera_is_336_bytes` + the cross-shader SPIR-V reflection guard.
- **Format Translation Layer** (`feedback_format_translation.md`): no interaction — TAA is downstream of all material/shader translation and fully game-agnostic.
- **VRAM** (`feedback_vram_baseline.md`): the DoF feature adds no new TAA-side images (CPU-side camera jitter only). History color + depth per FIF slot remains well within the 4 GB renderer budget.

## Methodology

1. Read the 2026-05-26 DIM11 baseline for delta tracking.
2. `git log --since=2026-05-26` over the TAA path → ~12 in-scope commits; prioritized `400fa68f` (DoF), `c6342845`/#1497, `2f7bcf78`/#1479, `6ada7a57`/#1397, and the camera-relative origin cascade (`bccf06f0`/`36f66493`/`6c844744`/`ba8e52d3`/`e4408042`).
3. Walked: `taa.rs` (params, history ring, dispatch, reset mechanism), `taa.comp` (full 276-line resolve), `context/draw.rs` (jitter + DoF build + `origin_corrected_prev_view_proj` + bulk barrier + dispatch ordering + composite/bloom integration), `scene_buffer/gpu_types.rs` (`GpuCamera`/`DofView`), `reflect.rs` (size + set-layout guards), `context/resize.rs` (recreate wiring).
4. Each finding re-checked against current source and disproof-attempted; both NEW findings reduced to dormant/doc-rot under scrutiny — no live correctness bug survived.
5. Dedup: `/tmp/audit/renderer/issues.json` (23 open) — no TAA/temporal/jitter/halton/history/reproject match.

---

Suggest: `/audit-publish docs/audits/AUDIT_RENDERER_2026-06-14_DIM11.md`
