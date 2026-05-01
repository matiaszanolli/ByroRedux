# Issue #767: NIF-D1-10 — BsPackedGeomDataCombined Transform field-order bug

**Severity**: CRITICAL · **Domain**: nif-parser, legacy-compat · **Type**: bug
**Source audit**: docs/audits/AUDIT_NIF_2026-04-30.md
**Twin commit**: `8ec6a69` (NiSkinData fix landed same day)

## Summary

`BsPackedGeomDataCombined::parse()` at `crates/nif/src/blocks/extra_data.rs:662` calls `read_ni_transform()` (NiAVObject inline order: Translation→Rotation→Scale) for a field that nif.xml types as `NiTransform` STRUCT (Rotation→Translation→Scale). Same bug class as today's NiSkinData fix.

## Fix

Replace `stream.read_ni_transform()` with `stream.read_ni_transform_struct()` at extra_data.rs:662. Add a value-validation test using non-identity rotation.

## Completeness checks

- [ ] Regression test with non-identity rotation
- [ ] Confirm no other `read_ni_transform()` callers parse a nif.xml `NiTransform` STRUCT field (audited 2026-04-30 — clean)
- [ ] Update `read_ni_transform_struct` doc-comment with this site
