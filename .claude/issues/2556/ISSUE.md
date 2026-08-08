# FNV-D2-03: EmissiveSource::None's doc contradicts Material::default()

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2556
**Finding ID**: FNV-D2-03

**Severity**: LOW
**Dimension**: NIFAL Canonical Translation (FNV slice)
**Location**: `crates/core/src/ecs/components/material.rs:453-458` vs `:359-362`
**Status**: NEW

## Description
`EmissiveSource::None`'s variant doc says `emissive_mult` defaults to 0.0, but `Material::default()` sets it to 1.0. No production impact on the NIF path (translation always overwrites it from `ImportedMaterial`, whose own default is 0.0); bites only direct `Material::default()` call sites (`cornell.rs`, save/load fixtures).

## Evidence
Confirmed directly: `Material::default()` (`material.rs:359-362`) sets `emissive_mult: 1.0`; `EmissiveSource::None`'s doc comment (`material.rs:453-458`) says "No emissive authoring; `emissive_mult` defaulted to 0.0."

## Impact
Documentation-only mismatch; misleading to anyone reading the enum doc as authoritative for `Material::default()`'s actual field values.

## Suggested Fix
Either change `Material::default()`'s `emissive_mult` to 0.0 (verify no call site depends on 1.0), or correct the `EmissiveSource::None` doc comment to say 1.0.

## Completeness Checks
- [ ] **TESTS**: If `Material::default()`'s value changes, confirm no `cornell.rs`/save-fixture call site depends on the old 1.0 default
