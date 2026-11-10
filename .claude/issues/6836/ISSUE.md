# PERF-2026-11-09-D4-01: `CompositePipeline::upload_params` copies only `offset_of(sky_apertures) + count x 48` bytes, but `view_proj` (added last by `fa9ada406`) sits after all 256 apertures, so it is never uploaded

Labels: medium,renderer,bug
Filed from: volumetrics-deep audit suite, 2026-11-09

**Severity**: MEDIUM (visual regression in submerged views; found under the "upload sizing" focus, owned by the renderer). **Status**: NEW — introduced by `fa9ada406` (2026-11-04, "Refs #6486"). #4805 (closed) added the prefix upload on the assumption that the aperture array is the struct's tail; #6486 (open) is the issue the field was added for. No guard covers the pair: `composite_params_layout_*` asserts offsets and size only, and no test exercises `upload_params`.

**Location**: `crates/renderer/src/vulkan/composite.rs` `upload_params` (the `prefix_bytes` computation) and `CompositeParams` (`view_proj` is the last field, `offset_of == 528 + MAX_SKY_APERTURES * 48 = 12,816`); consumer `underwaterSunVisibility` / `underwaterGodRays` in `crates/renderer/shaders/composite.frag` (`mat4 viewProj = params.view_proj;`); producer `build_composite_params` (`context/frame_params.rs`, `view_proj: view_proj.to_cols_array_2d()`); sole caller `context/build_and_upload_instances.rs`.

## Description
`upload_params` writes `write_mapped_prefix(.., prefix_bytes)` with `prefix_bytes = offset_of(sky_apertures) + aperture_count * size_of::<CompositeSkyAperture>()`, to avoid copying the unused 12 KB aperture tail. Before `fa9ada406` nothing followed the array, so the prefix was the whole live struct. `fa9ada406` appended `view_proj` after the array ("appended last so every earlier offset is unchanged"). With `aperture_count = 0` (every exterior, almost every interior) the prefix is 528 bytes; `view_proj` at byte 12,816 is outside it and keeps whatever the per-frame UBO was created with. It is only written on a frame with all 256 apertures populated.

## Evidence
`write_mapped_prefix` does `bytes[..prefix_bytes.min(bytes.len())]`. The god-ray taps do `vec4 clip = viewProj * vec4(q, 1.0); if (clip.w <= 1.0e-3) continue;`, so a zero matrix skips every tap and `visible` stays 1.0.

## Impact
When the camera is submerged, the underwater god rays lose their contact-shadow occlusion (rocks, hulls, terrain no longer block the shafts) on every frame since 2026-11-04. If freshly allocated UBO memory is not zero, the taps project through garbage instead; either way the host's value never arrives. Visual-only, submerged-only. Confidence: the dropped bytes are CONFIRMED by code reading; the visual consequence is derived and needs one submerged capture (FNV or Skyrim lake). This also makes #6486's "half-fixed" state worse than reported: the per-pixel `inverse()` is gone, and so is the matrix.

## Suggested Fix
Make the upload cover the trailing member: write the prefix and then `view_proj` separately (`write_mapped_at` of the last 64 bytes), or move `view_proj` ahead of `sky_apertures` and shift offsets in lockstep with `composite.frag` and the layout test. Add a test that builds a `CompositeParams` with a sentinel `view_proj`, runs the prefix-length arithmetic for `aperture_count = 0`, and asserts the sentinel's bytes are inside the uploaded range.

Related: #6486 (open), #4805 (closed), `fa9ada406`. Report: `docs/audits/AUDIT_PERFORMANCE_2026-11-09.md`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (any other UBO uploaded via `write_mapped_prefix` with fields after the variable-length array)
- [ ] **TESTS**: A regression test pins this specific fix

