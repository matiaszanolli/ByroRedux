# TD7-101: triangle.frag hand-writes INST_RENDER_LAYER_SHIFT/_MASK instead of the generated shader-constants header

**GitHub Issue**: #2045
**Labels**: high,renderer,tech-debt,bug

**Severity**: HIGH
**Dimension**: 7 (Magic Numbers & Hardcoded Constants)
**Location**: `crates/renderer/shaders/triangle.frag:80-81,402` vs. `crates/renderer/src/vulkan/scene_buffer/constants.rs:216-217`

## Description
`scene_buffer/constants.rs:216-217` defines the authoritative `INSTANCE_RENDER_LAYER_SHIFT`/`_MASK`, whose doc comment explicitly names the fragment shader's debug-viz branch as a consumer — but unlike every sibling `INSTANCE_FLAG_*`/`MAT_FLAG_*`/`MATERIAL_KIND_*` define (all emitted via the generated header in `shader_constants_data.rs` and pinned by a `*_match_*` guard test in `shader_constants.rs`), this pair was never added to `shader_constants_data.rs`. `triangle.frag` hand-declares the same two constants instead.

## Evidence
`crates/renderer/shaders/triangle.frag:80-81`: `const uint INST_RENDER_LAYER_SHIFT = 4u; const uint INST_RENDER_LAYER_MASK  = 0x3u;`, consumed at `:402` inside the live `DBG_VIZ_RENDER_LAYER` branch (not dead code). Confirmed `INSTANCE_RENDER_LAYER_SHIFT`/`_MASK` do not appear anywhere in `crates/renderer/src/shader_constants_data.rs` (only referenced in a doc comment at line 291 of that same file) or in `crates/renderer/build.rs`'s `#define` emission list — so the generated shader header never carries them, and no `*_match_*` lockstep test exists for this pair.

## Impact
If `RenderLayer`'s bit-packing ever changes, the Rust and shader sides can silently drift with no compiler or test error — invisible to `cargo test`, only visible as a wrong debug-viz color. Exactly the `feedback_shader_struct_sync.md` lockstep-drift pattern.

## Related
`#1190` (the `INSTANCE_FLAG_*` lockstep fix this pair should have followed); `feedback_shader_struct_sync.md`.

## Suggested Fix
Add the two consts to `shader_constants_data.rs`, let `build.rs` emit them, delete the 2 hand-written shader lines, add an `instance_render_layer_bits_match_scene_buffer_consts` test mirroring the existing `INSTANCE_FLAG_*` guard.

**Age**: introduced `088696e9` (2026-05-03), never migrated when siblings got their lockstep test.
**Effort**: small

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every other `INSTANCE_FLAG_*`/`MAT_FLAG_*`/`MATERIAL_KIND_*` define already has this lockstep test — confirm no other shader hand-writes a constant this pair's fix leaves behind)
- [ ] **TESTS**: A regression test pins this specific fix (`instance_render_layer_bits_match_scene_buffer_consts`)
