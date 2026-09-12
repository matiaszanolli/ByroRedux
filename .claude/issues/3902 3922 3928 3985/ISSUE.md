# Batch: #3902, #3922 (skipped), #3928, #3985

## #3902 — REN-2026-09-05-D7-01: secondary-ray albedo drops every albedo-modifying texture role
**Severity**: MEDIUM · **Location**: `crates/renderer/shaders/include/ray_hit.glsl` (`rayHitAlbedo`)

`rayHitAlbedo` applied only the constant diffuse tint. Five albedo-modifying texture roles the
raster path composes are read by no secondary ray: `decals[0..3]`, `tint`, `inner_layer`, `dark`,
`detail`. RT reflections/GI/water refraction shaded a different surface colour than raster.

Fix: Ported the raster path's exact composition (decal alpha-over, diffuse tint, tintMap,
innerLayer gated on MULTI_LAYER_PARALLAX, dark, detail) into `rayHitAlbedo`, widening its
signature to `(mat, uv, baseRgb, lod)`. Updated all 4 call sites. Recompiled SPIR-V.

Note: the COVERAGE half (decal alpha reaching `rayHitHasCoverage`) was already fixed by #3986
before this session — only the colour half remained.

## #3922 — SK-2026-09-05-D2-01: model-space-normal branch consumes _msn maps in the source basis — SKIPPED
**Severity**: HIGH

Skipped per user decision: the issue explicitly requires (a) a fresh per-vertex correlation
measurement on real FO4 `_msn` data (only Skyrim was measured) before touching this shared
branch, and (b) a RenderDoc/screenshot confirmation on a real FaceGen head before landing, not a
unit test alone. This sandbox has no game data and no Vulkan device. Left open for a session with
the right environment.

## #3928 — FO4-2026-09-05b-D9-01: nothing gates the lit palette path against real FO4 content
**Severity**: MEDIUM · **Location**: `crates/nif/src/import/material/fo4_shader_flag_tests.rs`,
`byroredux/src/asset_provider/tests/bgsm_merge.rs`

Existing unit tests assert flag bits only; nothing observes a colour or a real authored value for
the FO4 lit-palette (grayscale-to-palette) shader path.

Fix: Added an `#[ignore]`d corpus census test (`fo4_palette_corpus.rs`) that walks real FO4
`Materials.ba2`, counts palette-enabled BGSMs, and asserts the path is exercised by real content
(count > 0) rather than accidentally dead. No FO4 data available this session to establish an
exact baseline count — documented in the test.

## #3985 — REN-2026-09-06-D18-01: authored-still cloud layer scrolls; fallback ignores wind direction
**Severity**: MEDIUM · **Location**: `byroredux/src/systems/weather.rs` (`cloud_scroll_vectors`),
`crates/plugin/src/esm/records/weather.rs`

(a) Authored zero cloud velocity is indistinguishable from absent — the value-based sentinel
substitutes motion the artist didn't ask for. (b) The wind-driven fallback ignores the record's
own authored wind direction, using a fixed sign/ratio table.

Fix: Added `WeatherRecord::cloud_layer_velocities_authored: [bool; 4]` (mirrors the existing
`wind_direction_authored` precedent), threaded through env_translate.rs/components.rs/weather.rs
including the cross-fade transition path. `cloud_scroll_vectors` now branches on presence, and
its fallback rotates the per-layer ratio table by the authored wind direction (a no-op rotation
at the legacy east default, preserving backward compatibility exactly).
