# #1874 — Ghosted diagonal double-image artifact in TES interiors, sticks after camera parks — likely bad shared motion vector

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/1874
**Labels**: bug, renderer, high
**Filed via**: /audit-publish docs/audits/AUDIT_RENDERER_2026-07-04.md

---

- **Severity**: HIGH
- **Dimension**: Denoiser/Composite (SVGF) + TAA
- **Location**: no single root-cause site confirmed; traced-and-cleared candidates: `crates/renderer/shaders/triangle.vert` (clip-position/motion emission), `crates/renderer/shaders/triangle.frag` (`outMotion` write, ~line 350), `crates/renderer/src/vulkan/svgf.rs`, `crates/renderer/src/vulkan/taa.rs`, `crates/renderer/src/vulkan/context/draw.rs` (`prev_view_proj`/`prev_render_origin` update + `origin_corrected_prev_view_proj`, ~lines 2695-2696/3937)
- **Status**: NEW

## Description

Live user-reported symptom: in a Skyrim interior, a ghosted/doubled
translucency artifact appeared — two overlapping renders of the room blended
at roughly 50% opacity, offset diagonally, including a doubled view of the
player's own body. The artifact appeared stuck in place.

Full adversarial investigation in `docs/audits/AUDIT_RENDERER_2026-07-04.md`
(Symptom 2) narrowed the mechanism but did **not** find the root cause —
filing this as a tracking issue per the audit's "needs RenderDoc" item so it
isn't lost, not as a proposed fix.

## Evidence

Four dimensions cross-checked hypotheses:

- RT reflection/refraction/glass paths ruled out as the origin (no path there
  produces a full-screen ghosted duplicate).
- The G-buffer Mesh-ID attachment's bit 31 (`ALPHA_BLEND_NO_HISTORY`, the
  SVGF-accumulation-skip marker) write/decode confirmed correct end-to-end —
  not the cause.
- **Leading hypothesis (H1)**: a spatially-uniform wrong motion vector shared
  by both SVGF and TAA. The symptom (a full-screen, uniform, diagonal offset
  doubling both room geometry and the player's own body) is inconsistent with
  a per-object/per-material fault, and would pass every disocclusion/
  normal-cone rejection test on large flat interior surfaces (uniform
  mesh-ID, uniform normal).
- A secondary hypothesis (stale `prev_render_origin` from a skipped
  `draw_frame` early-out) was **definitively ruled out**: the
  `prev_view_proj`/`prev_render_origin` pair updates atomically with no
  branch between them, and stays self-consistent even across skipped frames.
- **Amplifier mechanism found**: TAA's parked-camera path deliberately skips
  the YCoCg luma clamp (intentional, #1479 — harmful to convergence when
  parked) and drives blend weight toward ~99% history. If a transient bad
  motion vector baked a doubled image into history, parking the camera
  freezes the ghost in place with no remaining rejection mechanism — matches
  the "stuck ~50%" character exactly.
- **Not found**: the actual origin of the bad motion vector. Every authoring
  site traced (`triangle.vert` clip-position emission, `triangle.frag`
  `outMotion` write, CPU-side `origin_corrected_prev_view_proj`) reads
  correct on static analysis.

## Impact

Visible full-screen rendering corruption in TES-family interiors (confirmed
in Skyrim) that persists indefinitely once the camera stops moving, until
the camera moves enough to force TAA history rejection. No known repro
trigger yet (transient — likely tied to a specific frame event: cell
transition, teleport, or a skinned/dynamic object entering view).

## Related

None — first report of this specific artifact.

## Suggested Fix

**None proposed** — this failure mode is invisible to `cargo test` per the
project's no-speculative-Vulkan-fix policy. A live RenderDoc capture on the
affected frame is required first, checking:
1. Whether the motion-vector G-buffer shows a uniform full-screen offset
   (supports camera-level hypothesis) vs. localized to the body (would
   redirect to skinning).
2. `prevViewProj` in the CameraUBO vs. the actual previous frame's
   `viewProj`, and both `render_origin` values, on that frame.
3. SVGF history + `histAge` — is the ghost baked into accumulated history,
   sitting at the sticky ~50% blend regime the symptom's "50% opacity"
   description matches.
4. TAA history in the same capture — confirming the doubled direct-lit
   geometry appears there too would confirm H1 over an SVGF-indirect-only
   explanation.

## Completeness Checks
- [ ] **TESTS**: Once root-caused, a regression test/golden-frame pins the specific transient event that produces the bad motion vector
- [ ] **SIBLING**: Once root-caused, check whether the same transient affects other games/cell types beyond the Skyrim interior it was first observed in
