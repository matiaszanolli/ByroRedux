---
issue: 1872, 1874
title: |
  1872: memory-budget.md doesn't track any screen-sized RT-denoiser image resources (SVGF, SSAO, TAA, bloom, volumetrics, water/caustic)
  1874: Ghosted diagonal double-image artifact in TES interiors, sticks after camera parks — likely bad shared motion vector
labels: documentation (1872); bug, renderer, high (1874)
state: OPEN, OPEN
---

## #1872 — memory-budget.md VRAM ledger gap

**Severity**: LOW
**Source**: SIBLING check while fixing #1814 (PERF-D5-NEW-04, ReSTIR reservoir SSBOs untracked in memory-budget.md)

Fixing #1814 added a ReSTIR Reservoirs section to docs/engine/memory-budget.md for the screen-sized ReSTIR-DI reservoir SSBOs. While doing the sibling sweep, grep confirmed the doc has zero mentions of SVGF, Bloom, SSAO, TAA, Volumetrics, Water, or Caustic — every other screen-sized per-pass image/buffer resource in the renderer is absent from the VRAM ledger, not just ReSTIR.

**Suggested Fix**: Audit each pass (svgf.rs, bloom.rs, ssao.rs, taa.rs, volumetrics_inject.comp/_integrate.comp owner, water.rs/water_caustic.rs) for its screen-sized image/buffer footprint at common resolutions, and add a row (or section, for the larger ones) to memory-budget.md's VRAM ledger — mirroring the format #1814 established for ReSTIR reservoirs.

## #1874 — Ghosted diagonal double-image, sticks after camera parks

**Severity**: HIGH
**Dimension**: Denoiser/Composite (SVGF) + TAA
**Location**: no single root-cause site confirmed; traced-and-cleared candidates: `triangle.vert` (clip-position/motion emission), `triangle.frag` (`outMotion` write, ~line 350), `svgf.rs`, `taa.rs`, `draw.rs` (`prev_view_proj`/`prev_render_origin` update + `origin_corrected_prev_view_proj`, ~lines 2695-2696/3937)

Live user-reported symptom: in a Skyrim interior, a ghosted/doubled translucency artifact appeared — two overlapping renders of the room blended at roughly 50% opacity, offset diagonally, including a doubled view of the player's own body. The artifact appeared stuck in place.

Full adversarial investigation in `docs/audits/AUDIT_RENDERER_2026-07-04.md` (Symptom 2) narrowed the mechanism but did **not** find the root cause — filed as a tracking issue per the audit's "needs RenderDoc" item, not as a proposed fix. **No known repro trigger** at filing time (transient).

**Evidence from the original audit**:
- RT reflection/refraction/glass paths ruled out (no path there produces a full-screen ghosted duplicate).
- G-buffer Mesh-ID attachment bit 31 (`ALPHA_BLEND_NO_HISTORY`, SVGF-accumulation-skip marker) write/decode confirmed correct — not the cause.
- **Leading hypothesis (H1)**: a spatially-uniform wrong motion vector shared by both SVGF and TAA — consistent with a full-screen uniform diagonal doubling of both room geometry and the player's body (would pass every disocclusion/normal-cone rejection test on flat interior surfaces).
- Stale `prev_render_origin` from a skipped `draw_frame` early-out: **ruled out** — the pair updates atomically, no branch between them.
- **Amplifier mechanism found**: TAA's parked-camera path skips the YCoCg luma clamp (intentional, #1479) and drives blend weight toward ~99% history — if a transient bad motion vector baked a doubled image into history, parking freezes the ghost with no remaining rejection mechanism. Matches the "stuck ~50%" character.
- **Not found**: the actual origin of the bad motion vector.

**My own repro this session (separate from the filed issue, found independently while investigating unrelated lighting work in `GSProspectorSaloonInterior`, FNV)**: the user reported and I visually confirmed the identical-looking artifact — a flat, ghosted, diagonally-streaked double-exposure panel — appearing consistently near the cell's `RestroomMirror01/02/03` props (`Clutter\RestroomMirror0{1,2,3}.NIF`, alpha-tested `brokenglasssheet01.dds`/`utilitydoorframe01.dds` submeshes). Confirmed via a clean A/B test (stashed all of today's unrelated session changes, rebuilt from unmodified `main`, reproduced identically at the same camera position) that this is **pre-existing and NOT caused by today's other work**, and the user separately confirmed **it reproduces across games, not just one title** — consistent with #1874 being a cross-title rendering-pipeline bug rather than FNV-specific content.

This gives #1874 something it didn't have at filing: **a reliable, on-demand repro** (`cargo run --release -- --esm FalloutNV.esm --cell GSProspectorSaloonInterior ... --bench-frames 200 --bench-hold`, then `byro-dbg`'s `cam.tp <restroommirror-entity-id>` + `screenshot`) instead of a transient, unreproducible event. Investigation should exploit this repro directly rather than re-deriving the prior audit's static-analysis chain from scratch.

## Suggested Fix (1874)

**None proposed at filing** — RenderDoc capture required per the project's no-speculative-Vulkan-fix policy. With a reliable repro now in hand, the plan is: reproduce live, capture the actual G-buffer motion-vector / SVGF history / TAA history state at the ghosted frame (via whatever introspection is available — RenderDoc if installed, or targeted debug-bit dumps), confirm whether H1 (uniform bad motion vector) holds specifically on/near the alpha-tested mirror geometry, and only then propose a fix.

## Completeness Checks
- [ ] **TESTS**: Once root-caused, a regression test/golden-frame pins the specific transient event that produces the bad motion vector
- [ ] **SIBLING**: Once root-caused, check whether the same transient affects other games/cell types beyond the Skyrim interior it was first observed in (already confirmed: also reproduces in FNV)
