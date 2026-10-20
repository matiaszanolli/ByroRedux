# TD6-2026-10-19-01: `PlayingScriptedIdle` is never inserted, so ObScript `IsIdlePlaying` (0x1070) always answers 0; its doc says the app's idle runtime adds it

Labels: low,scripting,gameplay,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW
- **Dimension**: 6 Stub & Placeholder Implementations
- **Location**: `crates/scripting/src/obscript_state.rs:377-383` (component + doc), `crates/scripting/src/obscript_more.rs:506-509` (the only reader); would-be writer sites `byroredux/src/systems/cinematic.rs:~101-109` (IdleResume insert) and `:~209` (IdleResume remove)
- **Age**: `6e9d55aa9`, 2026-10-18 (M47.3 close-out; ROADMAP M47.3 row says "Shipped")
- **Effort**: trivial
- **Status**: NEW
- **Description**: The doc says "the app's idle runtime adds it when the idle starts and removes it when the actor goes back to what it was doing". No code in `byroredux/` or `crates/` constructs or inserts it (`rg -nw PlayingScriptedIdle` -> the component def, `register`, the `has::<>` read at `obscript_more.rs:508`, and a save-ledger string). `IsIdlePlaying` therefore reads 0 on every actor even mid-`PlayIdle`. It is registered in the save ledger as "session-local like the IdleResume it follows", so the ledger entry also describes a writer that does not exist.
- **Evidence**: `rg -nw PlayingScriptedIdle crates byroredux --glob '*.rs'` (non-test): 4 lines, zero writes. `IdleResume` (the intended twin, `components/ai_anim.rs:35`) is written at `systems/cinematic.rs` where `PlayIdle` one-shots are installed and removed in `idle_resume_system`.
- **Proposed fix**: In `cinematic.rs`, insert `PlayingScriptedIdle` alongside each `IdleResume` insert and remove it where `idle_resume_system` removes `IdleResume` (same `done` list), or, if the getter is not wanted, drop the component and make 0x1070 decline. Add a one-line test that `IsIdlePlaying` flips during a one-shot.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
