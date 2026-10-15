# #5868: GAME-D2-2026-10-15-04: Scripted, parent-relayed or default-processing activation of a load door does nothing

- **Labels**: low,bug,gameplay,scripting
- **Filed from**: `docs/audits/AUDIT_GAMEPLAY_2026-10-15.md`

- **Location**: `byroredux/src/interaction.rs:1494-1520`; `docs/engine/scripting.md:1737`
- **Status**: NEW (from `745d64830` and `e2a234c6e`)
- **Description**: Apart from the `door.*` console command, the only production `queue_door_transition` call is inside the player's E-press path. No `ActivateEvent` consumer handles `DoorTeleport`, yet `scripting.md` lists the door as one. A script `Activate` of a load door (or a lever wired to one) is a silent no-op.
- **Suggested Fix**: Add a `DoorTeleport` arm on `ActivateEvent` for `activator == player`, or correct the doc and log the decline.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **LOCK_ORDER**: If a RwLock/query scope changes, TypeId-sorted acquisition and the system's declared `Access` row are preserved
- [ ] **TESTS**: A regression test pins this specific fix
