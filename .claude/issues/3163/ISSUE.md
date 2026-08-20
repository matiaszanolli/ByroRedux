# SAVE-D6-2026-08-20-01: a mid-column apply_deltas failure leaves the world partially overlaid after the irreversible teardown, skips the dead-actor reconciler (silently reverting #3022), and reports only to the log

**Issue**: #3163 — https://github.com/matiaszanolli/ByroRedux/issues/3163
**Finding ID**: `SAVE-D6-2026-08-20-01`
**Severity**: HIGH
**Dimension**: 6 — M45.1 Live Load-Apply
**Audit**: `/audit-save` — 2026-08-20 comprehensive suite, HEAD `bb0b92f2`
**Labels**: high, ecs, bug

---

**Audit**: `/audit-save` — `docs/audits/AUDIT_SAVE_2026-08-20.md` (HEAD `bb0b92f2`)
**Finding ID**: `SAVE-D6-2026-08-20-01`
**Severity**: HIGH
**Dimension**: 6 — M45.1 Live Load-Apply
**Data-Loss Class**: corruption-on-load

## Location

- `crates/save/src/driver.rs:273-291` — `apply_deltas`, the per-column `?`
- `byroredux/src/save_io.rs:1012-1029` — the drain's `match` (the `Ok`/`Err` arms)
- `byroredux/src/save_io.rs:964-968` — the teardown that has already run by then
- `byroredux/src/save_io.rs:84-130` — `MUTABLE_DELTA_COLUMNS`, the apply order

## Description

`apply_deltas` iterates `MUTABLE_DELTA_COLUMNS` in declaration order and propagates the first
`SaveError` with `?`:

```rust
// crates/save/src/driver.rs:281-289
for &name in columns {
    let (Some(value), Some(apply)) = (
        snapshot.components.get(name),
        registry.component_apply(name),
    ) else { continue; };
    applied += apply(world, value.clone(), remap)?;
}
```

Columns *before* the failing one have already mutated the world through `insert_batch`. There
is no transaction, no dry run, and no rollback — and by the time `apply_deltas` is called,
`drain_streaming_state` + `unload_current_interior` + `load_cell_with_masters` have all already
run, so the caller has nothing to fall back to either.

The caller's handling makes it worse in three specific ways:

1. **The `Err` arm logs and falls through.** It is `log::error!("save load: delta apply
   failed: {e}")` with no `return` — `validate_world`, `validate_form_ids`,
   `validate_cinematic_entity_refs` and `apply_player_pose` all still run
   (`save_io.rs:1031-1058`), so the session ends up **positioned and playable in a
   half-overlaid world** rather than visibly broken.
2. **It silently reverts #3022 for that load.** `reconcile_dead_actor_runtime_state(world)`
   sits in the **`Ok` arm only** (`save_io.rs:1014`). On a failure, `Dead` may already have
   been overlaid (it is column 11 of 20) while the AI/animation teardown that marker is
   supposed to imply never runs — **the exact inconsistency #3022 was filed to remove, and its
   fix is bypassed for that load with no signal that it was.**
3. **The log line is now the only trace.** With `SAVE-D4-2026-08-20-01`, a player-facing F9
   that lands here produces nothing observable at all.

Column order makes the blast radius concrete: `RigidBodyData` is 19th of 20, so a failure there
applies `Transform`, `Inventory`, `EquipmentSlots`, `ActorValues`, `EquippedWeapon`, `Dead` and
twelve more, then drops `RumbleOnActivate` **and** the reconciler.

## Evidence

- `crates/save/src/driver.rs:288` — the `?` inside the loop, with no accumulated-rollback state.
- `byroredux/src/save_io.rs:1013-1026` — the `Ok` arm holds
  `let dead = crate::combat::reconcile_dead_actor_runtime_state(world);`; the `Err` arm
  (`:1027`) holds only the `log::error!`.
- `byroredux/src/save_io.rs:1031-1058` — execution continues into `validate_world` and
  `apply_player_pose` regardless of which arm ran.
- **The trigger is reachable at HEAD** via `SAVE-D2-2026-08-20-01`:
  `RigidBodyData.collidable` is a required field added with no `FORMAT_MAJOR` bump, so a `v4`
  snapshot written 2026-08-18 passes every container gate (magic, `major == 4`, fingerprint,
  CRC) and then fails `serde_json::from_value` on exactly that column.

## Impact

Every recoverable per-column deserialisation error becomes an **unrecoverable half-applied live
world**. This is the one place in `execute_pending_save_loads` that does not follow the
function's own established standard: `validate_cell_loadable` (#1697) exists precisely so a
foreseeable failure is detected *before* the destructive teardown, and the same reasoning
applies here — **every column's decodability is knowable from the snapshot alone, with no world
mutation required.**

## Related

- `SAVE-D2-2026-08-20-01` — the reachable trigger.
- **#1697** / `SAVE-D6-02` — the pre-flight precedent this should follow.
- **#3022** — CLOSED; the reconciler the `Err` path skips, silently reverting that fix for the load.
- `SAVE-D4-2026-08-20-01` — why the `log::error!` is now the only channel.
- **#1847** / `SAVE-04` — the additive-only overlay contract.

## Suggested Fix

Split `apply_deltas` into a **non-mutating decode pass** over every column in `columns`
(deserialise to the typed `Vec<(u32, T)>` and discard) followed by the existing apply pass, and
hoist the decode pass into `execute_pending_save_loads` **before** `drain_streaming_state` —
alongside `validate_cell_loadable`, whose slot in the sequence it shares.

Then move `reconcile_dead_actor_runtime_state` out of the `Ok` arm so it runs on **any** outcome
in which `Dead` rows were applied, and give the `Err` arm a `return` or an explicit
"world is half-applied" escalation rather than falling through into pose restore.

## Completeness Checks
- [ ] **SIBLING**: `restore_resources` (`save_io.rs:1007`) has the same shape — it *does* `return` on `Err`; keep the two arms' failure semantics consistent after the fix
- [ ] **LOCK_ORDER**: if the decode pass takes storage locks, TypeId-sorted acquisition is preserved and the pass stays off the scheduler lane
- [ ] **TESTS**: a regression test builds a snapshot whose Nth column fails `from_value` and asserts the world is untouched (pre-flight rejects before teardown)
- [ ] **TESTS**: a regression test pins that `reconcile_dead_actor_runtime_state` runs on the failure path when `Dead` rows were applied (#3022 stays closed on both arms)
