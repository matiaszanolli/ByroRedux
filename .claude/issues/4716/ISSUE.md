# UI-D7-2026-09-21-02: layout's depth sort panics on a NaN depth that menu XML can author directly or compute

**Issue**: #4716
**Severity**: HIGH
**Labels**: high,ui,bug,safety

## Description
`walk_children` in `crates/menuxml/src/layout.rs` sorts each tile's children by depth with:
```rust
depth.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
```
This is not a total order once any `depth` is `NaN` (`NaN.partial_cmp(x)` is `None` for every `x`, including itself, so the `unwrap_or(Equal)` fallback produces an inconsistent relation — `NaN` compares `Equal` to everything while non-`NaN` values retain their real order, breaking transitivity). Since Rust 1.81, `slice::sort_by`'s implementation detects such total-order violations at runtime and panics with "user-provided comparison function does not correctly implement a total order". This workspace builds with rustc 1.96.

A `NaN` depth is directly authorable: `<depth>NaN</depth>` parses successfully through `literal_from_text` (`parse.rs:281`, plain `str::parse::<f32>()`, which accepts `"nan"`/`"NaN"`/`"NAN"`), or can arise from arithmetic such as `inf * 0` in a trait expression.

## Evidence
Verified at HEAD `ee6d3fb39`:
- `crates/menuxml/src/layout.rs` (`walk_children`), the `depth.sort_by(...)` call using `partial_cmp(...).unwrap_or(Ordering::Equal)` — present unchanged.
- `crates/menuxml/src/parse.rs:281` — `literal_from_text` has no rejection or normalization of non-finite text.
- This is the exact, well-documented Rust 1.81+ `sort_by`/`sort_unstable_by` panic class for comparators that are not total orders (glidesort/driftsort's runtime consistency check), triggered here by any NaN depth among siblings.

## Impact
Engine panic on the first HUD render of a menu with a `NaN` depth among any tile's children — on the main thread, with no `catch_unwind` around the render path. The trigger is the same untrusted-menu-XML surface as `PAR-D1-2026-09-21-05` (issue #4651) and `PAR-D2-2026-09-21-03` (issue #4652), but those cover the *parse*/*load* paths; this is the render path, which neither of them protects.

## Related
- PAR-D1-2026-09-21-05 (issue #4651), PAR-D2-2026-09-21-03 (issue #4652) — same untrusted-content surface, different (load-time) code paths.
- UI-D7-2026-09-21-01 — the companion render-path DoS in the same file pair.

## Suggested Fix
Sort with `f32::total_cmp` (a real total order, including NaN placement) instead of `partial_cmp().unwrap_or(Equal)`; or normalize a non-finite `depth` to 0 at the point it is read from `EvalState`/`literal_from_text`. Add a unit test with ~40 sibling tiles, every third one `<depth>NaN</depth>`, asserting `render_frame` (or `build_draw_list`) completes without panicking.

## Completeness Checks
- [ ] **TESTS**: A regression test with NaN-depth siblings pins the fix (no panic, and depth order for the non-NaN siblings is unaffected)
- [ ] **SIBLING**: Any other `partial_cmp(...).unwrap_or(...)` sort in `crates/menuxml` gets the same audit (grep for the pattern)

Source: docs/audits/AUDIT_UI_2026-09-21.md (UI-D7-2026-09-21-02)
