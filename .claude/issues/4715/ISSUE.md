# UI-D7-2026-09-21-01: MenuXml raster loop bounds are never clamped to the framebuffer; a huge or inf extent stalls or hangs every HUD render

**Issue**: #4715
**Severity**: HIGH
**Labels**: high,ui,bug,safety

## Description
`Framebuffer::blit` (`crates/menuxml/src/raster.rs`) computes its pixel-loop bounds from the authored tile rect (`dst`/`clip`), never clamps them to the framebuffer's own `width`/`height`. Per-pixel bounds checking only happens inside `blend` (called from `blend_texel`), i.e. once per iteration of an already-huge loop — the loop itself is never shrunk. `blit_sub` (the glyph path) has the same shape.

Feeding this: `layout.rs`'s `walk_children` only clamps `width`/`height` with `.max(0.0)` (no upper bound), and `parse.rs`'s `literal_from_text` parses trait text with plain `str::parse::<f32>()`, which accepts `inf`, `-inf`, `NaN`, and out-of-range literals like `1e39` (which parses to `f32::INFINITY`). `(inf).ceil() as i64` saturates to `i64::MAX` under Rust's float→int cast semantics.

## Evidence
Verified at HEAD `ee6d3fb39`:
- `crates/menuxml/src/raster.rs:178-181` (in `blit`):
  ```rust
  let x0 = (dst.x.max(clip.x)).floor() as i64;
  let y0 = (dst.y.max(clip.y)).floor() as i64;
  let x1 = ((dst.x + draw_w).min(clip.x + clip.w)).ceil() as i64;
  let y1 = ((dst.y + draw_h).min(clip.y + clip.h)).ceil() as i64;
  ```
  `clip` defaults to the tile's own `dst` rect (`raster.rs:165-172`) whenever no `<clipwindow>` is in scope (the root clip is `None` per `layout.rs`), so `draw_w`/`draw_h` — taken straight from authored `width`/`height`/`zoom` — set the loop bound directly. The only bounds check downstream is per-pixel, inside `blend` (`raster.rs:49-52`).
- `crates/menuxml/src/parse.rs:281`: `literal_from_text` — `trimmed.parse::<f32>()`, no finite-range validation.
- `blit_sub` (`raster.rs:295-298`) mirrors the same unclamped `x0..x1`/`y0..y1` computation for the glyph path.
- A `<image><width>16000</width><height>16000</height><zoom>&scale;</zoom>` tile turns into a 16000×16000 nested-loop raster (256M iterations) on every HUD render; an `inf`/`1e39` extent turns it into a loop bound of `i64::MAX`, i.e. an effective hang.

## Impact
A crafted or replaced Misc BSA (the surface `PAR-D1-2026-09-21-04/05` already exercise for the parser's own load-time explosion) reaches the render path too: a large-but-finite extent stalls the main thread on every HUD refresh (up to ~30 Hz); a non-finite extent is a permanent hang from the first render after `--hud`/`--menu` launch on Oblivion/FO3/FNV. Vanilla content is unaffected — the corpus renders at normal cost — but any future loose-file UI mod path (DarNified, MTUI) would inherit this immediately.

## Related
- PAR-D1-2026-09-21-04 (issue #4650) — same untrusted-XML surface, the load-time include-explosion sibling of this render-time one.
- UI-D7-2026-09-21-02 — the companion render-path panic in the same `layout.rs`/`parse.rs` pair.
- #4570 (closed) — the `blit`/`text_line` preflight dedup; did not add a bounds clamp.

## Suggested Fix
Clamp `x0..x1`/`y0..y1` to `[0, width) × [0, height)` (the framebuffer itself as the outermost clip) before iterating, in both `blit` and `blit_sub`. Map non-finite layout values (`width`/`height`/`zoom`/`depth`/etc.) to 0 or a sane ceiling at the point `literal_from_text` or the layout reader consumes them. Add a regression test that renders the 16000² case and asserts the `blend`/`blend_texel` call count stays bounded by the framebuffer's pixel count.

## Completeness Checks
- [ ] **SIBLING**: `blit_sub` (glyph path) gets the same clamp as `blit`, not just the stretched-image path
- [ ] **TESTS**: A regression test pins the clamped loop bound (call-count assertion) for both a huge finite extent and a non-finite (`inf`/`NaN`) one

Source: docs/audits/AUDIT_UI_2026-09-21.md (UI-D7-2026-09-21-01)
