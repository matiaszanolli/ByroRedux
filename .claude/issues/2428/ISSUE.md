# TD8-003: byroredux-ui declares byroredux-core and ruffle_render as unused dependencies

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2428
**Finding ID**: TD8-003 (source: `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 8 — Dead Code & Backwards-Compat Cruft
**Location**: `crates/ui/Cargo.toml`
**Status**: NEW

## Description
`byroredux-ui` declares `byroredux-core` and `ruffle_render` as dependencies with zero references — both confirmed via grep. A third flagged dep, `image`, is a softer call: it's reachable transitively via `ruffle_render_wgpu`'s inherent methods, so the explicit pin may be intentional; not recommended for deletion without a decision.

## Suggested Fix
Remove `byroredux-core` and `ruffle_render` (not `ruffle_render_wgpu`); leave `image` pending a version-pin decision.

## Completeness Checks
- [ ] **TESTS**: `cargo check` passes after removing the two confirmed-unused lines
