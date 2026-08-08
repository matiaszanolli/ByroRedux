# TD8-006: crates/debug-ui/src/lib.rs's pub use egui; pub use egui_winit; have zero downstream consumers

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2431
**Finding ID**: TD8-006 (source: `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 8 — Dead Code & Backwards-Compat Cruft
**Location**: `crates/debug-ui/src/lib.rs` (final two lines: `pub use egui; pub use egui_winit;`)
**Status**: NEW

## Description
Stated justification ("so the binary doesn't have to add a direct dep") never materialized — `byroredux/Cargo.toml` never added `egui`/`egui-winit` directly, and `main.rs` never routes through this re-export. `egui_pass.rs` declares its own direct `egui` dep instead. Same class as closed #1324 but a different, still-live pair (that fix targeted `pub` functions, not this type re-export).

## Suggested Fix
Delete both lines; re-add with an actual call site if ever needed.

## Age
~2.5 months.

## Completeness Checks
- [ ] **TESTS**: `cargo check` passes after removing both re-export lines
