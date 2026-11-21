# #7101 — ESM-D1-03: Residual FO76 279.0 / rv 209 pins after #6779 (`esm-records.md`, two tests)

Labels: low,esm-plugin,documentation,doc-rot,game:fo76
Filed from: docs/audits/AUDIT_ESM_2026-11-20.md (finding ESM-D1-03)

Source: `docs/audits/AUDIT_ESM_2026-11-20.md` (`/audit-esm`, HEAD 47ae285c6 / f3713bf5e) · ID **ESM-D1-03**

- **Severity**: LOW (doc/test rot; classification unaffected, floor is `>= 60.0`)
- **Dimension**: 1, header detection
- **Record / Sub-record**: TES4 / HEDR
- **Location**: `docs/engine/esm-records.md:228` ("68.0 → 266.0 → 279.0 (2026-09-20 patch; rec_ver 209 throughout)"); `crates/plugin/src/esm/records/tests.rs:2602-2609` (`scol_grup_parsed_for_fo76`: comment says "installed masters read 279.0 … Probe the current value plus one historical one" and the loop is `[279.0, 266.0]`); `crates/plugin/src/esm/records/spatial_units_tests.rs:107,236` (`[0.94, 1.34, 1.71, 1.0, 279.0]`)
- **Status**: NEW (residual of closed #6779, same class as #3405 / #4643, which also fixed only the table in `reader.rs`)
- **Description**: #6779 refreshed the `reader.rs` table, doc and probes. These other copies of "the current FO76 value" still say 279.0, and `esm-records.md` still says rec_ver 209 "throughout", which is false now that the masters read 210.
- **Evidence**: Direct TES4 read of both installed masters gives 280.0 / rv 210 (mtime 2026-10-24). No runtime effect: the tests still pass because the band is a floor.
- **Impact**: None at runtime. The "current value" probes no longer probe the current value.
- **Suggested fix**: Add 280.0 to those three loops. Reword the `esm-records.md` line to "drifts; last read 280.0 / rv 210 on 2026-11-08".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms (per-game layout splits, other callers of the same helper)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic fixture in the default lane; real-data pins stay in the ignored lane)
