# D12-2026-08-07-03: Four expect() panics recorded inside the open render pass (water + UI vertex/index buffers) violate the #956 no-panic-while-recording rule

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2505
**Finding ID**: D12-2026-08-07-03 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 12 — Command-buffer recording
**Location**: `crates/renderer/src/vulkan/context/geometry_pass.rs:529-536` (water vb/ib) and `:620-627` (UI vb/ib)
**Status**: NEW

## Description
Between `cmd_begin_render_pass` (`:36`) and `cmd_end_render_pass` (`:637`), the water and UI draw paths use `.expect("water mesh requires a per-mesh vertex buffer")` / `"UI mesh requires a per-mesh vertex buffer"` on `mesh.vertex_buffer` / `mesh.index_buffer`. `#956` / REN-D5-NEW-05 established the opposite house rule for this exact region (a `debug_assert!` was removed from the instance-overflow site because "it leaks the in-flight cmd buffer on unwind"), and the sibling `dispatch_direct` closure twenty lines above handles the identical `None` case gracefully: "A global-only scene mesh (distant terrain LOD, #1370) carries no per-mesh buffers — skip it".

## Evidence
`mesh.rs::upload_scene_mesh_global_only` (`:526-544`) produces `vertex_buffer: None`. Its callers today are LOD-only (`placement_lod.rs:485`, `object_lod.rs:295`, `terrain_lod.rs:657`, `terrain_lod_btr.rs:202`), so water/UI meshes always take the per-mesh path and the panic is unreachable **right now**. The precondition is a call-site convention, not a type-level guarantee.

## Impact
If any future path ever registers a water plane or UI quad global-only (e.g. a WATAL water-LOD tier), the panic unwinds with `cmd` mid-render-pass, `image_available[frame]` signal-pending, and `images_in_flight[img]` already pointing at this frame's fence — the leak class the six explicit `recreate_image_available_for_frame` recovery sites in `draw_frame` exist to prevent.

## Related
#956 / REN-D5-NEW-05, #1370 (global-only meshes), #910 / REN-D5-NEW-01, #1188 / REN-D1-NEW-05.

## Suggested Fix
Replace the four `expect()`s with `let ... else { continue; }` (water loop) / `else { /* skip overlay */ }` (UI block), mirroring `dispatch_direct`'s existing graceful skip, plus a one-line `log::warn!` gated by a `Once`.

## Completeness Checks
- [ ] **TESTS**: A regression test constructs a global-only water/UI mesh and confirms the draw path skips gracefully instead of panicking
