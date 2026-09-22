# ESM-2026-09-21-D1-03: compressed-record inflation ceiling calibrated on 3 masters — 82 vanilla Starfield SFTR records (up to 786:1) rejected as corrupt/hostile

**Issue**: #4640
**Filed**: 2026-09-22 (audit-publish, AUDIT_ESM_2026-09-21.md)

**Severity**: MEDIUM
**Dimension**: Header & GRUP Walk (real-data evidence from Dim 8)
**Game Affected**: Starfield
**Location**: `crates/plugin/src/esm/reader.rs:34-67` (`MAX_RECORD_INFLATION_RATIO = 512` + census doc), `:784-790` (rejection), `:2085-2105` (`inflation_ceiling_clears_every_observed_vanilla_shape`)

## Description
The ceiling's calibration doc cites a 4-master census (33,179 in FalloutNV, 44,153 in Skyrim, 56,399 in Fallout4, 0 in Oblivion "which compresses nothing"), worst ratio 102.1:1. Real data over all seven masters contradicts the Oblivion premise (41,789 compressed records there) and finds 82 vanilla Starfield `SFTR` records exceeding the ceiling.

## Evidence
- `Oblivion.esm`: 41,789 compressed (worst 101:1). `Fallout3.esm`: 41,903 (75:1). `SeventySix.esm`: 74,834 (19:1). `Starfield.esm`: 91,149, of which 82 (`SFTR` group) exceed the ceiling — e.g. `0x00167A07` 834→655,482 bytes (786:1), `0x000478BB` 846→655,493 against a 433,152 ceiling.
- `esm_dim8_coverage`'s SFTR census stops at the first ceiling error.

## Impact
Latent today — `SFTR` isn't dispatched so `skip_group()` never inflates it. The first SFTR decoder (already on the Starfield audit's next-target list), or any all-sub-record walker, would hit a hard `Err` that `?`-propagates out of `parse_esm_with_load_order`; `load_order.rs` catches it and merges `EsmIndex::default()` — silently dropping all of `Starfield.esm`.

## Suggested Fix
Re-census all seven masters, re-derive the bound so 786:1 clears with margin. Correct doc/test numbers and pin the SFTR shape. Consider a per-record skip+warning instead of a whole-parse `Err`.

## Related
#3399, #3410 (closed); `byroredux_bsa::safety::inflate_bounded` parity should be re-checked against the same data.

## Source
docs/audits/AUDIT_ESM_2026-09-21.md (ESM-2026-09-21-D1-03)
