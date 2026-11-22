# #7096 — ESM-03: One engine-path `Starfield.esm` load logs 62,576 spurious `remap_starfield` warn lines (four mis-strided or non-FormID decoders)

Labels: medium,esm-plugin,bug,game:starfield
Filed from: docs/audits/AUDIT_ESM_2026-11-20.md (finding ESM-03)

Source: `docs/audits/AUDIT_ESM_2026-11-20.md` (`/audit-esm`, HEAD 47ae285c6 / f3713bf5e) · ID **ESM-03**

- **Severity**: MEDIUM (every Starfield session; masks the genuine warning class; four decoder causes)
- **Dimension**: Real-Data Validation (Dim 8), with Dims 2/3/4/5 for the `DOOR` slice
- **Record / Sub-record**: `NPC_` `PRPS`, `LCTN` `ACSR`/`LCSR`/`ACPR`/`LCPR`, `DOOR` `ANAM`/`BNAM`, `CTDA` `param_1`
- **Location**: the warn is `crates/plugin/src/esm/reader.rs:617-645` (`remap_starfield`, the `None =>` arm). The callers are:
  - `records/actor/npc.rs:1154-1171`
  - `records/misc/location.rs:62-112`
  - `cell/support.rs:90-103`
  - `records/condition.rs:638-641`
- **Status**: NEW
- **Description**: `Starfield.esm` lists no masters, so any non-zero top byte that is not `0xFE`/`0xFD` cannot be resolved. `remap_starfield` then logs at `warn` and passes the value through. Four decoders hand it words that are not FormIDs:
  - The second half of a mis-strided 12-byte `PRPS` entry (ESM-D8-02).
  - The `wbUnknown(4)` / grid words of Starfield's longer `LCTN` ref entries (ESM-D8-01; xEdit `wbDefinitionsSF1.pas:13401-13470`, re-verified by the orchestrator).
  - `DOOR` component strings and the Facing Axis enum (ESM-01).
  - The `0xFFFFFFFE` CTDA sentinel (ESM-D8-03).

  `parse_esm` passes no remap, so the flood appears only on the real `parse_esm_with_load_order` path. No plugin example, test or `sf_smoke` baseline shows it.
- **Evidence**: release `byroredux --esm Starfield.esm --list-cells` (9.1 s, 4.07 GB RSS) logs 62,581 warn lines; 62,576 come from `remap_starfield` (247 distinct values). Each line was attributed by a backtrace-capturing `log::Log` in a scratch probe crate:

  | Source | Warn lines |
  |---|---|
  | `npc.rs:1169` | 23,981 |
  | `location.rs:106` | 33,735 |
  | `location.rs:68` | 4,065 |
  | `support.rs:91` | 620 |
  | `condition.rs:640` | 175 |

  The commonest values are `ffffffff` (23,557), `7fff7fff` (11,655) and `40a00000` = 5.0f (8,116).
- **Impact**:
  - Every Starfield boot formats and emits about 62k warn lines.
  - A genuine "names a master this plugin does not list" case (a mis-ordered or missing master, the reason #5075/#5076 kept this at `warn`) cannot be seen in the noise.
  - The underlying fabricated data is listed per cause.
- **Suggested Fix**: Fix the four decoders (ESM-01, ESM-D8-01..03). Then add a guard that parses `Starfield.esm` through the load-order path and counts remap pass-throughs. A fixture-scale variant could run in the default lane: a synthetic master-less Starfield plugin per affected record. Separately, consider aggregating the pass-through into one summary line per plugin (count + first N values), as the #1308 Oblivion `0x01` path does at `debug`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms (per-game layout splits, other callers of the same helper)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic fixture in the default lane; real-data pins stay in the ignored lane)
