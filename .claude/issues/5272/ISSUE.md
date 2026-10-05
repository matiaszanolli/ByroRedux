# #5272: SAFE-D3-2026-10-05-01: `PhysicsWorld::remove_body` prunes `body_labels` but not its #5161/#5246 siblings `explosion_offences` and `keyframe_refusals_logged`, which grow for the whole session

**Labels**: low,safety,physics,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5272

**Source**: `docs/audits/AUDIT_SAFETY_2026-10-05.md` — `SAFE-D3-2026-10-05-01` (HEAD `a2c24b16e`)

- **Severity**: LOW. This is CPU-side unbounded growth, keyed per event rather than per frame. Each entry is 8–12 B plus hash overhead, and an entry is added only when a body refuses a keyframe target or explodes.
- **Dimension**: Memory & Resource Leaks
- **Location**: `crates/physics/src/world.rs`:
  - fields `keyframe_refusals_logged` (`:315`, inserted at `:905`) and `explosion_offences` (`:343`, `entry(handle)` at `:1025`);
  - `remove_body` at `:536-560`, which prunes only `body_labels` (`:551`).

  Introduced by `44f7bab55` / `5ae7f8ad4` (#5161) and `e8de9f8c8` (#5246).
- **Status**: NEW. Searched `explosion_offences` and `keyframe_refusals_logged`: nothing found. Today's `/audit-physics` did not run in this suite.
- **Description**:
  - #5161 added three `RigidBodyHandle`-keyed diagnostics containers in one change. It documented and implemented the removal of exactly one of them: `body_labels` is "Dropped in `Self::remove_body`… the map must not accumulate one stale entry per despawned ragdoll bone".
  - The other two are never pruned, cleared or shrunk anywhere in the crate. `PhysicsWorld` is inserted once at boot (`byroredux/src/boot/world.rs`), so their lifetime is the session.
  - Every production removal path goes through `remove_body`, so it is the single place to fix: `cell_loader/unload.rs:788`, `ragdoll.rs:527`, `npc_spawn/loot_appearance.rs:187`, `crates/physics/src/ragdoll.rs:865` and `commands/ragdoll_status.rs:170`.
  - Keys are full handles (index + generation), so a stale entry can never alias a new body. The only cost is growth; correctness is unaffected.
- **Evidence**:
  ```rust
  if removed {
      // #5161 — the evidence-label dies with the body; the map must
      // not accumulate one stale entry per despawned ragdoll bone.
      self.body_labels.remove(&handle);
      self.wake();
      self.colliders_dirty = true;
  }
  ```
  `explosion_offences` is "deliberately never cleared on clean substeps" (`:335-342`). That rationale is about the escalation ladder for a *live* body, not about a removed one.
- **Impact**: There is no functional impact, only slow growth over a long session that despawns many exploding or refusing ragdolls. The FNV `SLscorpionBurrowINT` repro is the kind of content that drives it. The "Dim 3 CPU unbounded growth" rule asks for this class to be reported.
- **Related**: #5161, #5246, #4772.
- **Suggested Fix**: In `remove_body`'s `if removed` arm, also run `self.explosion_offences.remove(&handle)` and `self.keyframe_refusals_logged.remove(&handle)`. Extend the existing #5161 removal test to assert all three maps are empty after removal.

## Publisher note

Sibling: **PHYS-D2-2026-10-05-03** (`AUDIT_PHYSICS_2026-10-05.md`, filed separately) — `PhysicsWorld::articulation_joints` is likewise never pruned. Both can be fixed in the same `remove_body` arm.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **TESTS**: A regression test pins this specific fix
