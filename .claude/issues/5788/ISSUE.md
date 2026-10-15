# OBL-D5-2026-10-14-02: the `obscript_quests.rs` module doc still documents Message / MessageBox / GetButtonPressed as phase-1 stubs

**Labels**: low,game:oblivion,legacy-compat,scripting,documentation,doc-rot
**Source**: docs/audits/AUDIT_OBLIVION_2026-10-14.md

Found by `/audit-oblivion` (docs/audits/AUDIT_OBLIVION_2026-10-14.md, HEAD `adbd468e2`). Finding ID: `OBL-D5-2026-10-14-02`. Re-verified against current code before filing.

- **Severity**: LOW (doc rot)
- **Dimension**: 5
- **Location**: module header of `crates/scripting/src/obscript_quests.rs` (command table rows `0x1059 / 0x1000` and `0x101f`)
- **Status**: NEW
- **Description**: the table says "Message / MessageBox → effect-log entry (HUD is later work)" and "GetButtonPressed → -1 (no message menu this phase)". `5182f7817` made both real (`QuestCommandHost::message` → `show_obscript_message`; `0x101f` → `take_button`); the `0x1059 | 0x1000` arm comment was updated but the header was not. The locals persistence from `f67d82b3c` is not mentioned either.
- **Evidence**: `sed -n 1,25p crates/scripting/src/obscript_quests.rs` against `fn message` and the `0x101f =>` arm.
- **Impact**: misleads the next reader about what a vanilla quest script's `MessageBox` does today (it now raises a native, blocking page).
- **Suggested Fix**: update the two table rows and add a `GetQuestVariable` / `ObScriptQuestLocals` row.

## Completeness Checks
- [ ] **SIBLING**: Neighbouring docs/comments checked for the same drift
