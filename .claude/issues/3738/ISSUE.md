# #3738 — TD1-2026-08-30-03: `recreate_screen_passes` has regrown to 700 LOC in the file whose 761-LOC predecessor #1671 already split

**Labels**: bug, renderer, low, tech-debt

---

- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `crates/renderer/src/vulkan/context/resize.rs` — `recreate_screen_passes`
- **Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-30.md` (`TD1-2026-08-30-03`), HEAD `64f64480`

## Description

#1671 (CLOSED) split `recreate_swapchain` at 761 LOC into `recreate_swapchain_core` (now
332 LOC) plus siblings. `recreate_screen_passes` has since grown to **700 LOC** in the
same file — 4 LOC short of what triggered the original split, and the same shape: one
linear rebuild of every screen-sized attachment and its dependent descriptor writes.

This is the identical regrowth pattern `draw_frame` has now repeated three times
(#1052 → #1748 → #1857 → #2197 → #2255 → #3282). Worth noting explicitly: **closing a
function-split issue has not, historically, kept the function split.**

## Suggested Fix

Split per pass group, mirroring the attachment families the function rebuilds —
G-buffer attachments / SVGF + TAA history / composite + bloom chain / upscaler inputs.
Each group is an independent `create → transition → write descriptors` triple.

**Caution — this one IS render-pass adjacent.** It recreates attachments and rewrites
descriptor sets, so per `feedback_speculative_vulkan_fixes.md`: do **not** change
layout-transition order or barrier placement while moving code, and validate under
`BYRO_VALIDATION=1` rather than on `cargo test` alone. Use the `sed`-extract method and
diff-check (`cargo fmt` reformats the whole crate). Effort: medium.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (`recreate_swapchain_core` and its siblings)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
