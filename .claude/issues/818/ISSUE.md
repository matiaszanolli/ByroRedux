# #818 — FO4-D4-NEW-06: TXST records with DODT-only (3/382 vanilla) silently dropped from texture_sets map

**Severity**: LOW
**Location**: `crates/plugin/src/esm/cell/support.rs:265-267`
**Source**: `docs/audits/AUDIT_FO4_2026-05-04_DIM4.md`
**Created**: 2026-05-04
**Auto-resolves with**: #813 (DODT) + #814 (DNAM)

## Summary

The `if set != TextureSet::default()` guard drops 3 vanilla TXSTs that
have no TX00..TX07/MNAM but DO carry DODT/DNAM. Once #813/#814 land
and `TextureSet` carries those fields, the equality guard naturally
re-admits the records.

## Tracking

Likely never needs an independent fix — close once #813 + #814 land
and live measurement shows `texture_sets >= 382`. Until then, an
interim guard extension is documented in the issue body.

## How to fix

```
/fix-issue 818
```
