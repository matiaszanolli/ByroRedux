# #756: SF-DIM2-03: Zero real-data integration tests for Starfield BA2 paths (parallel to closed FO4 #587)

URL: https://github.com/matiaszanolli/ByroRedux/issues/756
Labels: enhancement, medium

---

**From**: `docs/audits/AUDIT_STARFIELD_2026-04-27.md` (Dim 2, SF-DIM2-03)
**Severity**: MEDIUM
**Status**: NEW (parallel to closed **#587**)

## Description

`crates/bsa/tests/ba2_real.rs` ships three FO4-gated tests (`fo4_meshes_ba2_v8_*`, `fo4_textures1_ba2_v7_*`) but **zero Starfield equivalents**. The session-7 sweep that originally validated v2/v3 was external (one-shot script) and not committed. A future regression in `compression_method` parsing or the v2/v3 header-extension offset slips silently through `cargo test`.

## Impact

Same #587 (FO4-DIM2-05) risk class — "100% pass rate today" is not a CI guard. Without committed tests, a future refactor that breaks the v3 LZ4 path only surfaces at runtime.

## Suggested Fix

Add three sibling tests gated on `BYROREDUX_STARFIELD_DATA` (default `/mnt/data/SteamLibrary/steamapps/common/Starfield/Data`):

1. `starfield_meshes01_ba2_v2_gnrl_extracts_nif_with_starfield_magic` — open v2 GNRL, extract a `.nif`, assert first 4 bytes spell `"Game"` (Gamebryo header).
2. `starfield_textures01_ba2_v3_dx10_extracts_lz4_block_dds` — open v3 DX10, extract a `.dds`, assert `dds[..4] == b"DDS "` and arbitrary mid-buffer byte is non-zero (LZ4 round-trip smoke).
3. `starfield_constellation_textures_ba2_v2_dx10_extracts_zlib_dds` — open v2 DX10, extract a `.dds`, assert magic. Guards SF-DIM2-01 by proving we don't gate on type_tag.

Mirror the `pick_entry` / `data_dir` helper structure already in `ba2_real.rs`.

## Completeness Checks

- [ ] **TESTS**: This issue *is* the test addition.
- [ ] **SIBLING**: Verify `cargo test --features ba2-real-data -p byroredux-bsa` exercises all three when `BYROREDUX_STARFIELD_DATA` is set.
- [ ] **DROP / LOCK_ORDER / FFI**: n/a.

## Related

- Closed #587 (FO4 BA2 real-data tests pattern)
