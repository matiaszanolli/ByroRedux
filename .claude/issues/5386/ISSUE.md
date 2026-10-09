# #5386: ESM-2026-10-08-D2-01: `SMQN` `RNAM` is stored as 24 × hours (xEdit scale `1/24`), but it is read as raw hours, so radiant re-fire windows are 24× too long

**Labels**: medium,esm-plugin,scripting,quests,bug,game:skyrim,game:fo4,game:fo76,game:starfield
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5386

**Source**: `docs/audits/AUDIT_ESM_2026-10-08.md` — `ESM-2026-10-08-D2-01` (HEAD `00f580e09`)

- **Severity**: MEDIUM. The value is live: it gates quest re-fire in `process_quest_node`.
- **Dimension**: Sub-Record Byte Accounting
- **Record / Sub-record**: `SMQN` / `RNAM`
- **Location**:
  - `crates/plugin/src/esm/records/misc/story_manager.rs:53-69` (`SmQuestLink` doc: "1152.0 = 48 game days") and `:212-224`.
  - `docs/engine/story-manager.md:195-200`.
  - Consumer: `crates/scripting/src/story_manager.rs:721-725` and `:779-786` (`hours_gate_open`).
- **Status**: NEW
- **Description**: Every game declares `wbFloat(RNAM, 'Hours until reset', cpNormal, False, 1/24)`: TES5.pas:7093, FO4.pas:8757, FO76 and SF1. In `wbInterface.pas`, `TwbFloatDef` displays `stored × fdScale` (`:16568-16569`) and writes `value / fdScale` (`:16395-16396`), so the stored float is hours × 24. Node-level `HNAM` (FO4+) is declared without a scale, and its values are already round hours (1, 12, 24, 72).
- **Evidence**: `RNAM` value census:

  | master | values (count) |
  |---|---|
  | Skyrim | 576 (250), 24 (51), 1152 (15), 12 (9), 4.8 (6), 2.4 (3), 48/96/288 (3 each), 6 (2), 1728 (1) |
  | FO4 | 576 (180), 288 (45), 432 (25), 24 (25), 144 (22), 192 (6), 1728 (4), 4320/4800 (2 each), 9600/2304/1152/48/12 (1–2) |
  | Starfield | 576 (60), 48 (41), 96 (32), 24 (31), 288 (12), 72 (8), 2.4 (5) |

  Divided by 24 these are round CK hour values (24, 48, 72, 12, 18, 6, 180, 200, 400 h; 0.1 h for the Sovngarde ambient scenes). Read raw, the dominant 576 means 24 game **days**.
- **Impact**: 250 Skyrim and 180 FO4 radiant links that should re-arm after 1 game day wait 24 days. FO4's Minutemen 4320 becomes 180 days instead of 7.5. The save-persisted `last_fire_hours` gate compounds the error on every reload.
- **Related**: D4-01, D4-02, #5366.
- **Suggested Fix**: Store `reset_hours = raw / 24.0` in `parse_sm_node`, citing xEdit's scale, and fix both docs ("1152.0 = 48 h"). Pin it with a real-data assert (the Skyrim 576-valued majority decodes to 24.0).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other xEdit-scaled `wbFloat` sub-records decoded raw; node-level `HNAM`; both doc sites)
- [ ] **TESTS**: A regression test pins this specific fix
