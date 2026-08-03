# FO3-D6-NEW-01: parse_real_facegen.rs docstring claims FNV+FO3 coverage but the test only ever exercises FNV by default

Filed from: `docs/audits/AUDIT_FO3_2026-08-03.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2335

**Severity**: LOW
**Location**: `crates/facegen/tests/parse_real_facegen.rs:1-41`
**Status**: NEW

### Description
The module doc claims FNV/FO3 coverage; the actual data-dir resolution + BSA filename are FNV-only, with no `BYROREDUX_FO3_DATA` fallback (unlike the sibling NIF/`.spt` real-data tests, which do have dedicated FO3 arms).

Confirmed against current code: `parse_real_facegen.rs:1-3` doc says "vanilla FNV / FO3 content"; `data_dir()` reads only `BYROREDUX_FNV_DATA` (or the hardcoded `FNV_DEFAULT_DATA` Steam path) and `FNV_MESH_BSA` is hardcoded to `"Fallout - Meshes.bsa"` under the FNV data dir — no FO3-specific env var or path exists anywhere in the file.

### Evidence
Manually pointed the FNV env var at the FO3 install — all 3 tests pass; FO3's `headhuman.{egm,egt,tri}` are byte-for-byte identical to the hardcoded FNV baselines (asset-reuse coincidence, not something the test structurally guarantees).

### Impact
No functional bug — FaceGen parsing genuinely works on real FO3 data. Pure test-coverage/CI-signal gap: nothing would catch a future FO3-only face asset (ghoul/super-mutant/robot) diverging from the FNV-shared assets.

### Suggested Fix
Parametrize the existing 3 tests over `[("FNV", …), ("FO3", …)]` mirroring the `Game` enum pattern in `parse_real_nifs.rs`/`parse_real_spt.rs`.

### Related
analogous to already-closed #1452/#2090 (same class, doc-overclaim)

## Completeness Checks
- [ ] **SIBLING**: Mirror the `Game`-enum parametrization pattern already used in `parse_real_nifs.rs`/`parse_real_spt.rs`
- [ ] **TESTS**: Parametrized test suite exercises both FNV and FO3 data dirs explicitly, not by coincidental byte-identity
