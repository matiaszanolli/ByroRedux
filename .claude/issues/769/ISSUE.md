# Issue #769: NIF-DIM2-01 — `until=` boundary uses ≤ instead of < (2 sites)

**Severity**: LOW · **Domain**: nif-parser, legacy-compat · **Type**: bug
**Source audit**: docs/audits/AUDIT_NIF_2026-04-30.md
**Related**: #765 (parent pattern)

## Summary

Two concrete instances of #765's pattern. `until="X"` in nif.xml is exclusive (field absent at version X exactly), but two parsers use `<=`:

- `crates/nif/src/blocks/interpolator.rs:294` — NiTransformData.Order at v10.1.0.0
- `crates/nif/src/blocks/properties.rs:233` — NiTexturingProperty.Apply Mode at v20.1.0.1

## Fix

`<=` → `<` in both sites. Add boundary unit tests.

## Completeness checks

- [ ] Boundary tests for v10.1.0.0 NiTransformData and v20.1.0.1 NiTexturingProperty
- [ ] One-time sweep for any other `<=` boundary that should be `<`
