# #5873: GAME-D4-2026-10-15-07: `OnTrapHitStop` events fire in `HashSet` iteration order

- **Labels**: low,bug,gameplay,combat
- **Filed from**: `docs/audits/AUDIT_GAMEPLAY_2026-10-15.md`

- **Location**: `byroredux/src/systems/trap_hits.rs:182`
- **Status**: NEW
- **Description**: The stop events are raised in `HashSet::difference` order, which is seeded per process, so script run order differs between runs.
- **Suggested Fix**: Sort the difference by FormID or entity before raising.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **LOCK_ORDER**: If a RwLock/query scope changes, TypeId-sorted acquisition and the system's declared `Access` row are preserved
- [ ] **TESTS**: A regression test pins this specific fix
