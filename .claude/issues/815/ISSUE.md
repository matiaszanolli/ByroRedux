# #815 — FO4-D4-NEW-03: PKIN FLTR sub-record silently dropped (230/872 vanilla PKINs)

**Severity**: MEDIUM
**Location**: `crates/plugin/src/esm/records/pkin.rs:80-99`
**Source**: `docs/audits/AUDIT_FO4_2026-05-04_DIM4.md`
**Created**: 2026-05-04

## Summary

`parse_pkin` has CNAM/VNAM/FNAM but no FLTR. The sister parser
`parse_scol` already collects FLTR — PKIN diverged. 230/872 vanilla
PKINs ship FLTR (workshop build-mode filter). No render-path impact
today; required for future Workshop UI.

## Sibling check

Mirror `parse_scol`'s FLTR arm exactly (records/scol.rs:158-175).

## How to fix

```
/fix-issue 815
```
