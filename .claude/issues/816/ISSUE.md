# #816 — FO4-D4-NEW-04: SCOL FULL display name silently dropped (124/2617 vanilla SCOLs)

**Severity**: LOW
**Location**: `crates/plugin/src/esm/records/scol.rs:111-198`
**Source**: `docs/audits/AUDIT_FO4_2026-05-04_DIM4.md`
**Created**: 2026-05-04

## Summary

`parse_scol` walks EDID/MODL/ONAM/DATA/FLTR but no FULL arm.
124/2617 vanilla SCOLs ship FULL. No render impact; consistency gap
with every other display-name-carrying record. Update doc-comment at
`scol.rs:107-110` to remove FULL from the "ignored" list.

## How to fix

```
/fix-issue 816
```
