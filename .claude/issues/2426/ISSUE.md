# TD8-001: thiserror declared as a direct dependency but never referenced in 4 crates (bsa, spt, papyrus, nif)

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2426
**Finding ID**: TD8-001 (source: `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 8 — Dead Code & Backwards-Compat Cruft
**Location**: `crates/bsa/Cargo.toml:8`, `crates/spt/Cargo.toml:18`, `crates/papyrus/Cargo.toml:8`, `crates/nif/Cargo.toml:9`
**Status**: NEW

## Description
`thiserror` is declared as a direct dependency but never referenced in 4 crates. `cargo machete` flags all four; verified by hand — none use `thiserror::`/`#[derive(Error)]` anywhere. Each hand-rolls its error type instead.

## Suggested Fix
Remove the dependency line from each; confirm with `cargo check`.

## Completeness Checks
- [ ] **TESTS**: `cargo check` (and full workspace build) passes after removing all four lines
