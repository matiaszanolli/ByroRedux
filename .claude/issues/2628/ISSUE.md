# #2628 — SF-D1-02: pitch_or_linear_size_for has no arm for DXGI 10/11/31, 78 vanilla textures get invalid dwPitchOrLinearSize

**Severity**: LOW · **Dimension**: 1 (BA2 v2/v3 LZ4 Block Decompression)
**Location**: `crates/bsa/src/ba2.rs::pitch_or_linear_size_for`

## Fix

Verified the premise: `pitch_or_linear_size_for`'s `bpp` match had no
arm for DXGI 10 (`R16G16B16A16_FLOAT`), 11 (`R16G16B16A16_UNORM`), or 31
(`R8G8B8A8_SNORM`), so all three formats fell through to the legacy
`(total_bytes, DDSD_LINEARSIZE)` fallback instead of the correct
row-pitch form — the same defect class #594 fixed, on formats that fix
never enumerated.

Applied the issue's own suggested fix exactly: `10 | 11 => Some(8)` (4
channels × 16-bit = 8 bytes/pixel) and `31 => Some(4)` (4 channels ×
8-bit = 4 bytes/pixel), added to the existing `bpp` match alongside the
other uncompressed formats.

## SIBLING (issue's own checklist item — "fix alongside SF-D1-03 — same
78-record set")

`SF-D1-03` (#2619, the renderer-side `map_dxgi_format` counterpart for
the same 78-record set) is already **CLOSED**. This fix completes the
BSA-side (DDS header synthesis) half of the same class.

## TESTS (issue's own checklist item — "a fixture for each of DXGI
10/11/31 asserts the correct `dwPitchOrLinearSize` form")

Three new tests, matching the existing per-format convention exactly:
`pitch_r16g16b16a16_float_matches_row_size_with_pitch_flag`,
`pitch_r16g16b16a16_unorm_matches_row_size_with_pitch_flag`,
`pitch_r8g8b8a8_snorm_matches_row_size_with_pitch_flag` — each asserts
the computed row pitch (`width * bpp`) and `DDSD_PITCH`.

**Reintroduce-and-revert verification**: temporarily removed the three
new match arms — confirmed all three new tests failed with the exact
symptom (`left: 0, right: <expected pitch>` — falling back to the
zero-byte `total_bytes` fixture value instead of computing a real
pitch). Restored the fix and reran — all 9 `pitch_*` tests in
`byroredux-bsa`'s `ba2::tests` pass again.

## Verification

- `cargo check -p byroredux-bsa --tests`: clean, zero warnings.
- `cargo test -q -p byroredux-bsa pitch_`: 9 passing, 0 failing (+3
  new).
- `cargo test -q -p byroredux-bsa`: 87 passing, 0 failing (full crate).
- `cargo test -q --no-fail-fast` (full workspace): **7185 passing, 0
  failing**.
