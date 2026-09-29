# UI-D7-2026-09-29-01: `raster::wrap` is quadratic in the string length when `wrapwidth` exceeds the text width, and it runs on every HUD render

**Labels**: medium,bug,ui,performance,safety

**Source report**: `docs/audits/AUDIT_UI_2026-09-29.md`

- **Severity**: MEDIUM
- **Dimension**: MenuXml & HUD Drivers
- **Profile**: MenuXml
- **Location**:
  - `crates/menuxml/src/raster.rs:465-494` (`wrap`)
  - Call chain: `raster.rs:422-458` (`draw_text_item`) ← `crates/menuxml/src/menu.rs:398` (`render_frame`) ←
    `byroredux/src/hud.rs:698` (every changed HUD frame, up to 30 Hz)
  - `wrap_width` comes from `crates/menuxml/src/layout.rs:225`, which has no upper cap
- **Status**: NEW. The #4715 fix bounded the pixel loops; the text path's pre-raster work was outside its scope. No
  matching open or closed issue.
- **Description**:
  - For each space-separated word, `wrap` builds `candidate = format!("{current} {word}")` and measures the whole
    candidate again with `Font::measure_width`, which walks it byte by byte.
  - While the text still fits (a large `wrapwidth`), `current` only grows. A text of n words therefore costs O(n²) in
    copying and measuring.
  - Menu XML is untrusted input (`ui.md`, #4715). A `<string>` literal or a `strings.xml` entity has no length cap,
    and `wrapwidth` accepts any finite value.
  - Wrapped lines are not cached between renders.
- **Evidence**: probe `/tmp/audit/ui/probe_wrap`. It is a release build that calls the public `raster::wrap` and
  `Framebuffer::text_line` with a synthetic font (9-px advance) and `wrapwidth = 1e9`:
  ```
   5000 words ( 15000 B): wrap    22.5 ms -> 1 line; text_line 0.3 ms
  10000 words ( 30000 B): wrap    89.3 ms
  20000 words ( 60000 B): wrap   359.1 ms
  40000 words (120000 B): wrap  1425.8 ms -> 1 line; text_line 1.0 ms
  control wrapwidth=300, 40000 words: 2.2 ms (3077 lines)
  ```
  - Doubling the word count quadruples the time.
  - The clipped glyph draw stays around 1 ms, and the control run shows that the cost is in `wrap`.
- **Impact**:
  - The trigger is one HUD text of about 100 KB with a large `wrapwidth`, in a replaced or modified
    `Oblivion - Misc.bsa` / `Fallout - Misc.bsa`. It stalls the main thread for about 1.4 s on every HUD render.
  - With the camera turning, the compass changes every frame, so the HUD re-renders on every 33 ms cadence tick and
    the game effectively freezes.
  - Vanilla strings are short, and the corpora pass. Only hostile or malformed content triggers this.
  - This is the same class and severity as the parse-side PAR-D1-2026-09-29-02.
- **Related**: #4715 (closed), PAR-D1-2026-09-29-02, #4570 (raster preflight duplication, tech debt)
- **Suggested Fix**: Keep a running width: add `space + measure_width(word)` for each word, and append into one
  `String` instead of calling `format!` again. Pin the fix with a linear-time test (for example, 40k words under a
  small time budget).

**Validated at HEAD 9fcfdc3fc**: `raster::wrap` in `crates/menuxml/src/raster.rs` still builds `format!("{current} {word}")` and re-measures the whole candidate with `font.measure_width` per word; no running width, no cache.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
