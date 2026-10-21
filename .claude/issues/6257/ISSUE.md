# #6257: OBL-D2-2026-10-21-01: Oblivion's PSDT calendar fields are never evaluated: 306 weekday-scheduled and 75 date-scheduled packages run every day (158 actors)

**Labels**: medium,bug,gameplay,ai,esm-plugin,game:oblivion,legacy-compat

**Source**: docs/audits/AUDIT_OBLIVION_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: 2 (TES4 PACK decode → ambient package selection) · **Location**: `crates/scripting/src/globals.rs:141-150` (`package_weekday` returns `None` for every game but `Fallout3NV`)

## Location
- `crates/scripting/src/globals.rs:141-150` (`package_weekday` returns `None` for every game but `Fallout3NV`)
- `crates/scripting/src/globals.rs:124-139` (`day_of_week`, which already carries Oblivion's Morndas origin)
- `crates/plugin/src/esm/records/misc/pack.rs:970-1003` (the PSDT arm drops byte 2, the date)
- `crates/plugin/src/esm/records/misc/pack.rs:422-431` (`PackSchedule`, which has no date field)
- consumer `byroredux/src/npc_spawn/ai_package.rs:782-791`

## Status at filing
NEW. #6079 (closed) fixed FO3/FNV only and states "the weekday is only supplied for Fallout 3/NV, where the enum is established". The skill records that same premise. The measurements below show the premise does not hold for Oblivion. No open or closed issue covers the Oblivion arm or the PSDT date byte.

## Description
The decode is shared. `PackDays::from_raw(sub.data[1] as i8)` runs for every game, and Oblivion's 8-byte PSDT has the FO3/FNV byte layout. But the selector only receives a weekday where `package_weekday` supplies one, and it supplies `None` on Oblivion, so `active_at` treats every Oblivion day constraint as satisfied. Both inputs the gate needs already exist for Oblivion:
- **The enum.** UESP *Oblivion Mod:Mod File Format/PACK* (local dump) documents PSDT byte 1 as 0–6 Sundas–Loredas, 7 MTWTF, 8 Loredas+Sundas, 9 MWF, 10 TTh, 255 any. That is exactly the `PackDays` mapping #6079 implemented.
- **The calendar.** `day_of_week(world, false, …)` already uses Oblivion's documented Morndas start ("always starts on Morndas", UESP *Calendar*). `GetDayOfWeek` (`obscript_more/extras.rs:185`) answers from it today.

Fallout's start weekday is, by the code's own comment, *not* documented. So the gate is enabled for the game whose calendar is less established and disabled for the one whose calendar is established.

Separately, PSDT byte 2 (day of month, 0 = any) is read for no game. FNV authors 0 on all 4 163 packages, so it never mattered there. Oblivion authors it on 75 packages.

## Evidence
(`/tmp/audit/oblivion/py/psdt*.py` over `Oblivion.esm`, 7 209 PACK) 
- **Weekday byte.** 6 903 are -1. 153 name a single day (0:53, 1:14, 2:11, 3:20, 4:11, 5:15, 6:29). 153 name a group (7:52, 8:36, 9:35, 10:30).
- **EditorIDs confirm the enum.**
  - Of the 83 single-day packages whose EditorID names a day (`…Mon…`, `…Sat…`, `…Sundas…`), 82 carry exactly that index.
  - 15 `…SS…` packages are 8. 24 `…MWF…` packages are 9. 22 `…TT…` packages are 10.
- **Affected actors.** 158 `NPC_`/`CREA` bases list a weekday package. On all 158 it sits above at least one other package, so on a non-matching day it shadows the package below. Examples:
  - `SECaldanaFightTue20x2` / `Thu` / `Sat`: the Crucible fight club runs nightly.
  - `SEFellmoorRanarrjoStalkCindanweSS9x12` (weekend) sits above five `SEFellmooreFarming*9x12` weekday packages (7), so the stalking package wins every day and the farming never runs.
  - `SEMazaddaFightMon/Wed/Sat20x2` likewise.
- **Date byte.** 75 packages author a date, such as the `ImpExRequest02`…`ImpExRequest27` family (12:00 for 5 h on the Nth of the month).

## Impact
Shivering Isles and Tamriel NPCs with weekly or monthly routines run the first-listed day variant every day, in its hour window. The intended activity for that day (farming, worship, home) is never reached in that window. This is behavioural, Oblivion-wide (about 2 % of actor bases) and persistent, but there is no crash and no data loss.

## Related
#6079 (FNV fix), ESM-2026-10-21-01 (Skyrim+ PSDT duration units, a different field), #3352 (zero-duration windows).

## Suggested Fix
1. Add `GameKind::Oblivion => Some(day_of_week(world, false, days_elapsed))` to `package_weekday`.
2. Decode PSDT byte 2 into `PackSchedule` and test it against the calendar's day of month. `GameClockGlobals::start_date` already holds Oblivion's authored `GameDay`/`GameMonth`.
3. Pin both with a test like `day_of_week_variants_select_by_the_game_calendar` that uses `GameKind::Oblivion`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
