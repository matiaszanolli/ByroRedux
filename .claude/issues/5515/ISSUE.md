# #5515: ESM-2026-10-09-D4-02: SMQN's per-quest TES5 `FNAM` ("24 Hours Till Reset") and FO76 `UNAM` ("Priority") are stored at node level, last one wins

**Labels**: bug, esm-plugin, game:fo76, game:skyrim, low, quests

**Source**: `docs/audits/AUDIT_ESM_2026-10-09.md` — finding `ESM-2026-10-09-D4-02` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: LOW. No consumer reads either field, and vanilla loses nothing today. The data model contradicts the reference.
- **Dimension**: Record Schema Dispatch & Coverage
- **Record / Sub-record**: `SMQN` / `FNAM`, `UNAM`
- **Location**: `crates/plugin/src/esm/records/misc/story_manager.rs:197-203` (field docs) and `:289-294` (arms).
- **Status**: NEW. This is a residual of #5420 (closed).
- **Description**: xEdit nests both fields inside the per-quest `wbRStructSK([0], 'Quest', …)`, next to `NNAM` and `RNAM`:
  - TES5: `NNAM`, `FNAM`, `RNAM` (`wbDefinitionsTES5.pas:7089-7094`)
  - FO76: `NNAM`, `RNAM`, `UNAM` (`wbDefinitionsFO76.pas:11029-11035`)

  The decode already models `RNAM` per link (`SmQuestLink::reset_hours`), but it puts `FNAM`/`UNAM` in node-level `Option<u32>`s, where each later quest's value overwrites the earlier one. (`HNAM` is a `wbFloat` held as raw `u32` bits. That is documented, so it is not filed.)
- **Evidence** (`scripts/sm_unam.py`):
  - `Skyrim.esm`: 5 of 448 SMQNs author `FNAM`. 2 carry several, with identical values.
  - `SeventySix.esm`: 4 of 1,026 SMQNs author `UNAM`, each once.
- **Impact**: A mod with mixed per-quest values, or a future consumer of FO76 per-quest priority, would get the last quest's value for every quest.
- **Related**: #5420, #5386 (closed).
- **Suggested Fix**: Move `fnam`/`unam` onto `SmQuestLink` as `Option<u32>` fields, filled from the most recent `NNAM` like `RNAM`, and keep the xEdit citations.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
