# #955 — REN-D5-NEW-04: redundant pre-loop depth-bias / depth-compare-op

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-11_DIM5.md`
**Dimension**: Command Recording
**Severity**: LOW
**Confidence**: HIGH
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/955

## Locations

- `crates/renderer/src/vulkan/context/draw.rs:1508-1516` — pre-loop unconditional sets
- `crates/renderer/src/vulkan/context/draw.rs:1471` — `last_render_layer: Option<…> = None`
- `crates/renderer/src/vulkan/context/draw.rs:1481` — `last_z_function: u8 = u8::MAX`
- `crates/renderer/src/vulkan/context/draw.rs:1598-1603, 1618-1622` — per-batch helpers

## Summary

Pre-loop `cmd_set_depth_bias(0,0,0)` and `cmd_set_depth_compare_op(LESS_OR_EQUAL)` are strictly dominated by the per-batch coalescing helpers, whose sentinel-initialised trackers (`Option::None`, `u8::MAX`) guarantee a fire on the first batch. Sibling of closed #912 (cull-mode pre-loop redundancy).

The other two pre-loop sets (`depth_test_enable(true)`, `depth_write_enable(true)`) interact with non-sentinel `true` initialisation and aren't strictly redundant — flipping them to `Option<bool> = None` is a noted optional second step.

## Fix (preferred)

Drop the two redundant pre-loop calls. Per-batch helpers cover Vulkan's "must be set before first draw" requirement via their sentinel-comparison gates.

## Tests

No regression test — visual output is byte-identical.
