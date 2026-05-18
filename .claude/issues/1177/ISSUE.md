# FO4-D2-NEW-02: docs/engine/archives.md lines 230-232 and 248-249 stale vs #593/#594

**Labels**: documentation, low

**Source**: [`docs/audits/AUDIT_FO4_2026-05-18.md`](docs/audits/AUDIT_FO4_2026-05-18.md)
**Dimension**: BA2 Reader (GNRL + DX10)
**Severity**: LOW (doc rot, not a parser bug)

## Observation

`docs/engine/archives.md`:

- **Lines 230-232**: "pitchOrLinearSize = computed for known DXGI formats (BC1/3/5/6/7), falls back to total pixel data length otherwise"
- **Lines 248-249**: "array size = 1"

Both descriptions predate two shipped fixes:

- **#593 / FO4-DIM2-02** added cubemap detection so `array size = 6` is emitted for cubemap textures. Verified at `crates/bsa/src/ba2.rs:791-792`. Regression test: `build_dds_header_cubemap_array_size_is_six`.
- **#594 / FO4-DIM2-03** added the `DDSD_PITCH` path for uncompressed DXGI formats (28/29/56/61/87/91). Verified at `crates/bsa/src/ba2.rs:843-851`. Regression test: `pitch_rgba8_unorm_matches_row_size_with_pitch_flag`.

## Why bug

The parser is correct. The docs are not. Per the `feedback_audit_findings` memory, 5/30 audit findings in the 2026-04 sweep were stale on premise — outdated docs are the most common source. A future auditor taking these lines as ground truth would file a stale finding.

## Fix

Update `docs/engine/archives.md`:

- Lines 230-232: describe the per-format pitch (`DDSD_PITCH` for R8 / R16 / R8G8B8A8 / etc.) vs linear-size (`DDSD_LINEARSIZE` for BC1-BC7) branch. Cite `pitch_rgba8_unorm_matches_row_size_with_pitch_flag`.
- Lines 248-249: describe the cubemap-vs-non-cubemap arraySize selection (1 for 2D, 6 for cubemap). Cite `build_dds_header_cubemap_array_size_is_six`.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: check `docs/engine/archives.md` for other passages describing code that has since been changed
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: N/A (docs only)

## Related

- #593 / FO4-DIM2-02 — cubemap arraySize=6
- #594 / FO4-DIM2-03 — DDSD_PITCH for uncompressed formats
