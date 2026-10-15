# REN-D7-2026-10-14-01: `view_proj_is_static` (#5370) tolerance depends on the 4096-snapped render origin: too tight for some standing poses, too loose for near geometry

**Labels**: medium,renderer,bug
**Source**: docs/audits/AUDIT_RENDERER_2026-10-14.md

Found by `/audit-renderer` (docs/audits/AUDIT_RENDERER_2026-10-14.md). Finding ID: `REN-D7-2026-10-14-01`. Status: NEW, confirmed against HEAD `adbd468e2`.

**Location**: `crates/renderer/src/vulkan/context/assemble_camera_and_lights.rs` (`view_proj_is_static`, `VIEW_PROJ_STATIC_REL_EPS`, `VIEW_PROJ_STATIC_ABS_EPS`, `view_proj_static_tests`), from `b909a6928`; `snap_render_origin` (`scene_buffer/constants.rs`)

## Description
The test allows `1e-6 + 1e-5 * max(|a|,|b|)` per matrix entry. The view-projection is render-origin-relative and the origin is snapped to a 4096-unit grid, so translation entries scale with the camera's offset from an arbitrary grid plane, not with pixel motion. Merges REN-D7-2026-10-14-01, REN-D2-2026-10-14-02 and REN-D4-2026-10-14-01 (three agents found the two directions independently).

- **Too tight (Dim 7):** the measured wobble (±1.6e-3 on Y) clears the flag only if `1e-5 * |entry| >= 1.6e-3`, i.e. eye height above the snapped plane >= 160 units for a level camera.
- **Too loose (Dim 2, Dim 4):** the docblock derives the sub-pixel bound at the origin's clip `w` (~2120). A surface at `w` = 10 sees the allowed translation delta (~0.045 u) as ~2.9 px/frame, so slow camera creep still reports parked.

## Evidence
- Dim 7 numeric model of the shipped predicate (20000 poses): level camera h=128 -> flag clear in 0% of poses, h=200 -> 100%, h=364 -> 100%; with ±20° pitch, 63–84% of poses parked for h=60–364. The 160-unit threshold is analytic; the model's absolute numbers are not a live capture.
- Too-loose arithmetic: `vp[12] = -4536.442`, `4536.442 * 1e-5 = 0.0454`; at 10 u and 1280 px that is ~2.9 px.
- `view_proj_static_tests` pin one matrix (Institute capture, `vp[13]=879`) at one eye height, so they see neither direction.
- Motion vectors still use the true previous matrix, so no mis-registration/ghosting; the too-loose side costs slight indirect softening.

## Impact
Where the flag stays false for a standing player, ReSTIR keeps its moving cap (16 / floor 0.1: the exact speckle #5370 reported), SVGF keeps its alpha floor and the caustic parked EMA never engages; behaviour then varies with position and heading, which is hard to bisect. Where it is too loose, slow drift engages the deep ReSTIR tail (1024 / 0.002) and progressive SVGF. Not measured live (the `restir_history` debug view, red = rejected, is the quickest check). Related: #5370 (closed), #4942, #5369.

## Suggested Fix
Test in an origin-independent space: reprojection error of a few probe points at near and mid depth (max NDC delta below a fraction of a pixel), or compare camera position (world units) and rotation block separately. Extend `view_proj_static_tests` with the wobble at eye heights 60 and 128 above the origin plane (must clear) and a 0.05-unit step 2000 units from the origin (must not clear).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
