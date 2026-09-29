# #5073: FO3-D4-01: A compressed BSA entry with an empty payload is a hard extract error

**Labels**: bug, import-pipeline, low, legacy-compat, game:fnv, game:fo3, game:oblivion

**Source report**: `docs/audits/AUDIT_FO3_2026-09-29.md`
**Severity**: LOW
**Dimension**: BSA v104 (reader discipline, owner `/audit-parsers`)

## Location
- `crates/bsa/src/archive/extract.rs` (compressed branch of `BsaArchive::extract`, the v103/v104 zlib arm).
- `crates/bsa/src/safety.rs` (`inflate_bounded_zlib`).

## Description
`Fallout - Meshes.bsa : meshes\dungeons\vaultruined\placeholder.txt` has record size 4 and is archive-compressed: a 4-byte original-size header of 0 and no zlib stream. `extract` passes the empty slice to `inflate_bounded_zlib`; `ZlibDecoder` fails with "incomplete deflate stream"; the raw-DEFLATE retry is skipped because `compressed.len() >= 2` is false. A valid empty file therefore returns `Err`, while `declared_size()` on the same entry returns `Ok(0)`.

## Evidence
Census by record-header walk; every such entry is a `.txt` placeholder:
- FO3: 1 (above).
- FNV: 1 (`Fallout - Misc.bsa : menus\do_not_delete.txt`).
- Oblivion: 35 (`textures\menus*\…\*.txt` in `Oblivion - Textures - Compressed.bsa`, each `incomplete deflate stream` with `declared=Some(0)`).

A full extraction sweep of all 16 FO3 BSAs (159 155 entries) had exactly this 1 error.

Validated at HEAD 9fcfdc3fc: the compressed branch computes `original_size` and `compressed_len = data_size - 4` and calls `inflate_bounded_zlib(&compressed, original_size, …)` with no empty-payload short-circuit; `inflate_bounded_zlib` returns the zlib error when `compressed.len() < 2`.

## Impact
No content loads these files. Whole-archive consumers (corpus sweeps, extract-all tooling, an archive browser) see spurious hard errors on vanilla data, and `declared_size` and `extract` disagree.

## Related
- #3410 (bounded inflate), #3812 (Adler-32 recovery).
- Label gap: BSA/BA2 readers have no own label; filed under `import-pipeline`.

## Suggested Fix
In the compressed branch, return `Ok(Vec::new())` when `original_size == 0` and the compressed stream is empty. Keep rejecting a non-zero declared size with an empty stream. Pin it with a synthetic 4-byte compressed record (and check the v105 LZ4 arm for the same edge).

## Completeness Checks
- [ ] **SIBLING**: Same edge checked in the v105 LZ4-frame arm and the BA2 GNRL/DX10 zlib paths
- [ ] **TESTS**: A regression test pins this specific fix (synthetic zero-size compressed record → `Ok(empty)`; non-zero size + empty stream → `Err`)

