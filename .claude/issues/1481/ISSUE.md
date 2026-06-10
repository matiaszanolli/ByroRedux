# #1481 — REN-D10-NEW-01: SVGF spatial firefly clamp scoped inside hasHistory branch

_Snapshot as filed 2026-06-09 from docs/audits/AUDIT_RENDERER_2026-06-09.md. GitHub is authoritative for live state._

**Severity**: LOW (cosmetic, self-heals next frame)
**Dimension**: Denoiser & Composite (SVGF)
**Source**: `docs/audits/AUDIT_RENDERER_2026-06-09.md`
**Status**: NEW

## Description
In `crates/renderer/shaders/svgf_temporal.comp`, the spatial firefly clamp (the 3×3 neighbour-max luminance cap, ~`:280-317`) is scoped **inside** the `if (hasHistory)` branch. On a freshly-disoccluded pixel (no history → `hasHistory == false`), a GI spike enters history un-clamped for one frame before the temporal clamp can act on it next frame.

## Evidence
- `svgf_temporal.comp:280-317` — the spatial neighbour-max clamp executes only on the has-history path; the no-history (disocclusion) path writes the current sample to history without the spatial cap.

## Impact
A one-frame firefly can leak into history on disocclusion edges. Self-heals on the following frame once history exists. Purely cosmetic — no crash, no validation error.

## Suggested Fix
Hoist the spatial firefly clamp ahead of the `if (hasHistory)` branch so it applies on the first (no-history) frame as well as subsequent frames.

## Completeness Checks
- [ ] **SIBLING**: check whether the TAA resolve (`taa.comp`) has an analogous first-frame un-clamped path on disocclusion.
- [ ] **TESTS**: N/A practical (visual); document the manual repro if added.
- [ ] **UNSAFE / DROP / LOCK_ORDER / FFI / CANONICAL-BOUNDARY**: N/A (shader-only).
