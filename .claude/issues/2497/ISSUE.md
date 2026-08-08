# REN-D10-NEW-02: caustic_splat.comp is the one CameraUBO re-declarer still missing #2164's renderOrigin.w payload note

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2497
**Finding ID**: REN-D10-NEW-02 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 10 — Camera-Relative Precision
**Location**: `crates/renderer/shaders/caustic_splat.comp:76`
**Status**: NEW (incomplete application of the fix for prior finding L-10 / #2164)

## Description
#2164 fixed the "w unused" documentation trap at `draw.rs`, `water.vert:83` and `cluster_cull.comp:69` — all three now read "w = FSR one-frame-reset flag (NOT padding — #2164/L-10)". The fourth `CameraUBO` re-declarer, `caustic_splat.comp`, was missed and still reads:
```glsl
vec4 renderOrigin;   // #markarth-precision — camera-relative render origin (added to inv_view_proj world reconstruction below). Keeps CameraUBO == sizeof(GpuCamera).
```
— no mention of `w` at all, and the trailing "Keeps CameraUBO == sizeof(GpuCamera)" reads as "this field is here for padding parity", exactly the reading #2164 set out to eliminate.

## Evidence
`grep -n "w unused" water.vert cluster_cull.comp draw.rs` → 0 hits (fixed); `caustic_splat.comp:76` carries neither the corrected wording nor a `w` description.

## Impact
Documentation only. Same latent trap class as the tracked `VolumetricsParams::render_origin.w` overload (#1928): a future author reading only this site could repurpose `w` and silently break the FSR reset-flag contract that `triangle.frag:582` (`clamp(renderOrigin.w, 0.0, 1.0)`) depends on.

## Related
Prior L-10 / #2164; #1928 / REN-D10-01.

## Suggested Fix
Copy `cluster_cull.comp:69`'s wording verbatim into `caustic_splat.comp:76`.

## Completeness Checks
- [ ] **TESTS**: N/A (comment-only change)
