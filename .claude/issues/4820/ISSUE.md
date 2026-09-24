# #4820 — GAME-D2-2026-09-24-04: `Enable()` at runtime on a ref that was disabled at cell load makes it interactive while it stays invisible and non-solid

**Labels**: low,gameplay,bug
**Filed from**: docs/audits/AUDIT_GAMEPLAY_2026-09-24.md

From `docs/audits/AUDIT_GAMEPLAY_2026-09-24.md` (HEAD `aabd99a05`).

- **Severity**: LOW
- **Dimension**: 2
- **Location**: `byroredux/src/cell_loader/spawn.rs:684-691` (the root and its door/lock payloads spawn before the disabled early return); `byroredux/src/interaction.rs:1222-1235` (the #4698 filter is read live)
- **Status**: NEW. This is the mirror of the documented #3278 limitation: there is no live re-spawn.
- **Description**: after `Enable()`, the root passes the live ledger filter, but it never received meshes or colliders. The player gets an invisible Open or Take prompt at the 24 BU fallback sphere, and an invisible door that still transitions.
- **Trigger**: a script-disabled ref is reloaded, then a quest stage `Enable()`s it while its cell is resident.
- **Related**: #3278, #4698; GAME-D5-2026-09-24-01 (its fix makes this path much more common).
- **Suggested Fix**: until live re-spawn exists, also require a spawned-content marker in `populate_candidates`. Or re-run the placement spawn on the enable edge.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (both NPC spawn paths, sibling interaction kinds, other parked `ReferenceState` facts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved (no guard held across `world.get`/physics queries)
- [ ] **SAVE**: If a saved shape changes, `FORMAT_MAJOR` is bumped and no `serde(default)` is added (#4465)
- [ ] **TESTS**: A regression test pins this specific fix
