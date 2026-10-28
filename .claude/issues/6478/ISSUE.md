# OBL-D5-2026-10-27-01: Fallout-only opcodes tagged `Games::Any` shadow Oblivion's `IsPlayerInJail` (0x10ab) and `CloseCurrentOblivionGate` (0x10c0)

Labels: medium, game:oblivion, legacy-compat, scripting, quests, bug
Source: docs/audits/AUDIT_OBLIVION_2026-10-27.md

**Audit report**: `docs/audits/AUDIT_OBLIVION_2026-10-27.md` -- finding `OBL-D5-2026-10-27-01` (MEDIUM)
**Location (re-mapped to current code)**: `crates/scripting/src/obscript_effects.rs`: `CMD_IGNORE_CRIME` 0x10ab (`Games::Any`, `set_alert`, ~:1202) and `CMD_GET_IGNORE_CRIME` 0x10c0 (`Games::Any`, `get_ignore_crime`, ~:1831); latent: `CMD_SET_ENEMY`/`CMD_SET_ALLY` 0x1078/0x1079 (~:1183/:1184) and `CMD_SEND_ASSAULT_ALARM` 0x105f (~:1203). Constants at :68-69 and :1313-1317. Routing in `crates/scripting/src/obscript_quests.rs` (~:625-640). Premise re-read at HEAD: all five rows are still `Games::Any`.
**Status at filing**: NEW (pre-existing from #6139; the `IsPlayerInJail` leg became observable when jail landed in e161c45f3)

## Description
The ObScript command table runs before the CTDA-getter route, and these rows match Oblivion opcodes although they are Fallout-only commands.
- **0x10ab** is `IsPlayerInJail` on Oblivion, and the CTDA evaluator already answers it (fn 171 is in `OBLIVION_DIALECT`, reading `jail::player_in_jail`). The call never reaches the evaluator. In object scripts `set_alert` runs with `on = false`, strips `IgnoreCrime` from the caller and returns `Done { value: None }`. The VM pushes a `None` expression value as `0.0` without counting a decline (`obscript_vm.rs` ~:879/:962/:1309; `declined_calls` does not move), so `If IsPlayerInJail == 1` is silently false. In quest scripts there is no caller, so the call is declined.
- **0x10c0** is `CloseCurrentOblivionGate` on Oblivion. The IgnoreCrime getter answers it with a number, so it reports done and no gate closes. That breaks the rule that unknown commands are counted, never faked.

## Evidence
- UESP *Oblivion Mod/Script Functions* (opcode column extracted from Oblivion.exe) lists 0x10AB IsPlayerInJail, 0x10C0 CloseCurrentOblivionGate, 0x1078 PositionWorld, 0x1079 PositionCell, 0x105F Yield. Oblivion has no IgnoreCrime, GetIgnoreCrime, SetEnemy, SetAlly or SendAssaultAlarm.
- `obscript_tools names Oblivion.esm all 1`: `+0x10ab scripts 7 lines 9 isplayerinjail 100%`, `+0x10c0 scripts 16 lines 16 closecurrentobliviongate 100%`.
- `IsPlayerInJail` callers: `PrisonCellDoorScript`, `PCPrisonCellDoorScript`, `AmuseiScript`, `LeyawiinJailorScript` (object scripts, read 0), `TutorialScript`, `MS09Script`, `TG00FindThievesGuildScript` (quest scripts, declined).
- `CloseCurrentOblivionGate` callers: `SigilStoneScript`, `OblivionGenericSigilStoneScript`, `MQ10SigilStoneScript`, `SigilRingBoom*`, ten `TrigZoneCloseCurrentOblivion*` scripts.

## Impact
The jail sentence that `jail.rs` tracks is invisible to every script that tests it: the prison-door jailbreak bounty never applies (its `== 0` arm always runs); the Thieves Guild start (needs the player in jail) and MS09's Bruma-dungeon branch never fire. Sigil-stone gate closures look modelled in the census when they are not.

## Related
#6139, #6221 (same "decline, don't answer a constant" rule), SCR-2026-10-27-01. `crates/scripting/examples/obscript_tools.rs:12` says "no opcode table exists locally", but the UESP dump has one.

## Suggested Fix
1. Mark the five rows `Games::Fallout`, so 0x10ab on Oblivion falls through to the fn-171 getter and 0x10c0 declines.
2. Add a table test asserting every `Any`/`Both` row's id names the same command in the Oblivion.exe opcode table.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

