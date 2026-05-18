# #1159 — REN-D10-NEW-12: SVGF nearest-tap fallback compares nearID == currID without ALPHA_BLEND_NO_HISTORY mask

**Severity**: LOW
**Domain**: renderer
**Status**: OPEN
**Source Audit**: `docs/audits/AUDIT_RENDERER_2026-05-17_DIM9_DIM10.md` — Dimension 10

## Location

`crates/renderer/shaders/svgf_temporal.comp:217`

## Description

The bilinear consistency loop at lines 140-192 masks bit 31 (the `ALPHA_BLEND_NO_HISTORY` marker added by #904 / #992) on both `prevID` and `currID` before comparing:

```glsl
if ((prevID & 0x7FFFFFFFu) != (currID & 0x7FFFFFFFu)) continue;
```

The sub-pixel-motion fallback below it at line 217 uses unmasked equality:

```glsl
if (nearID == currID && dot(currN, nearN) >= 0.9) {
```

The early-out at line 97 already guarantees `currID`'s bit 31 is unset (the shader returns before this code for sky / current-alpha-blend), so the only way the comparison falsely fails is when `nearID` has bit 31 set (previous frame was alpha-blend at this pixel) and the underlying 31-bit instance ID happens to match. Same-instance opaque ↔ alpha-blend transitions are rare but real (glass props with stage-controlled opacity, character cloaks moving between alpha-tested and alpha-blended draw paths during animation phases).

## Suggested Fix

Mirror the bilinear loop's mask — 1-line change:

```glsl
if ((nearID & 0x7FFFFFFFu) == (currID & 0x7FFFFFFFu) && dot(currN, nearN) >= 0.9) {
```

No behavioural change in the dominant path (currID's bit 31 is always 0 by the early-out, so masking currID is a no-op; masking nearID is the actual fix).

## Related

- #904 / #992 — alpha-blend bit-31 encoding into mesh-ID (R32_UINT format)
- #1131 — sub-pixel motion fallback added in REN-D10-NEW-01; this finding catches the masking inconsistency introduced alongside it
