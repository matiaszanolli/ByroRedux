# SF-D2-NEW-01: Starfield BA2 sweep regression-test is single-archive, not corpus-wide

**Labels**: bug, import-pipeline, low

**Source**: [`docs/audits/AUDIT_STARFIELD_2026-05-18.md`](docs/audits/AUDIT_STARFIELD_2026-05-18.md)
**Dimension**: BA2 v2/v3 LZ4 Block Decompression
**Severity**: LOW (test coverage gap)

## Observation

`crates/bsa/tests/ba2_real.rs:358-482`: the committed `#[ignore]` Starfield tests touch ONE archive per code-path:

- `starfield_meshes01_ba2_v2_gnrl_extracts_nif_with_starfield_magic` — Meshes01 (v2 GNRL)
- `starfield_textures01_ba2_v3_dx10_extracts_lz4_block_dds` — Textures01 (v3 DX10 / LZ4)
- `starfield_constellation_textures_ba2_v2_dx10_extracts_zlib_dds` — Constellation - Textures (v2 DX10 / zlib)

Session 7's "22 archives / ~128K DX10 textures / 0 failures" claim came from a one-shot external sweep. No in-tree test walks all 30 vanilla `Starfield - *.ba2` archives.

## Why bug

The audit-side "confirm Session 7's claim still holds" check has no in-tree mechanism. A regression that breaks any of the other 27 archives (mid-corpus DLC archives, recent content updates) would only be caught by an external run, not by `cargo test --ignored starfield`.

## Fix

Add a sweep test that:
1. `read_dir`s `BYROREDUX_STARFIELD_DATA`
2. Opens every `Starfield - *.ba2` (and per-CC/DLC archives if discoverable)
3. For each archive, opens via `Ba2Archive::open` and extracts one representative entry
4. Reports per-archive pass/fail summary — a single corrupted archive shouldn't abort the sweep

Keep `#[ignore]`. Model on `parse_rate_starfield_all_meshes` in `crates/nif/tests/parse_real_nifs.rs`.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: confirm parallel sweep tests exist for FO4 BA2 corpus (the `fo4_meshes_ba2_v8_brute_force_extract_zero_errors` test is the closest model)
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: this issue IS the test addition

## Related

- #708 — Session 7 BA2 v3 LZ4 work
- #759 — `parse_rate_starfield_all_meshes` (NIF-side full-corpus sweep model)
