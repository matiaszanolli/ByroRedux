# SCR-D5-2026-10-17-05: A FromEvent alias fill ignores the Reserves / Allow Reserved rule

Issue: #6006
Labels: low, scripting, bug, quests

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D5-2026-10-17-05: A FromEvent alias fill ignores the Reserves / Allow Reserved rule

- **Severity**: LOW
- **Dimension**: Scene/Package/Cinematic
- **Location**: `crates/scripting/src/scene/quest_alias.rs:1017-1090` (FromEvent branch checks `used`/Allow Reuse and Dead, never `reserved`) vs `:1099-1107` (`eligible` honours `reserved`/`allow_reserved`)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: CK *Quest Alias Tab* (ck-uesp-wiki line 33): once a reference is in a "Reserves Reference" alias the Story Manager fills no other alias with it unless the new alias is Allow Reserved or is a From External fill (From Event is not an exception). The candidate path enforces it through the shared `reserved` set; the early-`continue` FromEvent branch never consults it, nor does `event_aliases_fill` at quest-selection time. An event slot naming a reserved reference double-books it.
- **Impact**: rare double-booking of a reserved actor by an event-started quest; quest pre-emption semantics differ from the CK.
- **Suggested Fix**: apply `!allow_reserved && reserved.contains(&entity)` to the FromEvent branch before binding.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
