# UI-D5-2026-09-21-02: the always-on Scaleform --hud re-renders and reads back the full Ruffle target at the movie's frame rate even when nothing changed

**Issue**: #4717
**Severity**: MEDIUM
**Labels**: medium,ui,bug,performance,renderer

## Description
`SwfPlayer::render` (`crates/ui/src/player.rs`) gates the Ruffle render + GPU readback on a single `dirty` flag, and `dirty` is OR'd every `tick()` with `needs_render()` — a flag Ruffle sets whenever a frame ran or the mouse state changed, essentially every tick for a live movie at its native frame rate. `tick_ui_overlay` (`byroredux/src/app_frame.rs`) calls `ui.tick(dt)` and then `ui.render()` unconditionally on every engine frame the overlay is live, with no cadence cap of its own — the only cadence cap in the codebase (`HUD_REFRESH_INTERVAL`, 33 ms) belongs to the separate MenuXml HUD driver, not this path.

So while #2719 correctly gated the *upload* (`update_rgba`) on the post-render pixel comparison (`changed = rgba != self.pixel_buffer`), the render itself — `player.render()` (submits Ruffle's wgpu command list) plus `capture_frame()` (blocking `map_async` + `poll(Wait)`, a full-target row copy, and `unmultiply_alpha_rgba`) — still runs on essentially every tick, whether or not the picture actually changed.

## Evidence
Verified at HEAD `ee6d3fb39`:
- `crates/ui/src/player.rs` `tick()`: `self.dirty |= needs_render;` (comment above it documents the #2719 upload-side fix, but the render-side call below still runs whenever `dirty` is true).
- `crates/ui/src/player.rs` `render()`: `if !self.dirty { return None; }` — the only gate — followed unconditionally by `player.render()` and `capture_frame()`.
- `byroredux/src/app_frame.rs` `tick_ui_overlay`: `ui.tick(dt); ... match ui.render() { ... }` called every invocation, once per engine frame the overlay is live, with no rate limit ahead of it.
- The report's probe (`/tmp/audit/ui/hudprobe`) measured, at 1920×1080: FO4 `hudmenu.swf` (30 fps) — 150 render+readback passes in 5 s, 0 fresh uploads, 5.08 ms mean per pass (max 11.7 ms); Skyrim SE `hudmenu.swf` (24 fps) — 120 passes, 8.26 ms mean (max 16.4 ms).

## Impact
A periodic ~5-8 ms main-thread stall ahead of `draw_frame`, 24-30 times a second, for as long as `--hud` (Scaleform route) is on — steady-state gameplay cost on Skyrim and FO4, scaling with output resolution. Before `62fc0bf22` this was only a modal-menu cost; the always-on HUD route made it continuous.

## Related
- #3429 (open) — the `update_rgba` reallocation-and-fence half of the same call chain; this finding is the render-and-readback half that runs even when #3429's upload does not.

## Suggested Fix
Cap the Scaleform HUD's `render()`/readback at a fixed cadence (mirroring `HUD_REFRESH_INTERVAL`), and skip it entirely when no push, input, or Ruffle stage invalidation occurred since the last render. Longer term, replace the CPU `capture_frame` readback with an exported shared image (external memory) so the composite doesn't round-trip through host memory at all.

## Completeness Checks
- [ ] **TESTS**: A regression test/bench pins render+readback call count against a cadence cap for a static movie (no push, no input)

Source: docs/audits/AUDIT_UI_2026-09-21.md (UI-D5-2026-09-21-02)
