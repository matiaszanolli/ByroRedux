# SCR-D2-2026-10-17-08: `ALIAS_OWNER_BASE` (0xFC00_0000) overlaps regular plugin slot 0xFC

Issue: #5988
Labels: low, scripting, bug

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D2-2026-10-17-08: `ALIAS_OWNER_BASE` (0xFC00_0000) overlaps regular plugin slot 0xFC

- **Severity**: LOW
- **Dimension**: Fragment Dispatch & Locks
- **Location**: `crates/scripting/src/fragment/alias_scripts.rs:26-28,262-264`; slot map `crates/scripting/src/load_order.rs:84-97`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The doc says the alias-owner range "is no plugin's" and `is_alias_owner` tests only `owner >> 24 == 0xFC`. `global_slot_of` treats `0xFE` as light and `0xFD` as Starfield medium, so `Regular(0xFC)` (the 253rd full plugin) is a valid slot. A reference or quest FormID in that slot would be classified as an alias owner by `owners.rs`/`reference.rs` (`standing`, `vmad`, `alias_owner_*`) and resolve to `None`/stopped.
- **Evidence**: `owner >> 24 == ALIAS_OWNER_BASE >> 24` with no check against the loaded slot table.
- **Impact**: Only load orders with >= 253 full (non-ESL) plugins; scripts of that plugin lose events or run as aliases. Narrow.
- **Related**: none.
- **Suggested Fix**: Move the synthetic range above the real slot space (e.g. keep a separate `OwnerId` enum/flag bit) or assert at boot that no `Regular(0xFC)` slot is allocated.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
