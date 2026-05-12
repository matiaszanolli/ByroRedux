# #964 — REN-D10-NEW-07: SVGF force_history_reset shares counter across FIF slots

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-11_DIM10.md`
**Dimension**: Denoiser & Composite
**Severity**: LOW
**Confidence**: LOW (conditional on MAX_FRAMES_IN_FLIGHT > 2)
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/964

## Location

`crates/renderer/src/vulkan/svgf.rs:132-134` — counter shared across FIF slots.

## Summary

Safe under current `MAX_FRAMES_IN_FLIGHT == 2`. At 3+ buffered frames, the gate closes while one slot may still be UNDEFINED for pixels that took the alpha-blend / sky early-out in `svgf_temporal.comp:93-97`.

## Fix (preferred)

Per-slot counter: `frames_since_creation: [u32; MAX_FRAMES_IN_FLIGHT]`. Each slot advances independently when its dispatch runs; reset together on `recreate_on_resize`.

Alternative: change gate to `frames_since_creation < MAX_FRAMES_IN_FLIGHT * 2` (one-line, lossier).

## Tests

Re-verify existing recovery-window tests against per-slot model. Add `frames_since_creation_is_per_slot` pin.
