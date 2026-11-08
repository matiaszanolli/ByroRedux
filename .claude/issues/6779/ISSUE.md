# ESM-D1-01: FO76 masters now stamp HEDR 280.0 / TES4 record version 210; the sampled-value table, doc and test still pin 279.0 / 209

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6779
Labels: low, esm-plugin, documentation, doc-rot, game:fo76
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

- **Severity**: LOW (doc/test rot; classification is correct)
- **Dimension**: 1, header detection
- **Record/Sub-record**: TES4 / HEDR
- **Location**: `crates/plugin/src/esm/reader.rs:221`, `:249-262`, `:282`, `:1806`, `:1868-1876`
- **Status**: NEW (same class as closed #3405 / #4643 / #5077)
- **Description**: The `GameKind::from_header` sampled-value table and the `Fallout76` variant doc say
  "68.0 → 266.0 → 279.0 … 279.0 on 2026-09-20, both masters". The installed masters were updated on
  2026-10-24 (mtime of `SeventySix.esm` and `NW.esm`).
- **Evidence**: Direct TES4 header read of both files:
  `Fallout76/Data/SeventySix.esm  hedr=280.000 rv=210` and `NW.esm  hedr=280.000 rv=210`.
  Every other master matches the table: FO3 GOTY 0.94/rv 2 (DLC 0.94/rv 15), FNV 1.34/rv 2
  (DLC 1.32 to 1.34/rv 15), FO4 1.0 or 0.95/rv 131 (every installed DLC, CC and mod),
  Skyrim SE 1.71/rv 44 (cc *.esl 1.70), Starfield 0.96/rv 552 to 582 (including 30+ community masters
  at 555 to 576).
- **Impact**: None at runtime. The `>= 60.0` floor classifies 280.0 as Fallout76, and `light_master` stays
  false for FO76. The test and comment tables are what "every band gap is reasoned from"
  (#4643), and they are one patch stale again.
- **Suggested fix**: Add a 280.0/210 probe line to `game_kind_from_header_maps_real_master_hedr_values`
  and refresh the table date. Prefer wording "drifts; last read 280.0 on 2026-11-08" so the next patch
  does not re-open this.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
