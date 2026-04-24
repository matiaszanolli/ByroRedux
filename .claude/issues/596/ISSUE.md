# #596: FO4-DIM2-06: BA2 module docstring claims per-archive compression; code is per-chunk

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/596
**Labels**: documentation, import-pipeline, low, 

---

**From**: `docs/audits/AUDIT_FO4_2026-04-23.md` (Dim 2)
**Severity**: LOW
**Location**: `crates/bsa/src/ba2.rs:1-24`
**History**: Carry-forward of AUDIT_FO4_2026-04-17 L6, never previously filed.

## Description

The module docstring implies a single archive-level compression flag ("Compression is generally zlib for FO4; Starfield v3 uses LZ4"). Reality is two-axis:
- Archive-level **codec** (zlib vs LZ4 block) — determined by v3's `compression_method` field.
- **Per-chunk/per-file on-off** — determined by `packed_size == 0` (raw) vs nonzero (compressed).

## Evidence

`extract_general` at `ba2.rs:428-438` and `extract_dx10` at `ba2.rs:455-464` both branch on `packed_size == 0` independently of `self.compression`.

## Impact

Docs-only. Could confuse a contributor trying to add a new codec.

## Suggested Fix

Add a sentence to the top-of-file doc clarifying "codec is archive-wide; per-chunk on/off via `packed_size != 0`."

## Completeness Checks

- [ ] **TESTS**: n/a (doc-only change)
