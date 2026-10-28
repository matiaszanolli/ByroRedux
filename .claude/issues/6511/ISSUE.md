# SCR-2026-10-27-02: `SendTrespassAlarm` reports done with no effect

Labels: low, scripting, legacy-compat, game:fnv, bug
Source: docs/audits/AUDIT_SCRIPTING_2026-10-27.md

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-10-27.md` finding `SCR-2026-10-27-02` (LOW)
**Location (re-mapped to current tree)**: `crates/scripting/src/obscript_more/state.rs` (`cmd_115c`, ~:159-164; row ~:659, `Games::Both`)

**Severity**: LOW. Dimension: Legacy ObScript. Game: FNV (FO3 shares the row). Untrusted input: No.

## Description
The GECK says *SendTrespassAlarm* "Sends out a trespass alarm as if someone had reported the player trespassing." The handler returns `done("SendTrespassAlarm")` and changes nothing. Its comment ("a trespass crime nobody records") predates `CrimeKnowledge` (1a67c9f30), which carries `CrimeKind::Trespass`, and the guard rush. Under the Dim 6 ledger-honesty rule (#6190), a command with no effect should decline and be counted, not report done.

## Evidence
FalloutNV.esm has six calls: Gomorrah `vGOMAlarmRef.SendTrespassAlarm Player`; Ultra-Luxe `VMS18MarjorieREF`, `BennyREF` and the cashier trigger (each on a drawn weapon); the Atomic Wrangler trigger; the Camp Golf score terminal.

## Impact
Casino security never reacts to these alarms, and the `unknown_commands` census hides the gap.

## Related
#6190, 1a67c9f30 (`CrimeKnowledge`), #6427.

## Suggested Fix
Return `Action::Declined` until trespass is modelled, then route it into the crime runtime (`CrimeKind::Trespass`).

## Completeness Checks
- [ ] **SIBLING**: Other `done(..)`-returning no-effect rows in the command table checked
- [ ] **TESTS**: A regression test pins this specific fix

