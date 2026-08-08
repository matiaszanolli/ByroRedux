# SF-D8-2026-08-07-02: EmissiveSource::None doc claims a non-zero-emissive condition no producer checks

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2641
**Finding ID**: SF-D8-2026-08-07-02

**Severity**: LOW
**Dimension**: 8 (NIFAL Canonical Material Translation for Starfield)
**Location**: `crates/core/src/ecs/components/material.rs:452-460`
**Status**: NEW

## Description
`EmissiveSource::None`'s doc comment claims a "non-zero emissive" condition
that no producer checks. The doc says materials land in `None` "or where
none of them authored a non-zero emissive"; all three writers
(`dedicated_shader.rs:300,397`, `legacy_properties.rs:149`,
`asset_provider/material.rs:1230`) set their variant unconditionally once
their property class is bound, so a BSLSP with `emissive_multiple = 0.0`
reports `Lighting`, not `None`.

## Evidence
All three `EmissiveSource` writers set their variant unconditionally,
without checking for a non-zero emissive value as the doc claims.

## Impact
Harmless today — `emissive_source` has no consumer anywhere in
`crates/renderer/` — but a trap for the future BSEffect render path #1280
exists to enable.

## Suggested Fix
Fix the doc, or add the `!= 0.0` gate to match the documented contract.

## Completeness Checks
- [ ] **TESTS**: If the gate is added (not just the doc fixed), a zero-emissive fixture asserts `EmissiveSource::None`
