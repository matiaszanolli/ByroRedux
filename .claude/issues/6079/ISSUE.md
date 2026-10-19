# FNV-SD-06: PSDT Day-of-Week is ignored; 53 FNV packages (5 actor bases) schedule by weekday

Labels: low,ai,esm-plugin,bug,game:fnv,legacy-compat

**Severity**: LOW · **Dimension**: 5 (ambient AI package selection) · **Game**: FNV (FO3 not censused)

## Description
`PSDT` Day-of-Week is ignored. `PackSchedule` keeps start hour and duration only, and the PTDT doc calls the other PSDT bytes "unused". The game calendar exists now (`GameTimeRes::day`, `GameCalendar`, `gregorian_weekday`).

## Evidence
Location: `crates/plugin/src/esm/records/misc/pack.rs:355-358, 406` (`PackSchedule`; PTDT doc at `:149-160`), `byroredux/src/npc_spawn/ai_package.rs:772-791` (`select_active_package` passes only `game_hour`). Re-confirmed: no weekday handling in `pack.rs` or `ai_package.rs`.

Raw scan of the 4,163 FNV PACK records: Month is -1 for 4,163 / 4,163 and Date is 0 for 4,163 / 4,163 (genuinely wildcards); **Day-of-Week is constrained on 53 packages** (Sun 5, Mon 4, Tue 1, Wed 3, Fri 2, Sat 2, Weekdays 1, Weekends 8, MWF 14, TT 13). Five actor bases list them, as variant triples: `VProspectorDenPrimmPatrolAM{7,8,9,10}` x {`MWF`,`SS`,`TT`} and `VSpecialGSGeckoHunters*MWF`. All three variants pass the hour test, so the first listed wins every day.

## Impact
The Primm Prospector Den patrol and the Goodsprings gecko hunters run the wrong day's route. Small, but the docs call the bytes unused and they are not. ROADMAP lists "calendar-aware scheduling" as an open M42 item generically; no issue exists.

## Related
M42 ambient AI.

## Suggested Fix
Decode Day-of-Week into `PackSchedule` and test it against the calendar weekday in `active_at`.

Source: docs/audits/AUDIT_FNV_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the sibling games' paths (FO3, FNV, Oblivion share this path)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser / spawn boundary, never pushed into shaders or the renderer
- [ ] **TESTS**: A regression test pins this specific fix (real-data `--ignored` test where the defect is a data census)
