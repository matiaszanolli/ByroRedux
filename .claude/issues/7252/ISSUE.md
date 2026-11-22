# #7252: INC-2026-11-22-11: The #7230 drain resets `PackageDayBlocks` after the cell reload, wiping the loaded session's own spawn-time Once Per Day notes

**Labels**: low,gameplay,ai,save-load,bug

From `docs/audits/AUDIT_INCREMENTAL_2026-11-22.md` (report ID **IA-C-02**), audited over `2b40f76b6..f9f65373a`.

- **Severity**: LOW
- **Dimension**: gameplay Dim 5 / save Dim 5 (drain ordering)
- **Location**: `byroredux/src/save_io/live_load.rs:480-500` (`reset_resource::<PackageDayBlocks>`), called at `:787` after `reload_*_session`; recorder `byroredux/src/npc_spawn/ai_package.rs:1211-1214` (`note_selected` at spawn)
- **Changed in**: `save_io/live_load.rs` / 48c6ea693 (#7230)
- **Status**: NEW
- **Description**: The map is keyed by the outgoing session's EntityIds, so a reset is right; but `discard_outgoing_session_requests` runs after the reload (the test `live_load_drain_discards_the_outgoing_sessions_requests` pins `reload < discard < pose`). `apply_ai_package_behavior` calls `package_flags::note_selected` at spawn for the winner of an Once Per Day package, so actors spawned by the reload lose that block while a cold start keeps it. Whether an actor loses it depends on whether its spawn finalized before the drain (spawn is resumable and can span frames).
- **Evidence**: `once_per_day_blocked` excludes only `current != Some(package)`; with the block gone, the package can win again inside its 24 hours once it stops being the running one.
- **Impact**: Minor: an Once Per Day package may repeat within 24 game hours for actors that were already running it at load. No data loss.
- **Related**: #7230 (closed), AIP-FLG-001
- **Suggested Fix**: Prune only entries whose actor is no longer live (`world.is_alive` / spawned-in-this-session check) instead of replacing the whole map, or reset before the reload.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
