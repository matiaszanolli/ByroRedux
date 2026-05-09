# Investigation — #905 (REN-AUDIT-CROSS-01)

## Confirmed state at HEAD (f62d4bd)

### composite.rs

- **Init** writes all 8 bindings (lines 596-676): HDR (0), indirect (1), albedo (2), params UBO (3), depth (4), caustic (5), volumetric (6), bloom (7).
- **`recreate_on_resize`** at line 849 takes 6 view sources (HDR-internal, indirect, albedo, depth, caustic) and writes only bindings 0-5 at lines 973-1004. Bindings 6 and 7 are never re-written.
- Layout is correctly declared with all 8 bindings (lines 462-513), and `validate_set_layout` at line 514 cross-checks against the SPIR-V — no shader-side drift.
- Pool is sized for `MAX_FRAMES_IN_FLIGHT * 7 + 1` (line 555-566).

### bloom.rs

- `BloomPipeline::new` at line 154 takes `screen_extent: vk::Extent2D` and sizes the down/up mip pyramid from it.
- Per-mip images are extent-dependent (line 750+).
- Pipelines, layouts, sampler, descriptor pool are NOT extent-dependent.
- `output_views()` at line 672 returns the per-frame-in-flight up_mips[0] views (the bloom output composite samples).
- No `recreate_on_resize` method exists.
- `destroy()` at line 676 is complete (drains frames, destroys pool/pipelines/layouts/sampler).

### volumetrics.rs

- Uses fixed `FROXEL_WIDTH = 160`, `FROXEL_HEIGHT = 90`, `FROXEL_DEPTH = 128` (lines 117-119).
- **NOT screen-extent-dependent** — does NOT need `recreate_on_resize` for image extent.
- Composite still needs to rewrite binding 6 to reference the (stable) `integrated_views()`.

### resize.rs

- Current composite call at line 375-388 passes 6 views (HDR-implicit, indirect, albedo, depth, caustic) — missing volumetric and bloom.
- SSAO uses destroy+new at call site (lines 213-253) — canonical pattern in this codebase for full-pipeline recreation.

### context/mod.rs

- Bloom + composite init at lines 1400-1450. Both consumed `volumetric_views` (from `volumetrics.integrated_views()`, line 1381-1390) and `bloom_views` (from `bloom.output_views()`, line 1419-1428) as of M58/M55 Phase 4.

## Failure mode (what currently breaks)

- After window resize, bloom's mip pyramid stays at the old extent. Composite samples bloom at the wrong texel size → bloom additive contribution drifts off bright surfaces.
- Volumetric's binding 6 is currently visible-gated off (`vol.rgb * 0.0` at composite.frag:362), so the missing rewrite there is currently silent.
- The composite descriptor still holds the original-extent image views, so there is no UAF *today*. The UAF window opens the moment bloom's `recreate_on_resize` ships without composite's `recreate_on_resize` being extended in the same PR.

## Fix decision

Atomic 4-part change in one PR:

1. **composite.rs**: Extend `recreate_on_resize` signature with `volumetric_views: &[vk::ImageView]` + `bloom_views: &[vk::ImageView]`. Add bindings 6 + 7 writes. Pin `[vk::WriteDescriptorSet; 8]` typed array (compile catches divergence) + `debug_assert_eq!` on length for runtime safety.

2. **resize.rs**: After SSAO destroy+new, do bloom destroy+new (mirrors SSAO). Snapshot volumetric_views (stable across resize) + bloom_views (fresh from new bloom). Pass both to extended composite recreate.

3. **No new method on BloomPipeline / VolumetricsPipeline**: SSAO is the canonical pattern for full-pipeline recreation; bloom matches that profile (no external views to rewrite). Volumetric needs no recreate at all (fixed froxel size).

4. **Binding-count guard**: typed `[_; 8]` array + `debug_assert_eq!(writes.len(), 8)` in both init and recreate.

## RenderDoc requirement

Per `feedback_speculative_vulkan_fixes.md`, this fix touches Vulkan descriptor writes + pipeline recreation whose failure modes are invisible to `cargo test`. Manual live-resize verification with VK_LAYER_KHRONOS_validation enabled is required before claiming done. Plan: implement → cargo test → flag user that RenderDoc-verified manual test is needed before close.

## Files touched

- `crates/renderer/src/vulkan/composite.rs` (signature + binding writes)
- `crates/renderer/src/vulkan/context/resize.rs` (bloom destroy+new + composite call site)

Estimated scope: 2 files, ~80 lines added.
