# TD1-2026-10-19-01: The M47.3 ObScript command surface is a chain of six `match cmd` functions (150-780 lines each, ~270 arms), each ending `_ => next::run(..)`

Labels: low,scripting,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity (match arms > 50, fn > 200)
- **Location**: `crates/scripting/src/obscript_effects.rs:301` (`run`, 780 lines, 63 arms) then `obscript_more.rs:296` (233 / 24) -> `obscript_more/state.rs:19` (435 / 69, falls through at L451) -> `actions.rs:11` (260 / 26, L268) -> `extras.rs:157` (157 / 15, L311) -> `tail.rs:89` (483 / 71, nest 7); plus `obscript_quests.rs:481` `call` (359 / 28) and `obscript_effects.rs:1228` `getter` (310 / 36)
- **Status**: NEW (the five-file chain did not exist at the 10-15 snapshot; `obscript_more/` landed 2026-10-18, 21 commits touch these files in two days)
- **Effort**: medium
- **Description**: each command id is dispatched by trying up to six `match` blocks in a fixed order, so which function owns a command is only discoverable by reading the chain; a command added to an earlier block silently shadows a later one (e.g. `state.rs:259` forwards `0x1170 if !fallout` back into `obscript_effects::run`, `tail.rs:460` and `extras.rs:300` call `obscript_actions::run` / `obscript_effects::run` with synthetic ids). `obscript_effects::run` also opens with a 30-entry `matches!(cmd, CMD_SET_ENEMY | CMD_SET_ALLY | ...)` (`needs_caller`) that restates, per command, a fact the arm already knows. This is the shape the 10-15 report fixed on the Papyrus side by family files, re-created in a flat form on the ObScript side.
- **Evidence**: `fnscan.py` rows above; `sed -n 296,330p obscript_effects.rs`; `grep -n '_ => super::' obscript_more/*.rs` shows the five fall-through links.
- **Proposed fix**: one row table, `static COMMANDS: &[Command { id: u16, games: Games, needs_caller: bool, handler: fn(&World, &[ObScriptValue], &Context) -> Action }]`, built from per-family files (`obscript/cmd/{inventory,combat,faction_perk,lock_open,world_pos,player,ui_message,quest,state_query,ledger,...}.rs`, one `fn` per command or small family; the bodies are already 5-40 lines), looked up by sorted-slice `binary_search_by_key` or a `HashMap<u16, _>` built once; `handles()` (obscript_effects.rs:117) becomes a table lookup. Order-dependence disappears and a duplicate id becomes a unit test over the table. Convert in chain order so `obscript_more/tests.rs` (683 lines) stays the safety net. Re-check `include_str!` readers of the `obscript_*` files before moving text (none seen at this snapshot).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
