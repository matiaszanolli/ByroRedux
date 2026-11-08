# ESM-D4-01: FO76 `PERK` entries are never opened (2-byte `PRKE`), and the record-level `DATA` / `CTDA` arms swallow the entry bodies

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6789
Labels: low, esm-plugin, bug, game:fo76
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

- **Severity**: LOW (FO76 `PERK` is a declared PRD non-goal, PERK-NG-006, so no consumer reads these; but the record-level fields are silently wrong)
- **Dimension**: 4 · **Record/Sub-record**: FO76 `PERK` `PRKE` / `DATA` / `CTDA`
- **Location**: `crates/plugin/src/esm/records/misc/magic.rs:530` (`b"PRKE" if sub.data.len() >= 3`), `:512` (record-level `DATA`), `:681` (`CTDA | CIS1 | CIS2 if block is None`, new in PERK-DEC-001, `6a76e9f54`)
- **Status**: NEW
- **Description**: xEdit's FO76 `PRKE` is `Type, Rank` only; the Priority byte is commented out (`FO76.pas:17912`: `wbStructSK(PRKE, [1, {2,} 0]`). The parser requires 3 bytes, so on FO76 no entry block ever opens. Every later entry sub-record then hits a record-level arm instead:
  - the entry `DATA` (4 B) re-runs the record-level arm and overwrites `is_trait` / `level` / `num_ranks` / `playable` / `hidden`;
  - the entry `CTDA`s are appended to `PerkRecord::conditions` as if record-level.
- **Evidence**:
  - `SeventySix.esm` has 2,525 `PRKE` (all exactly 2 B), 2,522 entry `DATA` of 4 B and 1,992 record-level `DATA` of 3 B.
  - `perk_entry_census SeventySix.esm`: `1992 PERK`, `entries: 0 quest, 0 ability, 0 entry-point`. FO4 on the same binary decodes entries.
  - 4,230 `CTDA` across 1,074 records; they are all treated as record-level.
- **Impact**: FO76 `PerkRecord` playable / rank / level / condition data is wrong. Latent: nothing consumes FO76 perks today (EPFD is deliberately `None` for FO76, test `parse_perk_fo4_epfd_shapes` tail). It would corrupt any future FO76 perk or Studio-style tooling.
- **Suggested fix**: accept `PRKE >= 2` (priority 0) when `game == Fallout76`, or gate the record-level `DATA` / `CTDA` arms to "no `PRKE` seen yet" so entry bodies are dropped rather than aliased. Add a FO76 fixture to `magic.rs` tests (2-byte `PRKE`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
