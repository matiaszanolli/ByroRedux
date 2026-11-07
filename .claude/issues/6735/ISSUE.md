**Source**: `docs/audits/AUDIT_EXTERIOR_2026-11-07.md` (EXT-D3-03).

- **Severity**: LOW (test gap)
- **Dimension**: Ground-cover pipeline
- **Location**: `byroGcLocalGust` (`crates/renderer/shaders/include/groundcover_wind.glsl`) vs `groundcover::local_gust` (`crates/core/src/ecs/components/groundcover.rs`); `byroGcGrassColour` (`crates/renderer/shaders/groundcover_blade.vert`) vs `cover_density_inputs` (`byroredux/src/cell_loader/terrain.rs`)
- **Tier Violated**: n/a
- **Game Affected**: all
- **Status**: NEW (the gust half is mentioned in the audit skill as unpinned, but no issue tracks it)

## Description
1. **Gust mirror.** The stateless local-gust lattice walk exists twice, in Rust and GLSL. The audit read both line by line and they match today: `lowbias32` constants, `salt * 0x9E3779B9`, centre range `[0.3, 0.7]`, the sin² envelope, salt `0xB16B00B5`; the `GROUNDCOVER_GUST_*` constants flow from the core `WIND_GUST_*` values through the generated header. Nothing stops a one-sided edit of the hash or cell logic.
2. **Grass-colour wiring.** Nothing tests that `layer_grass_colour` follows the splat lane order, or that the ordered-mix algebra in `byroGcGrassColour` drops unknown lanes. The only coverage is the `union_colour` unit test and a substring check in `blade_ground_colour_uses_the_shared_terrain_splat_chain`.

## Impact
A future edit to one side changes wind or grass hue on screen with every test green.

## Suggested Fix
A CPU mirror test, or a shader-text check that normalises both bodies, as `crates/renderer/src/vulkan/scene_buffer/terrain_height_blend_tests.rs` does for the height blend. Add a host test that `cover_density_inputs` fills the colour lanes in the same order as `layer_affinity`.

## Related
#6248 (same shader), `da6d65dc1`, `69a3b0b30`.

## Completeness Checks
- [ ] **SIBLING**: other Rust↔GLSL mirrors in ground cover (`groundcover_lod.glsl`, wind bend fraction) checked for a pin
- [ ] **TESTS**: both couplings pinned
