# TD8-005: byroredux-renderer declares byroredux-platform and winit with zero references

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2430
**Finding ID**: TD8-005 (source: `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 8 — Dead Code & Backwards-Compat Cruft
**Location**: `crates/renderer/Cargo.toml`
**Status**: NEW

## Description
`byroredux-renderer` declares `byroredux-platform` and `winit` with zero references. The renderer talks to the window purely through `raw-window-handle` trait objects, never needing concrete `winit`/`platform` types.

## Suggested Fix
Remove both lines; run a full workspace build afterward given this crate's many downstream consumers.

## Completeness Checks
- [ ] **TESTS**: Full workspace build passes after removing both lines (this crate is depended on widely — verify no transitive breakage)
