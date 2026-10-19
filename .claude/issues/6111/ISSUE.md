# REN-SD-2026-10-19-05: A texture newly queued by `SwapTexture` is not flushed until some unrelated upload batch runs

Labels: low,renderer,memory,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6111

**Severity**: LOW (visible checkerboard until the next flush; not verified at runtime) · **Dimension**: Memory/Lifecycle

## Location
`byroredux/src/app_step.rs` `step_texture_swaps` (no flush); flush callers are `flush_pending_cell_textures*` (cell apply), `flush_pending_lod_textures` (only when a LOD attempt ran), and the NPC loaders (`loot_appearance.rs`).

## Description
`apply_texture_swap` goes through `resolve_texture`, which on a miss reserves a bindless slot (descriptor = fallback) and queues the DDS. The frame renderer does not drain `pending_dds_uploads` (the app only reads the count for the loading-cover decision). In an interior, or an exterior with no LOD or cell work pending, the next flush can be arbitrarily far away.

## Evidence
`step_texture_swaps` (read at publish time) has no flush call; `grep -rn flush_pending_uploads byroredux crates` shows no caller from `step_texture_swaps`, `draw_frame` or the idle path.

## Impact
The swapped surface shows the fallback texture until some other batch flushes the shared queue. Unverified at runtime (no engine launch).

## Related
REN-SD-2026-10-19-04 (same command).

## Suggested Fix
Call the existing `flush_pending_cell_textures` helper at the end of `step_texture_swaps` when it applied anything.

Source: docs/audits/AUDIT_RENDERER_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
