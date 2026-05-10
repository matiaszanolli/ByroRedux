# #904 — REN-D11-NEW-02: TAA disocclusion compares full u16 mesh_id — bit-15 alpha-blend toggle force-resets history on opacity transitions

**Severity**: LOW (one-frame flicker on rare transitions)
**Domain**: Vulkan renderer · TAA disocclusion · shader-only
**Source audit**: `docs/audits/AUDIT_RENDERER_2026-05-08_DIM11.md` § Dimension 11
**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/904
**Status**: NEW · CONFIRMED at HEAD `53f4f64`

## Location

`crates/renderer/shaders/taa.comp:107-120` — disocclusion + alpha-blend gates

## Summary

`disocclusion = (currMid != prevMid)` at `taa.comp:111` compares the full u16. Per `helpers.rs:54-62`, bit 15 is reserved as `ALPHA_BLEND_NO_HISTORY`; the 15-bit instance ID lives in bits 0-14. Same-instance opacity transitions (alpha-blended ↔ opaque) flip bit 15 even when the 15-bit instance ID is identical, force-resetting history for one frame.

The dedicated `alphaBlend = (currMid & 0x8000u) != 0u` at line 120 already short-circuits to current-pass-through for *currently* alpha-blended pixels, so the drift only manifests when current is opaque AND previous was alpha-blended on the same instance.

## Fix path (shader-only)

```glsl
bool disocclusion = ((currMid & 0x7FFFu) != (prevMid & 0x7FFFu));
```

Same line (`taa.comp:111`). Mask to bits 0-14 so disocclusion sees only the instance ID; bit-15 retains its meaning via the dedicated `alphaBlend` path. After GLSL change: regenerate `.spv`.

**Sibling**: SVGF temporal pass does its own mesh_id disocclusion — apply same masking convention in same patch if applicable.

## Related

- #903 — sibling Dim 11 NaN finding; both shader-only, can land together
- #676 / DEN-6 (closed) — HDR alpha preservation through TAA (different bit, same "must not clobber marker bit" theme)
- `helpers.rs:54-62` — invariant declaration site for the 15-bit / bit-15 split
