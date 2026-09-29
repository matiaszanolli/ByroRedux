# TD1-2026-09-29-06: `esm/records/actor/mod.rs` crossed to 2120 (+125)

**Labels**: low,esm-plugin,tech-debt,bug

**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

- **Severity**: LOW · **Dimension**: 1 · **Status**: NEW · **Effort**: small
- **Location**: `crates/plugin/src/esm/records/actor/mod.rs`
- **Age**: `9789d8153` (#4414 faction hostility, +80); `5570c221c`; `cd4fc019a` (#4415 magic)
- **Suggested Fix**: split by record, and re-export from `mod.rs` so `records::actor::*` paths stay put.
  - `npc.rs`: `NpcRecord` + `parse_npc*`
  - `race.rs`: `RaceRecord`, `head_part`, and `parse_race` (a 358-line function)
  - `class.rs`
  - `faction.rs`

**Validated at HEAD 9fcfdc3fc**: `prod_loc crates/plugin/src/esm/records/actor/mod.rs` = 2120.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
