# REN-D4-01: nothing pins that every `cmd_set_depth_bias` site goes through `layer_depth_bias`

Labels: low, renderer, pipeline, test-gap, bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/7026

**Severity**: LOW
**Source**: `docs/audits/AUDIT_RENDERER_2026-11-16.md` (renderer audit, delta since 2026-11-09)
**Location**: `layer_depth_bias` (`crates/renderer/src/vulkan/pipeline.rs`); two call sites in `crates/renderer/src/vulkan/context/geometry_pass.rs`
**Status**: NEW (validated against HEAD; no matching open issue)

## Description
INC6-1 (`0c118907c`) fixed a reversed-Z sign flip that was missed for 8 days; only a core test (`a_layer_bias_pulls_toward_the_camera_under_either_mapping`) pins `map_depth_bias`, and no renderer test references `layer_depth_bias`. A new site reading `RenderLayer::depth_bias()` raw compiles and passes everything.

**Impact**: test gap; the wrong sign is invisible to `cargo test` and to a casual look.

## Suggested Fix
A source scan (same shape as `the_context_constructor_builds_no_std_hash_map`, via `source_scan::production_text`) asserting `.depth_bias()` appears nowhere in `crates/renderer/src` outside `layer_depth_bias`, or rename the core method so the raw triple is non-obvious.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
