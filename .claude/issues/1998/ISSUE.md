# REN-D15-02: Water-plane comment claims RenderLayer::Decal gives depth-bias z-fight protection — pipeline actually disables depth bias

**Filed**: 2026-07-15 · **Source**: `docs/audits/AUDIT_RENDERER_2026-07-15_DIM15.md` (Dimension 15: Water) · **Labels**: `low,renderer,documentation`

## Description

`spawn_water_plane` tags the spawned plane `RenderLayer::Decal` with a comment claiming this "pushes water onto a slightly biased depth ladder ... without z-fighting." Not true in the current pipeline: `water.rs`'s `build_pipeline` sets `.depth_bias_enable(false)`, `WATER_PIPELINE_DYNAMIC_STATES` has no `DEPTH_BIAS`, and the water draw loop in `draw.rs` never calls `cmd_set_depth_bias`. `RenderLayer::Decal` still does real work (draw-order placement via the sort key), but the depth-bias half of the rationale isn't realized.

## Evidence

```rust
// crates/renderer/src/vulkan/water.rs::build_pipeline
.depth_bias_enable(false)
// WATER_PIPELINE_DYNAMIC_STATES has no DEPTH_BIAS
```

## Impact

Low practical risk — water never writes depth, so no true z-fight; at most a thin shoreline comparison-order band. Primarily a documentation-accuracy issue.

## Related

- REN-D15-01 (#1997) — same file, same "shoreline surface quality" checklist area.

## Suggested Fix

Correct the comment to say Decal is used purely for draw-order here, not depth bias. Only add real depth-bias state if a RenderDoc capture shows actual shoreline z-fighting.

## Completeness Checks
- [ ] **SIBLING**: Check other `RenderLayer::Decal` call sites for the same inaccurate depth-bias assumption

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/1998
