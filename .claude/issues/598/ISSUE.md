# #598: FO4-DIM2-08: BA2 Meshes.ba2 packed/unpacked ratio anomaly uninvestigated

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/598
**Labels**: import-pipeline, low, 

---

**From**: `docs/audits/AUDIT_FO4_2026-04-23.md` (Dim 2)
**Severity**: LOW
**Location**: `crates/bsa/src/ba2.rs` (parse path)
**History**: Carry-forward of AUDIT_FO4_2026-04-17 L5, never previously filed.

## Description

Prior sweep found some GNRL records in `Fallout4 - Meshes.ba2` where `packed_size / unpacked_size ≈ 3.0` — a ratio impossible for well-formed zlib deflate (zlib never inflates data by 3×; worst case on uncompressible input is ~0.1% overhead). Zero extraction failures observed, but the math is nonsense, suggesting either (a) a layout quirk on tiny files where `packed_size` is padded to a block alignment, or (b) a reader mis-interpretation. Not investigated in prior audit.

## Evidence

Prior-audit measurement on vanilla `Fallout4 - Meshes.ba2` (session-7 sweep). Exact files not recorded in the audit summary.

## Impact

Unknown. Could indicate a parser bug masked by zlib's resilience (zlib can decode a prefix and stop at the first end-of-stream marker). Worth a dedicated investigation to either confirm benign alignment or uncover a subtle bug.

## Suggested Fix

Write `crates/bsa/examples/ba2_ratio_anomaly.rs` that scans all vanilla BA2s, logs every record where `packed > unpacked`, groups by extension + archive. Validate against xEdit's BA2Explorer for the same entries.

## Completeness Checks

- [ ] **UNSAFE**: n/a
- [ ] **SIBLING**: Same scan applied to BSA v103/v104/v105 via `archive.rs`
- [ ] **DROP**: n/a
- [ ] **LOCK_ORDER**: n/a
- [ ] **FFI**: n/a
- [ ] **TESTS**: Lock the finding as a test only after root cause is confirmed.
