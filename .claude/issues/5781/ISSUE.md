# OBL-D1-2026-10-14-02: the drift corpus swallows per-entry extract failures

**Labels**: low,game:oblivion,legacy-compat,nif-parser,test-gap,bug
**Source**: docs/audits/AUDIT_OBLIVION_2026-10-14.md

Found by `/audit-oblivion` (docs/audits/AUDIT_OBLIVION_2026-10-14.md, HEAD `adbd468e2`). Finding ID: `OBL-D1-2026-10-14-02`. Re-verified against current code before filing.

- **Severity**: LOW (test gap, defense in depth only)
- **Dimension**: 1
- **Location**: `crates/nif/tests/oblivion_stream_drift_corpus.rs` `no_block_sizes_drift_detector_has_zero_false_positives_on_real_corpus` (`let Ok(bytes) = archive.extract(f) else { continue; }`, `let _ = parse_nif(&bytes)`)
- **Status**: NEW
- **Description**: #5439 closed the open-failure facet. An archive that opens but whose entries fail to extract (or NIFs that hard-fail to parse) is still counted in `total_files` and contributes nothing. The test asserts only `count == 0` and never that anything was actually walked.
- **Evidence**: with the base BSA truncated to 300 MB the test printed "1 archive(s), 8032 NIF(s), 0 drift warning(s)" and passed while 6 643 of 8 032 extracts failed.
- **Impact**: none today, because `parse_rate_oblivion`, `oblivion_block_count_parity` and `per_block_baseline_oblivion` all go red on the same input. The detector alone would silently measure ~17 % of the corpus.
- **Related**: #5439, #3711.
- **Suggested Fix**: count extract and parse failures and assert zero (or assert `parsed == total_files`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' arms, sibling parsers/tests)
- [ ] **TESTS**: A regression test pins this specific fix
