# Audit — Renderer, TAA Deep Pass (Dimension 11)

**Audit date**: 2026-05-08
**HEAD**: `53f4f64` (post Fix #901+#902 ROADMAP refresh; workspace tests 1814 / 1814 passing)
**Scope**: Dimension 11 only (Temporal Antialiasing, M37.5). Other 15 dimensions covered by `AUDIT_RENDERER_2026-05-07.md` and not re-run.
**Prior coverage**:
- 2026-05-07 full-16-dim audit declared dim 11 clean (`Halton + YCoCg + reset path intact; #801 STRM-N1 wired`).
- 2026-05-06 audit noted INFO `D5-OBS-01` (TAA ordering audit-spec mismatch) — already filed, not re-filed.
- Only TAA-territory commit since 2026-05-07 is `f9683ab` (#898 interior-fill docstring), which is `triangle.frag` not TAA — TAA proper is structurally unchanged since the clean declaration.

## Executive Summary

The 2026-05-07 "clean" claim **holds at depth**. Going deep on the path that the breadth-pass spent ~20 lines on confirms the architecture is correct end-to-end:

- `GpuCamera.view_proj` at [scene_buffer.rs:284-290](crates/renderer/src/vulkan/scene_buffer.rs#L284-L290) is the un-jittered base matrix; jitter is delivered as a separate `vec4` ([scene_buffer.rs:303-306](crates/renderer/src/vulkan/scene_buffer.rs#L303-L306)) and applied only to `gl_Position.xy` *after* `fragCurrClipPos` is captured at [triangle.vert:210](crates/renderer/shaders/triangle.vert#L210). Motion vectors are pure scene motion.
- Halton(2,3) cycles `1..=8` via `frame_counter % 8 + 1`, jitter range `[-0.5, 0.5)` pixel units via `(h - 0.5) * 2 / w`. Sequence has no zero-seam at index 0 (h2(1)=0.5 → x-jitter=0, h3(1)=0.333 → y-jitter≠0; subsequent indices distinct).
- `taa.comp` implements the textbook design: Catmull-Rom 9-tap history sample (Jimenez SIGGRAPH 2016) + Karis-style `mean ± 1.25σ` YCoCg variance clamp + luma-weighted blend.
- `frames_since_creation` reset on swapchain resize via `recreate_on_resize` ([taa.rs:705-727](crates/renderer/src/vulkan/taa.rs#L705-L727)) AND on cell-streaming events via `signal_history_reset()` → `signal_temporal_discontinuity(8)` from [main.rs:618](byroredux/src/main.rs#L618) and [main.rs:764](byroredux/src/main.rs#L764).

Two new LOW findings surfaced at depth — both bounded, both real, neither blocking.

| Severity | Count | Findings                                                                                              |
|----------|-------|-------------------------------------------------------------------------------------------------------|
| CRITICAL | 0     | —                                                                                                     |
| HIGH     | 0     | —                                                                                                     |
| MEDIUM   | 0     | —                                                                                                     |
| LOW      | 2     | NaN-propagation reliance on undefined GLSL `min`/`max` semantics; mesh_id full-u16 disocclusion drift |
| INFO     | 0     | —                                                                                                     |

## TAA Pipeline Assessment

- **Jitter assembly** (Rust): ✓ correct. `view_proj` un-jittered, `prev_view_proj` un-jittered, `inv_view_proj` from un-jittered, `jitter` separate vec4.
- **Jitter application** (vertex shader): ✓ correct. Capture `fragCurrClipPos = currClip` at line 210, then `currClip.xy += jitter.xy * currClip.w` at line 219 (jitter scaled by `clip.w` so the offset is constant in NDC after perspective divide). Jitter is `vec2(0)` no-op when TAA is off.
- **Motion-vector capture** (vertex shader): ✓ correct. `fragPrevClipPos = prevViewProj * prevWorldPos` at line 211 — `prevWorldPos` composed via `bones_prev` per #641, so skinned-mesh motion is per-vertex joint motion not just camera motion.
- **First-frame guard** (taa.comp:93-96): ✓ correct. `params.params.y > 0.5` → pass through current. No blend, no history read, no NaN risk on cold-start.
- **Offscreen guard** (taa.comp:103): ✓ correct. Out-of-range central UV → pass-through.
- **Disocclusion via mesh_id mismatch** (taa.comp:107-111): ⚠️ functionally correct but uses full u16 — see REN-D11-NEW-02.
- **Alpha-blend opt-out via bit 15** (taa.comp:120): ✓ correct. `(currMid & 0x8000u) != 0u` → forced pass-through for alpha-blended pixels. Catches z-fight flips that would amplify TAA jitter into cross-hatch moiré.
- **Catmull-Rom 9-tap history sample** (taa.comp:47-71): ✓ algorithm correct (Jimenez SIGGRAPH 2016, 9-tap variant for ~40% fewer fetches). `CLAMP_TO_EDGE` sampler at [taa.rs:176-178](crates/renderer/src/vulkan/taa.rs#L176-L178) handles edge-sampling.
- **YCoCg variance clamp** (taa.comp:127-148): ✓ correct. 3×3 moments + Karis `mean ± 1.25σ` clamp window. Conversion matrix is the standard `Y = R/4 + G/2 + B/4`, etc. Round-trip identity holds.
- **Luma-weighted blend** (taa.comp:158-164): ✓ correct. Reciprocal-luma weights with α gating reduce flicker on bright highlights. `max(..., 1e-6)` divisor prevents NaN from divide-by-zero.
- **HDR alpha preservation** (#676 / DEN-6): ✓ correct. `currA` captured at line 90 and re-written at all three `imageStore` sites.
- **History image format**: `rgba16f` storage image (binding 5, `writeonly`) + sampler read on binding 4. NaN-representable but no live NaN source.
- **Layout discipline**: history images held in `GENERAL`, no per-frame UNDEFINED transitions.
- **Cell-streaming reset wiring**: ✓ #801 intact. `step_streaming::unload` and `consume_streaming_payload::Ok(Some)` both call `ctx.signal_temporal_discontinuity(SVGF_TAA_STREAMING_RECOVERY_FRAMES)` which propagates to `taa.signal_history_reset()` at [context/mod.rs:1632](crates/renderer/src/vulkan/context/mod.rs#L1632).

## Findings

### REN-D11-NEW-01: NaN propagation through TAA history relies on undefined GLSL `min`/`max` semantics

**Severity**: LOW (fragile-but-works)
**Dimension**: 11 (TAA)
**Location**: [taa.comp:147-154](crates/renderer/shaders/taa.comp#L147-L154), [taa.comp:166](crates/renderer/shaders/taa.comp#L166)
**Status**: NEW · CONFIRMED at HEAD `53f4f64`

The temporal blend `outRgb = (currRgb·wCurr·α + clampedHist·wHist·(1-α)) / max(...)` has no explicit `isnan(histRgb)` guard. NaN can only enter via `sample_history_catmull_rom` reading a poisoned history pixel — first-frame guard at `taa.comp:93-96` prevents the bootstrap case, but if a single fragment in `uPrevHistory` ever holds NaN (e.g. from a future RT path that produces NaN on a degenerate ray query and writes it through `out_hdr`), the YCoCg `clamp(histYc, yMin, yMax)` at line 153 is the only thing standing between NaN and self-perpetuating history poison.

`clamp(x, mn, mx)` is `min(max(x, mn), mx)`. Per the GLSL spec, `min`/`max` propagation on NaN is **implementation-defined** — most desktop drivers (NVIDIA / AMD / Intel) return the non-NaN argument so the clamp acts as an implicit NaN filter, but a future driver or compiler emitting strict IEEE 754 propagation would break this invariant. Final `max(outRgb, vec3(0.0))` at line 166 has the same caveat.

**Impact**: dormant. No live NaN source today. If a future RT branch produces NaN, the visual symptom would be persistent black/garbage pixels in the TAA output until disocclusion (mesh_id mismatch) or the next swapchain resize / cell-streaming event clears them. Pure latent regression risk.

**Suggested fix** (sketch — does NOT touch Vulkan state, safe to ship):
```glsl
// Drop NaN/Inf from history before any blend
if (any(isnan(histRgb)) || any(isinf(histRgb))) {
    histRgb = currRgb;
}
```
Insert at `taa.comp:151` between the Catmull-Rom sample and the YCoCg clamp. Same pattern works for `outRgb` final write if defence-in-depth is desired.

**Dedup-check**: searched `nan` / `NaN` / `isnan` / `taa` / `history` against `/tmp/audit/renderer/issues.json` (200 issues). #801 covers cell-load reset, not NaN; no other open / closed issue matches.

---

### REN-D11-NEW-02: Disocclusion comparison uses full u16 mesh_id — bit-15 alpha-blend toggle force-resets history on opacity transitions

**Severity**: LOW (one-frame flicker on rare transitions)
**Dimension**: 11 (TAA)
**Location**: [taa.comp:107-120](crates/renderer/shaders/taa.comp#L107-L120)
**Status**: NEW · CONFIRMED at HEAD `53f4f64`

`disocclusion = (currMid != prevMid)` at line 111 compares the full u16. Per [helpers.rs:54-62](crates/renderer/src/vulkan/context/helpers.rs#L54-L62), bit 15 is reserved as `ALPHA_BLEND_NO_HISTORY`; the 15-bit instance ID lives in bits 0-14. When the *same instance* transitions from alpha-blended → opaque (or vice versa) between frames, the bit-15 flag flips even though the 15-bit instance ID is identical — `currMid != prevMid` returns true → history reset → one-frame visual flicker.

The dedicated `alphaBlend = (currMid & 0x8000u) != 0u` at line 120 already short-circuits to current-pass-through for *currently* alpha-blended pixels. So the bit-15-toggle drift only manifests in two narrow cases:

1. **Current frame opaque, previous frame alpha-blended on the same instance.** Disocclusion wins: history reset.
2. **Current frame alpha-blended, previous frame opaque.** `alphaBlend = true` wins first; disocclusion is moot.

Case 1 is the visible one — for one frame after an instance transitions out of alpha-blend, TAA falls back to current-only. The audit's checklist for Dim 11 explicitly flagged: *"Verify the comparison is a strict `==` on the 15-bit id, not the full u16."* Today's code does the full u16 compare.

**Impact**: dormant on shipped vanilla content (alpha-blend transitions are rare — flickering torches, fading dialogue overlays). When triggered, one frame of un-AA'd geometry. Below the visible floor under most circumstances.

**Suggested fix** (sketch):
```glsl
bool disocclusion = ((currMid & 0x7FFFu) != (prevMid & 0x7FFFu));
```
Same line (taa.comp:111). Mask to bits 0-14 so the disocclusion test sees only the instance ID. Bit-15 retains its meaning via the dedicated `alphaBlend` path.

**Dedup-check**: searched `mesh_id` / `disocclusion` / `bit 15` / `0x7fff` / `0x8000` against the issue queue. No matches. Distinct from #676 / DEN-6 (which was about HDR alpha at the SVGF temporal write, unrelated to mesh_id).

## Regression-guard verifications

- **#801 STRM-N1 wired**: `main.rs:618` (`step_streaming::unload`) and `:764` (`consume_streaming_payload::Ok(Some)`) both call `ctx.signal_temporal_discontinuity(SVGF_TAA_STREAMING_RECOVERY_FRAMES)`, propagating to `taa.signal_history_reset()` at `context/mod.rs:1632`. Cell-streaming history reset intact.
- **#641 / motion-vector skinning**: `triangle.vert:211` uses `bones_prev` to compose `prevWorldPos`, not the static current-pose matrix. Verified.
- **Un-jittered motion vectors**: `triangle.vert:151-152` comment + line 210 capture before line 219 jitter — verified structurally.
- **#676 / DEN-6 HDR alpha**: `currA` captured at `taa.comp:90` and rewritten at all three `imageStore` sites (94 / 123 / 166).
- **CLAMP_TO_EDGE sampler** (taa.rs:176-178) covers Catmull-Rom 9-tap edge-sampling; offscreen check at `taa.comp:103` redundant-but-safe.
- **Halton(2,3) jitter range and period**: index `(frame_counter % 8) + 1` cycles `1..=8` over distinct values; jitter mapped to `[-0.5, 0.5)` pixels.

## Coverage gaps

- **Visual-quality A/B**: not measured. Read-only audit; no RenderDoc capture. Both NEW findings are structural-correctness gaps with bounded visual impact (NaN propagation: dormant; mesh_id 15-bit drift: 1-frame flicker on rare transitions). A live A/B against `BYROREDUX_RENDER_DEBUG` flags would tighten severity but was out of scope.
- **TAA-disabled path**: didn't trace whether the dispatch is skipped entirely vs run-and-ignored when TAA is off. Worth a follow-up if a TAA-disable toggle ships (per audit checklist: *"the TAA dispatch should be skipped entirely (not run + ignored)"*).
- **`MAX_FRAMES_IN_FLIGHT` × Halton period seam**: Halton period is 8 frames, FIF cycle is 2 — no seam structurally, but the jitter sequence at FIF boundary `(frame n, frame n+2, frame n+4, …)` is a 4-element subsample of the Halton sequence rather than the full 8-element pattern. Each FIF slot sees a non-uniform 4-sample sub-pattern. Probably below the visible floor; flag for future deep-pass.

## Suggested next step

Both NEW findings are LOW + suggested-fix-sketches are non-Vulkan-state (pure shader edits) and safe under the speculative-vulkan-fix policy. They can land together in a small `taa.comp` patch.

```
/audit-publish docs/audits/AUDIT_RENDERER_2026-05-08_DIM11.md
```
