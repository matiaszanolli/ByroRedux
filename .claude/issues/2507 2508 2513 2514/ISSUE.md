# Issue batch: 2507, 2508, 2513, 2514 (renderer, all LOW)

## #2507 — REN-D14-2026-08-07-04 (caustics)
`record_caustic_splat_pass` (`crates/renderer/src/vulkan/context/post_passes.rs`)
skips its ENTIRE body — including the `cmd_clear_color_image` — on both the
`caustic_failed` permanent latch and `tlas_handle == None` paths. The
accumulator freezes with stale contents; `composite.frag` samples it
unconditionally every frame (no validity gate), so a frozen pattern paints
over the whole scene until a resize recreates the slots.

Fix: on either skip path, record a one-shot `cmd_clear_color_image` on
`slots[frame]` (existing GENERAL→GENERAL barriers), gated by a
`caustic_cleared_on_skip: [bool; MAX_FRAMES_IN_FLIGHT]` latch so it's one
clear per slot, not every frame. Mirrors the #479 SVGF permanent-failure
latch pattern.

## #2508 — REN-D15-NEW-03 (water caustics)
Composite binding 8 (`waterCausticTex`) falls back to `caustic_views` (the
glass/MLP caustic accumulator's OWN sampled views — already bound at
binding 5, `causticTex`) whenever `water_caustic_accum` is `None`
(`crates/renderer/src/vulkan/context/mod.rs:2596-2603` init,
`resize.rs:852-857` resize). `composite.frag` sums both bindings, so this
silently doubles glass-caustic brightness instead of contributing zero.
The write-side twin of this bug was already fixed via
`placeholder_caustic_sink` (#2142) — the read side never got the same fix.

Fix: bind binding 8 to a genuinely zero-valued R32_UINT image at full
render resolution on the fallback path (not a 1×1 sink — `composite.frag`
texelFetches at `textureSize(causticTex, 0)` coordinates), correcting the
factually-wrong "causticAccum is all-zero" comment.

## #2513 — REN-D20-NEW-03 (telemetry)
`GpuTimerSnapshot`'s fourteen `*_active: bool` fields (added by #2278 to
disambiguate "inactive" from "genuinely 0ms") have zero consumers.
`fill_skin_coverage_stats` (`crates/renderer/src/vulkan/context/mod.rs`)
copies only the `_ms` fields; `SkinCoverageStats`
(`crates/core/src/ecs/resources/mod.rs`) has no `_active` members;
`metrics.rs`'s debug-UI grid prints `0.000 ms` for both "skipped" and
"ran instantly".

Fix: add matching `bool` fields to `SkinCoverageStats`, copy them in
`fill_skin_coverage_stats`, widen the `gpu_pass_ms` grid tuple to
`(String, Option<f32>)` so inactive brackets render "n/a".

## #2514 — REN-D21-2026-08-07-02 (PBR-BSDF)
`subsurface`/`sheen`/`sheen_tint`/`anisotropic` are hardcoded to `0.0`
literals in `collect_static_mesh_draws`
(`byroredux/src/render/static_meshes.rs:~627-633`) with a "when the
importer surfaces them" TODO — `GpuMaterial` carries the fields,
`pbr.glsl`/`lighting.glsl` consume them, but no CPU producer can ever
drive them non-zero. Three shipped shader lobes (fake-SSS, sheen,
anisotropic GGX) are dead code end-to-end.

Fix: plumb the four scalars from `Material` through `DrawCommand` (adding
fields if absent), expose via `mat.set` so the Cornell harness can sweep
them.

All four: LOW severity, renderer domain (`byroredux-renderer` +
`byroredux` binary crate). #2507/#2508 touch Vulkan record/bind logic —
follow existing precedent patterns (#479, #2142) rather than novel
pipeline changes. #2513/#2514 are CPU-side data-flow wiring, lower risk.
