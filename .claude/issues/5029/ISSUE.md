# UI-D5-2026-09-29-01: two delta commits spliced a new doc comment into an existing one, so `shared_descriptors` and `MenuRenderer::render_frame` lost their docs to the neighbouring item

**Labels**: low,documentation,doc-rot,ui

**Source report**: `docs/audits/AUDIT_UI_2026-09-29.md`

- **Severity**: LOW
- **Dimension**: Render & Overlay Upload
- **Profile**: both (the `player.rs` site) and MenuXml (the `menu.rs` site)
- **Location**: `crates/ui/src/player.rs:71-118` (df25ceaeb, #4595); `crates/menuxml/src/menu.rs:345-354`
  (66471d455, #4608)
- **Status**: NEW
- **Description**:
  - **`player.rs`**:
    - The #2733 block starting "Process-wide Ruffle GPU bundle…" runs straight into "#4595 — true when a Vulkan wgpu
      adapter exists…". It covers the sharing model, the trade-off, why a failure is not cached, and why the first
      creation is serialised.
    - The combined block documents `#[cfg(test)] fn vulkan_adapter_available`.
    - `shared_descriptors` (`:118`) has no doc.
  - **`menu.rs`**:
    - "Evaluate, lay out, and rasterize one frame. Returns the RGBA pixels…" now opens the doc for `frame_pixels`.
    - `render_frame` (`:354`) has no doc.
- **Impact**: documentation only. The device-lifetime and raster contracts show up on the wrong items in rustdoc and
  IDE hovers.
- **Related**: #4892 (other stale UI docs)
- **Suggested Fix**: Move `player.rs:71-93` back above `fn shared_descriptors`, and move `menu.rs:345-346` back above
  `fn render_frame`.

**Validated at HEAD 9fcfdc3fc**: in `crates/ui/src/player.rs` the #2733 "Process-wide Ruffle GPU bundle…" doc runs straight into the #4595 doc above `#[cfg(test)] fn vulkan_adapter_available`, leaving `fn shared_descriptors` undocumented; in `crates/menuxml/src/menu.rs` "Evaluate, lay out, and rasterize one frame…" sits above `frame_pixels` and `render_frame` has no doc.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
