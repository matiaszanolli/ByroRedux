# #5870: GAME-D5-2026-10-15-03: `ActivePackages` is never retired on death or unload

- **Labels**: low,bug,gameplay,ai,scripting
- **Filed from**: `docs/audits/AUDIT_GAMEPLAY_2026-10-15.md`

- **Location**: `byroredux/src/combat.rs:963-1003` (removes `AmbientPackageRuntime` with no transition report); reader at the VM's `GetCurrentPackage` (`crates/scripting/src/fragment/vm.rs:2939`)
- **Status**: NEW
- **Description**: `GetCurrentPackage` on a corpse returns its last package for the rest of the session, but None after a reload. The map grows for the session. Package End fragments also do not run on death.
- **Suggested Fix**: Report a `→ None` transition from `reconcile_dead_actor` and from unload.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **LOCK_ORDER**: If a RwLock/query scope changes, TypeId-sorted acquisition and the system's declared `Access` row are preserved
- [ ] **TESTS**: A regression test pins this specific fix
