## Finding REN2-14 — Renderer Audit 2026-06-11

- **Severity**: LOW
- **Dimension**: Material Table (doc-rot)
- **Location**: `crates/renderer/src/vulkan/material.rs:1033,1051-1052`; actual constant at `crates/renderer/src/vulkan/scene_buffer/constants.rs:182`
- **Status**: NEW. Validated CONFIRMED at HEAD `1e8a25ab`.

## Description

The `MaterialTable::intern` doc-comment claims "well under the 4096 cap (scene_buffer.rs:60-62)" (`material.rs:1051-1052`) and references pre-split `scene_buffer.rs:N` line numbers (also at `:1033`). The actual cap is `MAX_MATERIALS: usize = 16384` (`scene_buffer/constants.rs:182`, raised in `7823eb59`), and `scene_buffer.rs` has been a directory (`scene_buffer/`) since the Session 34/35 splits.

## Suggested Fix

Update the doc to the real constant (reference `MAX_MATERIALS` by name, not value/line) and the post-split path. Fold into the doc-rot pass with #1484.

## Completeness Checks
- [ ] **SIBLING**: Grep for other `scene_buffer.rs:` line references and stale `4096` cap mentions
- [ ] **TESTS**: N/A (doc-only); prefer name-references over literals so it can't rot again

---
Source: `docs/audits/AUDIT_RENDERER_2026-06-11.md` · Filed by `/audit-publish`
