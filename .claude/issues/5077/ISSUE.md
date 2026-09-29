# #5077 — ESM-2026-09-29-D1-01: reader.rs constant comments were left on the pre-#4640 census, and #4643 missed one "the real 266.0" pin

**Labels**: low, documentation, doc-rot, esm-plugin

**Source**: `docs/audits/AUDIT_ESM_2026-09-29.md` — finding `ESM-2026-09-29-D1-01`

**Severity**: LOW

**Dimension**: Header Detection & GRUP Walk (doc-rot)

**Record / Sub-record**: — (compressed records; TES4 `HEDR`)

**Location**: `crates/plugin/src/esm/reader.rs:67-83` (`MIN_RECORD_INFLATED_CEILING`, `MAX_RECORD_INFLATED_BYTES`); `crates/plugin/src/esm/reader.rs:261`

**Status in report**: NEW. These are residuals of the #4640 and #4643 fixes, both closed.

## Description

#4640 replaced the ratio census with a seven-master table but left the two sibling constants' docs on the old data:
- **`MAX_RECORD_INFLATED_BYTES`** still says "the largest real inflated record across the same four masters is 225 433 bytes (`Fallout4.esm`), which this clears by ~290×". The Starfield SFTR `0x00167A07` in the new table alone inflates to 655,482 bytes, so the real headroom is about 100×.
- **`MIN_RECORD_INFLATED_CEILING`** still says "the worst observed ratios all come from ~60–85 byte `LAND` records". The worst is now an 834-byte SFTR.
- **The FO76 band arm** (#4643 residual) still comments "Floor deliberately far below the real 266.0 (#3405)", while #4643 unpinned every other occurrence of 266.0 in the code, the docs and the skill. The installed masters read 279.0.

## Impact

None at runtime; the bounds are correct. The docs are the evidence the next recalibration reasons from.

## Related

#4640, #4643 (closed).

## Suggested Fix

Point both constant docs at the seven-master table: largest inflated 655,482 bytes (Starfield SFTR), about 100× headroom under 64 MiB, worst ratio from an 834-byte SFTR. Change line 261 to "far below every sampled FO76 value (68.0 → 266.0 → 279.0)".

Validated at HEAD 9fcfdc3fc: `crates/plugin/src/esm/reader.rs` still says "~60–85 byte `LAND` records" and "225 433 bytes (`Fallout4.esm`)" in the `MIN_RECORD_INFLATED_CEILING` / `MAX_RECORD_INFLATED_BYTES` docs, and "Floor deliberately far below the real 266.0 (#3405)" in the FO76 band arm; a test comment also still cites "225 433 bytes (FO4)".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the test-side comment that also quotes 225 433 bytes)
