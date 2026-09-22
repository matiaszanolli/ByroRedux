# PAR-D4-2026-09-21-03: Sweep gaps: no FO76 BGSM or BA2 sweep, no FNV MenuXml corpus, no Oblivion EGM test, and CDB asserts only non-zero counts

Labels: low,bug,import-pipeline,test-gap,game:fo76,game:fo3,game:fnv,game:oblivion

## Description
- **FO76 BGSM** (`crates/bgsm/tests/parse_all.rs:255-268`). Never swept. This audit measured 29,989/29,991 OK; the 2 failures are vanilla `.bgsm` files that are Material-Editor JSON text (`materials\atx\setdressing\atx_plushie_mr.fuzzy_valentinesday\*.bgsm`), rejected with `BadMagic`. Whether FO76's own runtime reads JSON BGSM is unverified.
- **FO76 BA2** (`crates/bsa/tests/ba2_real.rs`). No parser-crate test; the DX10 path is never exercised on FO76 content, which now holds 40 `.ba2` files after the 2026-09-20 archive rewrite.
- **FO3 BSA**. The v104 BSAs have no test.
- **FNV MenuXml**. The HUD profile ships (`hud.rs:148`, 121 menu XMLs) with zero tests.
- **Oblivion FaceGen**. 141 EGMs, no test. The FNV/FO3 EGM test (`crates/facegen/tests/parse_real_facegen.rs:178-227`) prints the non-finite count instead of asserting it, which would have caught PAR-D5-2026-09-21-01 far earlier.
- **CDB** (`crates/sfmaterial/tests/real_cdb.rs:83-90`). Asserts only non-zero counts; the measured 97 classes / 1,438,780 values could be pinned exactly.

Verified unchanged at HEAD `ee6d3fb39`: none of the six gaps has a new test at any of the cited locations.

## Evidence
See the Gate Matrix in the source report; probe logs `probe_bgsm_scan.log` and `probe_dup_scan.log` back the FO76 BGSM/BA2 counts.

## Impact
Format branches with real vanilla content in the installed corpora but no regression pin, across five titles.

## Related
PAR-D4-2026-09-21-01 (no CI lane runs any of these even if they existed), #3466

## Suggested Fix
Add FO76 BGSM (with a JSON-form allowlist) and FO76 BA2 GNRL/DX10 sweeps, an FNV MenuXml corpus test, and an Oblivion EGM case. Pin the CDB counts exactly rather than just asserting non-zero.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D4-2026-09-21-03)

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix