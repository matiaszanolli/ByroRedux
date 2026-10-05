# #5267 — GAME-D4-2026-10-05-01: reference_state::restore runs the death teardown (ragdoll activation) synchronously at actor-job completion — the un-propagated-skeleton point #4814 queued apply_starts_dead to avoid

- **Labels**: medium,gameplay,combat,physics,save-load,bug
- **Filed from**: `docs/audits/AUDIT_GAMEPLAY_2026-10-05.md` (2026-10-05 comprehensive suite)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5267

- **Severity**: MEDIUM (a corpse renders or simulates from a wrong ragdoll seed; loot and `Dead` state are correct)
- **Dimension**: 4 — Combat & Death Pipeline (death reconciled once / correctly); also Dim 2 (loot persistence)
- **Location**:
  - `byroredux/src/cell_loader/reference_state.rs:297-300` (`if state.dead { world.insert(entity, Dead); reconcile_dead_actor(world, entity); }`)
  - versus `:341-355` (`apply_starts_dead`: "Queued rather than run here because the ragdoll seeds from bone `GlobalTransform`s, which a freshly spawned skeleton does not have in world space until the next PostUpdate propagation")
  - caller: `cell_loader/references/synth_child.rs:80-98` (`stamp_quest_reference` → `restore`), at the actor-job completion `references/mod.rs:746-766`
  - `byroredux/src/combat.rs:566-640` (`reconcile_dead_actor` → `activate_ragdoll`)
- **Status**: NEW. It predates the baseline: the dead branch is unchanged, but #4814 (`f87490826`) documented the hazard after this branch existed. It may relate to #4772 (open; FO3 restore's first ragdoll solve blows up), which is unconfirmed.
- **Trigger**: any game with ragdoll templates (FO3/FNV/Skyrim). Kill an NPC; its ragdoll settles. Leave the cell (door transition, or exterior stream-out), so `capture` parks `dead: true`. Then return. The respawned actor job completes, and if its skeleton import and completion land in the same budgeted call (`NpcSpawnJob::step` loops units until `budget.should_yield()`), the corpse is affected.
- **Description**:
  - New entities are spawned with `GlobalTransform::IDENTITY` (`scene/nif_loader.rs:1343`, `:1794`). Only the placement root is seeded with a world `GlobalTransform` (`npc_spawn/resumable/mod.rs:358`).
  - `restore` inserts `Dead` and calls `reconcile_dead_actor` immediately. That function calls `activate_ragdoll`, which seeds every body from `bone GT ∘ local offset` (`ragdoll.rs:338-345`).
  - On a skeleton that has not yet propagated, every bone sits at the origin. #5161's sanity gate (`crates/physics/src/ragdoll.rs:51-52, 98-117`) rejects only a seed more than 1e5 BU from the root, so within 100k BU of the origin the ragdoll is **built at world origin**; further away, the activation is rejected.
  - Either way, `reconcile_dead_actor` has already stripped the corpse's `AnimationPlayer`/`AnimationStack`. A rejected corpse stands in its bind pose.
  - Nothing retries the activation. `reconcile_dead_actor_runtime_state` runs propagation first, but only on a save load, and it would then report "ragdoll already active" for the origin case.
  - `apply_starts_dead`, called at the same completion point, deliberately routes through `queue_dead_actor_reconciliation` → Late `reconcile_pending_dead_actors_system`, which runs after PostUpdate propagation.
- **Evidence**:
  - The #4814 rationale comment quoted above.
  - The two producers sit side by side in the completion arm: `restore` (via `stamp_quest_reference`) and then `apply_starts_dead`.
  - `restore_resident` (`save_io.rs:1796`) also calls `restore` before the propagation inside `reconcile_dead_actor_runtime_state` (`combat.rs:655-672`).
  - The tests `empty_container_and_dead_actor_survive_repeated_evictions` and `starts_dead_actor_is_a_queued_corpse_at_completion` use fixtures without a `RagdollTemplate`, so neither can observe the seed.
- **Impact**: a revisited corpse appears at world origin, or stands upright, instead of lying where it fell. This happens on every revisit where the respawn completes in a single budgeted call. Looting is unaffected because `Dead`, `Inventory` and the collider owner live on the root. Whether a given respawn completes in a single call depends on the budget. I did not prove the frequency, because no engine was run.
- **Related**: #4814 (the queueing precedent); #4772 (open, ragdoll blow-up on FO3 restore); #5161 (the seed gate); GAME-D1-2026-10-05-01 (the same restore call also drives `reconcile_worn_gear`).
- **Suggested Fix**: in `restore`, insert `Dead` and call `crate::combat::queue_dead_actor_reconciliation(world, entity)` in place of the synchronous reconcile, mirroring `apply_starts_dead`. Pin it with a test that gives the restored actor a `RagdollTemplate` and asserts that no `RagdollActive` exists until the Late drain.

_Source: `AUDIT_GAMEPLAY_2026-10-05.md` (GAME-D4-2026-10-05-01), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
