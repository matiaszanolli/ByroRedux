# #812 — FO4-D2-NEW-02: Zlib decompressed-size mismatch is logged at debug while LZ4 hard-errors

**Severity**: LOW
**Location**: `crates/bsa/src/ba2.rs:466-472`
**Source audit**: `docs/audits/AUDIT_FO4_2026-05-04_DIM2.md`
**Created**: 2026-05-04
**Related**: open #598 (Meshes.ba2 packed/unpacked ratio anomaly); closed #622 (BSA-side sibling fix that explicitly punted BA2 to a later audit).

## Summary

Asymmetric strictness in `decompress_chunk`: zlib emits `log::debug!` and
returns the (possibly short) buffer; LZ4-block hard-errors on the same
size mismatch. Hides truncated/malformed archives from downstream consumers.

## Recommended sequencing

1. Investigate #598 first (the Meshes.ba2 ratio anomaly is the only
   real-data source of size mismatches we know of — explain it before
   tightening the default behaviour).
2. Promote `log::debug!` → `log::warn!` immediately (lower-effort visibility fix).
3. Add `Ba2Strictness::{Lenient, Strict}` toggle once #598 is resolved or
   determined benign.

## Sibling check

#622 (SK-D2-04) tightened the equivalent BSA-side post-LZ4 length assertion.
Apply the same discipline here.

## How to fix

```
/fix-issue 812
```
