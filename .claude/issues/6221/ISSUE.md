# #6221: SCR-2026-10-21-07: Four ObScript commands answer with constants for state the engine does not read, instead of declining

**Labels**: medium,scripting,bug,game:oblivion,game:fo3,game:fnv

**Source**: docs/audits/AUDIT_SCRIPTING_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Legacy ObScript · **Location**: `crates/scripting/src/obscript_more/tail.rs:460-463` (`IsGuard` 0x107d → `flag(false)`), `:458` (`GetIsPlayerBirthsign` 0x10e0 → `false`), `:573-579` (`IsPlayerActionActive` 0x1170 → `false` for indices 1..=10 except 6); `crates/scripting/src/obscript_more/state.rs:357-360` (`GetAnimAction` 0x10db → 2 for any armed combatant, else -1)

**Audit status**: NEW. `IsGuard` landed in `5d0891e78`, a commit mislabelled "Add physics and renderer audits for 2026-10-18". None of the four is in the `scripting.md` M47.3 tail table.

**Untrusted-Input**: No

## Description

The Dim 6 contract says anything unmodelled is "a traced no-op returning 0, counted per owner in `ObScriptQuestTimers.unknown_commands`, never faked". `obscript_more` "answers only from a named page, engine state or the ledger".
- `IsGuard`: the CS page reads the CLAS Guard flag, and the Oblivion CLAS flags are parsed (`crates/plugin/src/esm/records/actor/class.rs:55-67`, `:129`). The answer exists in the data but is not read.
- `GetIsPlayerBirthsign`: answers for a player who always has a birthsign.
- `IsPlayerActionActive`: answers "not attacking / blocking / …" for actions nobody tracks.
- `GetAnimAction`: invents ATTACK (2) for every armed actor in combat.

`HasBeenEaten` and `GetCrimeKnown` → `false` are defensible: the engine's world cannot produce that state.

## Evidence

the arms above. Three return the same number a decline would (0), but they bypass the `unknown_commands` census that `obscript_tools` uses to measure coverage. `GetAnimAction` returns 2 or -1 where a decline returns 0.

## Impact

`if GetAnimAction == 2` fires every frame for any armed combatant. The coverage census overstates what runs.

## Related

`feedback_no_guessing`; #6139 (structure of the same command chain).

## Suggested Fix

Read `flags_oblivion` for `IsGuard` (bit per UESP *CLAS*). Return `Action::Declined` for the birthsign, the untracked action indices and `GetAnimAction` outside a tracked attack state.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other constant answers in obscript_more/tail.rs and state.rs)
- [ ] **TESTS**: A regression test pins this specific fix
