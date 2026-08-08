# TD8-002: byroredux-debug-ui declares 3 dependencies with zero source references, incl. the entire Vulkan renderer crate

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2427
**Finding ID**: TD8-002 (source: `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 8 — Dead Code & Backwards-Compat Cruft
**Location**: `crates/debug-ui/Cargo.toml:8` (`byroredux-renderer`), `:11` (`egui-ash-renderer`), `:13` (`anyhow`)
**Status**: NEW

## Description
`byroredux-debug-ui`'s `Cargo.toml` declares the *entire Vulkan renderer crate* (and its transitive `ash`/`gpu-allocator`/`rspirv`/`fsr3-sys`) plus `egui-ash-renderer` and `anyhow` with zero source references. This is real build-graph bloat and contradicts the crate's own module doc ("the renderer stays a pure-GPU layer"). Biggest single build-graph win in this audit cycle.

## Suggested Fix
Delete all three lines; confirm with `cargo check`.

## Age
~2.5 months — survived a later unrelated cleanup pass on the same file.

## Completeness Checks
- [ ] **TESTS**: `cargo check` (and full workspace build) passes after removing all three lines
