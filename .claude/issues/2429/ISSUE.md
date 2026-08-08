# TD8-004: byroredux-platform declares byroredux-core as a dependency with zero references

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2429
**Finding ID**: TD8-004 (source: `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 8 — Dead Code & Backwards-Compat Cruft
**Location**: `crates/platform/Cargo.toml`
**Status**: NEW

## Description
`byroredux-platform` declares `byroredux-core` as a dependency with zero references. The crate's own module doc describes itself as a deliberately small, dependency-light `winit` wrapper — consistent with the empty grep.

## Suggested Fix
Remove the line; confirm with `cargo check`.

## Completeness Checks
- [ ] **TESTS**: `cargo check` (and full workspace build) passes after removal
