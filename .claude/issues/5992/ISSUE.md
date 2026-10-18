# SCR-D3-2026-10-17-04: `GetTriggerObjectCount` returns a wrong count (excludes the player) instead of declining

Issue: #5992
Labels: medium, scripting, bug

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D3-2026-10-17-04: `GetTriggerObjectCount` returns a wrong count (excludes the player) instead of declining

- **Severity**: MEDIUM
- **Dimension**: Event Runtime
- **Location**: `crates/scripting/src/trigger.rs:145-149` (`trigger_object_count`), `:229` (candidate set excludes the player);
  VM native `fragment/vm/natives/reference.rs:126-129`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The count is `|{(trigger, actor)}|` from `TriggerOccupancyState.inside`, whose actor set is `SceneAliasCandidate` entities
  **minus the player** (`*entity != player`, `:229`); the player's occupancy lives only in `TriggerVolume.occupant_inside`. The CK says "the number of
  objects currently inside this trigger volume" (any object). The native returns the number as a normal `Int` rather than stopping the run (the VM
  contract is "a native that cannot honour its values stops the run"). Secondary: when the candidate list is empty the occupancy pass is skipped
  entirely (`if !actors.is_empty()`, `:254`), so `inside` keeps stale pairs for actors that despawned and the count stays > 0 until another candidate
  exists.
- **Evidence**: `trigger.rs:229` filter, `:254-345` pass gated on `!actors.is_empty()`, `:651`/test `:687` only exercises actor pairs.
- **Impact**: a script gating on `GetTriggerObjectCount() == 0` / `> 0` (trap reset, "area is empty") reads 0 while the player stands in the volume
  and a stale non-zero after the last occupant unloads.
- **Related**: `3cb6056e0` (introduced), docs/engine/scripting.md:1272.
- **Suggested Fix**: add `occupant_inside == Some(true)` of the volume to the count, clear `inside`/`observed_*` when the candidate list is empty, or
  stop the VM run when the occupancy pass has not covered the player.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
