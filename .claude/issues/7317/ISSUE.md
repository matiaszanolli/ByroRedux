# #7317 ECS-D4-01: `PackageDayBlocks` is keyed by `EntityId`, so a Once Per Day block is lost when streaming respawns the actor

Labels: low,ecs,gameplay,ai,bug

From `docs/audits/AUDIT_ECS_2026-11-22.md` (HEAD e2a4c54cc; re-verified at f5329b2f5).

- **Severity**: LOW
- **Dimension**: 4 Resources (resource vs component), and 7 (load/unload symmetry). The Dim 4 and Dim 7 agents reported it independently, as D4-01 and D7-02.
- **Location**: `byroredux/src/npc_spawn/package_flags.rs` `PackageDayBlocks(HashMap<(EntityId, u32), f64>)`, `once_per_day_blocked`, `note_selected`. `byroredux/src/cell_loader/stream_snapshot.rs` `ActorStreamSnapshot` does not carry the block.
- **Status**: NEW. #7252 (closed) fixed the live-load reset ordering, not the keying.
- **Description**: the block is keyed by the actor's `EntityId`. Exterior streaming despawns the actor, and on respawn it gets a fresh id, while the snapshot carries its FormID-keyed state but not the block. A Once Per Day package can then win again inside the 24 game hours, contrary to AIP-FLG-001 (`docs/prd/ai-packages-completion.md`). `NOT_SAVED_BY_DESIGN` covers save/load, not streaming. The old entry is pruned lazily, so there is no leak.
- **Impact**: Once Per Day is enforced only while the actor's entity is alive, which is a gameplay-fidelity gap.
- **Suggested Fix**: key by the actor's FormID (as `ActorRelocations` does), or carry the block in `ActorStreamSnapshot`. Add a despawn/respawn test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other multi-storage gathers / other rows / other producers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, the process-wide order (and TypeId-sorted pair acquisition) is preserved; the `BYRO_LOCK_ORDER_CHECK=1` lanes (core, scripting, bin) stay green
- [ ] **TESTS**: A regression test pins this specific fix (for a lock-order fix: a fixture that holds every storage involved, so the detector lane can see the edge)
