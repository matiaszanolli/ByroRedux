# #4967: REN-D7-2026-09-27-05: The dormant DOF-through-TAA path cannot produce bokeh — the lens offset is baked into both view-proj matrices, so motion vectors cancel it

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4967
- **Labels**: low,renderer,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D7-2026-09-27-05**._

- **Severity**: LOW (dormant: no production writer sets `Camera.aperture > 0`; only `Default` 0.0 and tests)
- **Dimension**: TAA
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` `dof_effective_view_proj` (its doc: "non-zero motion → reduced TAA weight → blur"). `assemble_camera_and_lights.rs` passes `vp = &effective_vp` into `GpuCamera.view_proj`, into `history.prev_view_proj`, and into the `camera_static` compare. `triangle.vert` computes `fragCurrClipPos = viewProj * worldPos` and `fragPrevClipPos = prevViewProj * prevWorldPos`.
- **Status**: NEW.
- **Description**:
  - TAA sub-pixel jitter is added to `gl_Position` **after** the motion clip positions are taken, so motion vectors are jitter-free.
  - The DOF lens offset instead lives inside `view_proj` and `prev_view_proj`, so each frame's motion vector contains the lens parallax.
  - TAA fetches history at `uv − motion` and re-aligns each world point, then blends at a **flat** α = 0.1: `taa.comp` has no motion-dependent weight. The per-frame parallax is therefore undone instead of integrated, and out-of-focus surfaces converge sharp, not blurred.
  - DOF also makes `camera_static` false every frame, which disables SVGF progressive accumulation and the ReSTIR parked cap.
- **Evidence**: `taa.comp`: `float alpha = params.params.x;` (host `let alpha = 0.1;` in `taa.rs`). No motion term in the blend.
- **Impact**: None today, because the path is dormant. If DOF is enabled (imagespace DOF is on the roadmap), it will look sharp and the doc will mislead whoever debugs it.
- **Related**: #4002, #2518 (`fsr_gated_dof`).
- **Suggested Fix**: Build the motion-vector matrices from the pinhole camera (keep a separate pinhole `prev_view_proj`) and apply the lens offset like the jitter, or correct the doc to state that DOF needs a dedicated pass.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
