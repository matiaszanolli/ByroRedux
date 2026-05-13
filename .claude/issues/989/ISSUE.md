# #989 — SK-D6-NEW-01: No .STRINGS companion-file loader

**Source**: `docs/audits/AUDIT_SKYRIM_2026-05-12.md` § Dim 6
**Severity**: LOW (cosmetic — does not block rendering)
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/989

## Locations

- `crates/plugin/src/esm/records/common.rs:105-114` — placeholder emitter
- `crates/plugin/src/esm/reader.rs:557` — `FileHeader.localized`

## Summary

`FileHeader.localized` is captured, and every `read_lstring_or_zstring` call site emits a `<lstring 0xNNNNNNNN>` placeholder. The Phase 2 follow-up (loading `Strings/<plugin>_<lang>.{STRINGS,DLSTRINGS,ILSTRINGS}` and resolving the placeholder) has never been implemented; `grep -rn 'STRINGS|stringstable|StringTable|strings_file' crates/plugin/src/esm/` returns zero hits.

## Fix

Land `crates/plugin/src/esm/strings_table.rs` honouring the UESP STRINGS format (8-byte header + count × (id, offset) + string blob). Surface as an optional `&StringTableSet` parameter on `parse_esm_with_load_order`. ~150 LOC + fixtures.

## Related

- #348 (CLOSED) — Phase 1 placeholder
