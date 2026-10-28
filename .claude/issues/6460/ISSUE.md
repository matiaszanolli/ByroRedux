# FNV-2026-10-27-01: #6251 residual: 406 FNV INFOs, 4 whole quests (157 INFOs) and 22 objective targets are still declined only by CTDA functions the ObScript VM already answers

Labels: high, scripting, dialogue, game:fnv, legacy-compat, bug
Source: docs/audits/AUDIT_FNV_2026-10-27.md

**Source**: `docs/audits/AUDIT_FNV_2026-10-27.md` finding `FNV-2026-10-27-01` (Dimension 5, dialogue and quests through the M47.1 condition gates; owner `/audit-scripting` catalog + `/audit-gameplay` Dim 2) · Severity HIGH (raised from MEDIUM by the suite because the FO3 leg drops 24% of FO3's INFOs) · Status NEW (residual of #6251, closed by `f57c75616`) · Game: FNV (FO3 leg measured in AUDIT_FO3_2026-10-27 as a dup of this finding — file once, label `game:fo3` too if the FO3 report is not published separately).

**Location**: `crates/scripting/src/condition/catalog.rs` (`ROWS`, `FOREIGN_INDICES`: 365 is the Oblivion-only `GetPlayerInSEWorld` row, foreign on FO3/FNV where 365 is `IsChild`). Existing getter bodies: `crates/scripting/src/obscript_more/tail.rs` (`is_child`, `0x116d`); `obscript_more.rs` (`get_talked_to_pc`, `0x1032`); `cmd_1174` (`IsInList`, `0x1174`); `CMD_GET_HAS_NOTE` `0x117e`, `CMD_GET_LOCKED` `0x1005`, `CMD_GET_CURRENT_TIME` `0x1012`, `CMD_GET_IN_SAME_CELL` `0x1020`, `CMD_GET_BASE_AV` `0x1115`, `CMD_GET_DESTRUCTION_STAGE` `0x11d7`; `get_map_marker_visible` `0x11ec`, `get_hit_location` `0x1187`, `cmd_1120` (`GetFriendHit`), `get_current_ai_package` `0x106e`.

## Description
The fail-closed gates decline any condition whose function `is_modeled` rejects. The ObScript VM answers these functions as script getters (command id `0x1000 + index`), but the CTDA catalog has no row for them. The gates therefore cannot read state the engine already keeps. #6251 catalogued ten getters and named only 427 and 372 as follow-ups; 427 has since landed.

## Evidence
`catalog.rs` `ROWS` contains no row with index 372 (`IsInList`), 50 (`GetTalkedToPC`), 382, 5, 18, 32 or 277; index 365 maps only to the Oblivion `GetPlayerInSEWorld` row.

CTDA census over `FalloutNV.esm`: 984 of 23,247 INFOs are still declined (5,968 at 10-21). Of those, **406 are declined only by VM-implemented functions**: 365 x140, 50 x71, 382 x39, 372 x32, 5 x24, 18 x18, 32 x15, 277 x15 and smaller. Quest-level gates decline 4 quests whole: **`GenericAdultCombat`** (71 INFOs, `IsChild`), **`GenericKids`** (69, `IsChild`), **`Followers`** (14, `IsInList`) and `DialogueCrimeKarma` (3, `GetTalkedToPC`). 22 `QSTA` targets are declined only by VM getters: 471 x9, 6 x4, 492 x3 and others. The largest remaining non-VM blocker is `GetScriptVariable` 53 (333 INFOs).

FO3 leg (AUDIT_FO3_2026-10-27): with no row for `IsInList` (372) or `GetTalkedToPC` (50), the quest-condition gate fails closed on **21 FO3 quests, 5,377 of 22,327 INFOs (24%)** (DialogueMegaton 746, ConvUnderworld 516, ConvMegaton 448, MS16 447, DialogueRepublicDave 404, Followers 395, DialogueCitadel 380), plus 522 INFOs carrying these functions in their own conditions.

## Impact
Every adult's generic combat barks and every child's generic lines are unselectable on FNV, and so are the `Followers` lines. Some objective markers never highlight. On FO3, 24% of INFOs are dropped. Nothing crashes.

## Related
#6251 (closed; same class), #5750 (427), AUDIT_FO3_2026-10-27.

## Suggested Fix
As in #6251, give each function one implementation: add catalog rows (the 365 row is a game-variant pair) that call the existing getter bodies. Add a real-data floor on the number of declined FNV INFOs and quests.

## Completeness Checks
- [ ] **SIBLING**: FO3, Oblivion and Skyrim catalog rows for the same indices (`FOREIGN_INDICES` pairs) checked; the 365 `IsChild` vs `GetPlayerInSEWorld` split handled
- [ ] **CANONICAL-BOUNDARY**: Each function has one implementation shared by the VM getter and the condition catalog, no duplicated logic
- [ ] **LOCK_ORDER**: New catalog evaluators declare their reads on `npc_dialogue_selection` (condition catalog <-> schedule scan)
- [ ] **TESTS**: A real-data floor asserts the declined FNV (and FO3) INFO/quest counts do not regress

