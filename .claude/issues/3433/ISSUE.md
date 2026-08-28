# #3433: UI-D6-2026-08-27-07: `docs/engine/ui.md` still describes a 6-attachment main render pass; it has 8 (+depth)

- **Severity**: LOW
- **Dimension**: Catalog Fidelity & Drift (doc rot)
- **Profile**: both
- **Location**: `docs/engine/ui.md:297-301` vs `crates/renderer/src/vulkan/context/helpers.rs:148-190` and `crates/renderer/src/vulkan/pipeline.rs:941-955`
- **Source**: `docs/audits/AUDIT_UI_2026-08-27.md` (UI-D6-2026-08-27-07)

## Description

The doc reads "The main render pass has 6 color attachments (HDR + normal + motion + mesh-id + ...). The UI pipeline writes RGBA to slot 0 (HDR) only; **the other five** attachments use a no-op blend state". The pass has **8** color attachments plus depth — slots 6 (`fsr_reactive`, `R8_UNORM`) and 7 (`fsr_transparency`, `R8_UNORM`) landed with the FSR3 work — and `create_ui_pipeline` correctly masks **seven** of them. The masking itself is right; only the count is stale.

## Evidence

`crates/renderer/src/vulkan/context/helpers.rs:148` — "Main render pass writes to 8 color attachments + depth", enumerated 0-7 plus 8 = depth.

`crates/renderer/src/vulkan/pipeline.rs:941-955` — one `ui_hdr_blend` (slot 0) plus seven `ui_noop_blend` (slots 1-7, including `// 6 fsr_reactive` and `// 7 fsr_transparency`).

## Impact

Cosmetic, but it is a G-buffer contract number in the doc that owns the UI's render integration, and the same doc's next paragraph correctly pins the 20-byte `UiVertex` — so a reader has no cue that one number is maintained and the other is not.

## Related

#2730 / #3088 / #3153 / #3272 (the same drift class).

## Suggested Fix

`6` -> `8`, `five` -> `seven`, and name slots 6/7.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other attachment-count claims in `docs/engine/`)
