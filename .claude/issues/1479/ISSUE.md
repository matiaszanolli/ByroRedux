# #1479 — REN-D11-NEW-01: Parked-camera TAA luma-clamp skip ghosts moving actors

_Snapshot as filed 2026-06-09 from docs/audits/AUDIT_RENDERER_2026-06-09.md. GitHub is authoritative for live state._

**Severity**: MEDIUM (visual quality; no crash, no validation error)
**Dimension**: TAA (M37.5)
**Source**: `docs/audits/AUDIT_RENDERER_2026-06-09.md`
**Status**: NEW

## Description
`camera_static` is derived **only** from the frame-to-frame view-proj matrix delta (`crates/renderer/src/vulkan/context/draw.rs:635`) and is blind to scene/object motion. A walking actor, swinging door, or animated mesh leaves the camera matrix unchanged → `camera_static = true`. That flag flows into `taa.upload_params(.., camera_static)` (`draw.rs:2148`), which (a) drops α to `1/(N+1)` capped at 1/256 (`crates/renderer/src/vulkan/taa.rs:684-688`), and (b) sets `params.z = 1.0`, which makes `crates/renderer/shaders/taa.comp:225` **skip the luma (Y) variance clamp**, keeping only the Co/Cg chroma clamp.

A moving actor's interior pixels have non-zero motion vectors but the SAME `mesh_id` at the reprojected pixel (the actor occupies both current and reprojected pixel), so `disocclusion == false`, the temporal tap is taken at ~99.6% history weight with the anti-ghosting luma clamp disabled → luminance edges trail.

## Evidence
- `draw.rs:635` — `camera_static` ignores object motion (matrix-only).
- `taa.comp:147` — `disocclusion = ((currMid & 0x7FFFFFFFu) != (prevMid & 0x7FFFFFFFu))` is false for same-instance interior pixels of a translating actor.
- `taa.comp:225` — `if (cameraStatic) { histYc.yz = clamp(histYc.yz, yMin.yz, yMax.yz); }` — only chroma clamped; luma (`histYc.x`) passes unclamped.
- `taa.rs:684-688` — α collapses to `1/(static_frames+1)` (≈1/256), so moving-actor history dominates (~99.6% history weight).
- The shader's own comment (`taa.comp:217-221`) justifies the skip for a parked camera ("never settles") — valid for camera-induced jitter, but does not account for genuine in-scene motion.

## Impact
Visible luminance smear / ghost trails on moving actors and animated geometry whenever the player stands still — common (dialogue, menus open, AFK, scripted scenes). Worsens the longer the camera is parked (α → 1/256). Blast radius is the moving silhouette interior, not the whole frame.

## Suggested Fix
Gate the luma-clamp skip on **per-pixel** motion, not the global camera flag. In `taa.comp`, treat a pixel as static only when `cameraStatic && dot(motion,motion) < epsilon` (motion is already dilated at `:115-129`); otherwise fall through to the full `histYc = clamp(histYc, yMin, yMax)` path with the normal α=0.1. ~3 lines, no host change; preserves glass/rough-metal convergence on truly static pixels while re-arming the anti-ghost clamp on any moving fragment.

## Completeness Checks
- [ ] **SIBLING**: confirm the SVGF denoiser path doesn't have the analogous camera-static-vs-object-motion assumption.
- [ ] **TESTS**: add/extend a TAA test or a documented manual repro (parked camera + animated actor) confirming no luma trail.
- [ ] **UNSAFE / DROP / LOCK_ORDER / FFI / CANONICAL-BOUNDARY**: N/A (shader-only change).
