# SCR-D6-2026-10-17-02: `MessageBoxQueue` has no bound, unlike every sibling presentation queue

Issue: #6012
Labels: medium, scripting, bug

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D6-2026-10-17-02: `MessageBoxQueue` has no bound, unlike every sibling presentation queue

- **Severity**: MEDIUM
- **Dimension**: Legacy ObScript
- **Location**: `byroredux/src/notifications.rs:30-56` (`MessageBoxQueue`, `route_script_messages` `push_back`); producers `obscript_quests.rs:213-216`
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: `PlayerNotifications` caps at 8 (`MAX_PENDING`) and `ObScriptEffectLog` at 64, but boxes are `push_back`-ed unconditionally. Boxes drain only when the player answers (`app_frame.rs:884-895`: while any page is open the queue just waits; with no UI it is dismissed). An unconditional `MessageBox` in a quest GameMode block (common mod bug; the real engine blocks, here nothing pauses the 5 s tick) enqueues one box per statement per tick, each with up to 64 KB text plus buttons, until the player clears them one by one.
- **Impact**: Unbounded memory growth and an unwinnable modal chain on a buggy/hostile mod; defense-in-depth gap.
- **Suggested Fix**: Cap the queue (drop-oldest or refuse when full, with a log) and optionally suppress a new box while one from the same quest is already pending.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
