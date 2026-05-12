# #956 — REN-D5-NEW-05: debug_assert! panics inside active recording

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-11_DIM5.md`
**Dimension**: Command Recording
**Severity**: LOW
**Confidence**: MED
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/956

## Locations

- `crates/renderer/src/vulkan/context/draw.rs:1154-1160` — the `debug_assert!`
- `crates/renderer/src/vulkan/context/draw.rs:245` — `begin_command_buffer`
- `crates/renderer/src/vulkan/context/draw.rs:2067` — `end_command_buffer`

## Summary

`debug_assert!(gpu_instances.len() <= 0x7FFF, ...)` fires between `begin_command_buffer` and `end_command_buffer`. A debug-build panic here leaks the command buffer in pending state with no matching `end_command_buffer` on the unwind path. Process aborts shortly after, so practically harmless, but the dev-only canary itself is unsafe inside a recording.

Reachable on Skyrim / FO4 dense city cells (~50K REFRs) when streaming radius widens. The R16→R32 mesh_id upgrade prescribed inline is the proper fix; this is the safety net until that lands.

## Fix (preferred)

Promote to recoverable: replace `debug_assert!` with `if gpu_instances.len() > 0x7FFF { log::error!(\"...\"); /* truncate or skip-draw */ }`. Silent panics on dense cells in debug builds aren't useful; a logged error + draw skip survives the frame.

## Tests

Optional — construct 32 768 mock GpuInstances and assert recoverable path engages without panic. Skip if no test stub for `draw_frame` slice.
