# #5072 — CONC-D6-2026-09-29-01: A swapchain-format-change rebuild of `EguiPass` drops the egui font atlas, and egui never re-sends it

**Labels**: low,renderer,vulkan,bug

**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-09-29.md`
**Severity**: LOW
**Dimension**: Resource Lifecycle (swapchain recreate)

## Location
- `crates/renderer/src/vulkan/context/resize.rs` — the #2475 format-change arm (`pass.destroy` + `EguiPass::new`)
- `crates/renderer/src/vulkan/egui_pass.rs` — `promote_partial_deltas`, `image_mirrors`

## Description
The rebuilt pass has a fresh `egui_ash_renderer::Renderer` (empty `managed_textures`/`textures`) and an empty `image_mirrors`. The app's `egui::Context` is not reset, so egui believes the font atlas is resident and only sends partial deltas for new glyphs: (1) a partial delta arrives and, with no mirror entry, `promote_partial_deltas` passes it through unchanged; (2) the crate's `set_textures` returns `BadTexture` for the unknown id (`egui-ash-renderer-0.11.0/src/renderer/mod.rs:351`); (3) every `cmd_draw` sampling the atlas also returns `BadTexture` (`:589`). The overlay is dead for the session with an error every frame.

## Evidence
The resize arm constructs `EguiPass::new(...)` with no hand-over of textures or mirrors; `EguiPass::new` initialises `image_mirrors: FxHashMap::default()`. Dates from the #2475 full rebuild (`fd8f67e2a`), not caused by #4986.

## Impact
Only on a surface-format change (HDR toggle or display move). Debug overlay and native pause/inventory/dialogue pages (all egui) render nothing after the flip. Not visible to `cargo test`.

## Related
#2475, #2685, #4986 (whose mirror holds exactly the data a re-seed needs), REN-D5-2026-09-29-01.

## Suggested Fix
Take `image_mirrors` out of the old pass before `destroy`, then replay each mirror as a full delta into the rebuilt pass; alternatively have egui re-send its textures after a rebuild. Add a unit test that the rebuild re-seeds managed textures.

Validated at HEAD 9fcfdc3fc: format-change arm in `resize.rs` still calls `pass.destroy` then `EguiPass::new` with no mirror hand-over; `EguiPass::new` sets `image_mirrors: FxHashMap::default()`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
