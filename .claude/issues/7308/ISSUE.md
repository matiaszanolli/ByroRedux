# #7308: SCR-D5-02: #7115's `FollowUpTopics` cache has no test; a missing resource makes every follow-up silently never fire

- **Severity**: LOW
- **Dimension**: Scene/Package/Cinematic (dialogue)
- **Untrusted-Input**: No
- **Location**: `byroredux/src/systems/npc_dialogue.rs:738-763` (`FollowUpTopics`, `follow_up_topic`) and `:771-826` (`npc_dialogue_follow_up`)
- **Status**: NEW (test gap; the fix itself checks out, see Verified)
- **Description**: The cache's own invariants are the Weak keying (`Weak::ptr_eq`, rebuild on index swap) and agreement with the uncached `values().find_map` scan. Neither is pinned. `rg 'follow_up|FollowUp' byroredux/src` finds only the system, the row declaration, the init and the registry allow-list entry. The `tests` module at `:1197` has no case for `npc_dialogue_follow_up` or `follow_up_topic`. The only exercise is the live Doc Mitchell route (smoke dt1). `follow_up_topic` does `world.try_resource::<FollowUpTopics>()?`, so a world without the resource (a harness that does not go through `build_world`, which does `init_resource` at `boot/world.rs:466`) answers `None` and the loop `continue`s past every follow-up with no log.
- **Evidence**: `let cache = world.try_resource::<FollowUpTopics>()?;` (`npc_dialogue.rs:750`)
- **Impact**: A future change to the keying (for example a clone of the index) or a regression in the `entry().or_insert` order would silently stop the follow-ups (FNV's opening Pip-Boy and gear hand-out). Nothing but the live smoke would notice.
- **Related**: #7115, #7114
- **Suggested Fix**: Add two unit tests in `npc_dialogue.rs`: (1) two indexes swapped through `LoadedCellIndex` rebuild the map, and a follow-up INFO moved between DIALs resolves to the new owner; (2) `follow_up_topic` equals the old scan for every INFO of a small multi-DIAL index. Optionally log once when the resource is missing.

**Found by**: `docs/audits/AUDIT_SCRIPTING_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other latent / suspending paths, the other drain owners, the other ObScript hosts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the `apply_effect` lock inventory is updated
- [ ] **TESTS**: A regression test pins this specific fix
