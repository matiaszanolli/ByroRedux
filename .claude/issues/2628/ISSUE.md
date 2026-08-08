# SF-D1-02: pitch_or_linear_size_for has no arm for DXGI 10/11/31, 78 vanilla textures get invalid dwPitchOrLinearSize

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2628
**Finding ID**: SF-D1-02

**Severity**: LOW
**Dimension**: 1 (BA2 v2/v3 LZ4 Block Decompression)
**Location**: `crates/bsa/src/ba2.rs:952-1002`
**Status**: NEW — same defect class as #594/FO4-DIM2-03 (CLOSED), on formats that fix never enumerated

## Description
`pitch_or_linear_size_for` has no arm for DXGI 10/11/31 — 78 vanilla
textures get an invalid `dwPitchOrLinearSize`. `dxgi_format` histogram
across all 137,383 DX10 records shows fmt 31 (`R8G8B8A8_SNORM`, 63 records
— 62 chargen face normal maps), fmt 10 (`R16G16B16A16_FLOAT`, 13 records —
12 cubemaps + the LTC area-light LUT), fmt 11 (`R16G16B16A16_UNORM`, 2
records — gas-giant gradient textures) all fall to the legacy
`(total_bytes, DDSD_LINEARSIZE)` branch instead of the correct
`DDSD_PITCH` form.

## Evidence
Full-corpus DXGI histogram: 63 + 13 + 2 = 78 records across formats 10/11/31
fall through to the wrong branch.

## Impact
Invisible in-engine (the DX10 extended header is read, not the legacy
field), so the blast radius is external tooling / texture dumps — same
standard #594 was fixed under.

## Suggested Fix
Add `10 | 11 => Some(8)`, `31 => Some(4)` to the `bpp` match with matching
tests.

## Related
#594 (CLOSED), SF-D1-03 (same records, renderer side).

## Completeness Checks
- [ ] **SIBLING**: Fix alongside SF-D1-03 — same 78-record set
- [ ] **TESTS**: A fixture for each of DXGI 10/11/31 asserts the correct `dwPitchOrLinearSize` form
