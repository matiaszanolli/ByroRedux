# CONC-D3-2026-08-30-01: `skin.dump` holds `SkinnedMesh` across `format_skin_dump`'s `GlobalTransform` read — the console half of #2388 was never fixed

**Issue**: #3648
**Labels**: bug, ecs, medium, concurrency
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md`

---

Source: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md` — CONC-D3-2026-08-30-01 (MEDIUM, D3 · ECS Lock Ordering & Deadlock).

**Location**: `byroredux/src/commands/assets.rs:712-715` (callee at `byroredux/src/commands/shared.rs:90-138`).

## Description

`docs/engine/ecs.md:600-604` fixes the process-wide order `... -> GlobalTransform -> SkinnedMesh -> MeshHandle -> ...`.

`SkinDumpCommand::execute` binds the `ComponentRef<SkinnedMesh>` returned by `world.get::<SkinnedMesh>` through a `let ... else`, so the read guard lives to the end of the function, and then calls `format_skin_dump`, which acquires `GlobalTransform` (and `Name`, `StringPool`) **per bone** with that guard still live. That is `SkinnedMesh -> GlobalTransform`, the **inverse** of the canonical order.

**#2388 fixed exactly this inversion** in the debug-server sibling `eval_inspect_skinned_mesh` — whose comment at `crates/debug-server/src/evaluator.rs:255-262` states the fix — but **the console-command sibling that goes through `format_skin_dump` was missed**.

## Evidence

```rust
// byroredux/src/commands/assets.rs:712-715 — SkinnedMesh guard lives past the call
let Some(skin) = world.get::<SkinnedMesh>(entity) else {
    return CommandOutput::line(format!("Entity {} has no SkinnedMesh component", entity));
};
let lines = format_skin_dump(world, entity, &skin);
```
```rust
// byroredux/src/commands/shared.rs:136-138 — inside that call, with `skin` still held
let world_mat = world
    .get::<GlobalTransform>(*bone_e)
    .map(|gt| gt.to_matrix());
```
```rust
// crates/debug-server/src/evaluator.rs:262-264 — the canonical direction, post-#2388
let gt_q = world.query::<GlobalTransform>();
let Some(skin_q) = world.query::<SkinnedMesh>() else { ... };
```

Opposing `GlobalTransform -> SkinnedMesh` edges exist at `byroredux/src/systems/bounds.rs:135-138` (`make_world_bound_propagation_system`, a `Stage::PostUpdate` `add_exclusive_with_access` system that runs **every frame**), `crates/debug-server/src/evaluator.rs:262-263` and `:349-350`, and `build_skinned_palettes` (`byroredux/src/render/skinned.rs`, named as the canonical establisher in `docs/engine/ecs.md:608-610`).

## Trigger Conditions

Debug build with `BYRO_LOCK_ORDER_CHECK=1`. Run any frame (so `make_world_bound_propagation_system` records `GlobalTransform -> SkinnedMesh`), then `skin.dump <id>` in `byro-dbg`. The console command records the reverse edge and `global_order::record_and_check` panics on whichever observation lands second. **Order of the two is irrelevant — the cycle closes either way.**

## Impact

No live deadlock today — every opposing site is either an exclusive system or main-thread render collection, so the hold periods cannot overlap. The concrete cost is the one `docs/engine/ecs.md:643-649` names: *"an inverted pair that is safe still aborts a debug build once both sites run."* `skin.dump` and `walk` both dispatch from the same `DebugDrainSystem`, so this is the literal #2388 reproduction with one command name changed.

It also erodes the invariant that would make a future promotion of `world_bound_propagation` to a parallel lane safe by construction.

## Related

#2388 (fixed the debug-server half), #3445, #3446, ECS-D1-01 in `docs/audits/AUDIT_ECS_2026-08-30.md` (same class).

## Suggested Fix

**Snapshot before acquiring** — clone the `SkinnedMesh` (or just its `bones` / `bind_inverses` / `skeleton_root` / `global_skin_transform`) into an owned local and **drop the guard** before calling `format_skin_dump`. `format_skin_dump` already takes `&SkinnedMesh`, so passing `&owned` is a two-line change with no behaviour difference.

## Completeness Checks
- [ ] **LOCK_ORDER**: The `SkinnedMesh` guard is *dropped*, not merely reordered — `format_skin_dump` also reaches `Name` and `StringPool`
- [ ] **SIBLING**: Every other console command routing through `byroredux/src/commands/shared.rs` helpers audited for the same held-guard-across-helper shape
- [ ] **TESTS**: `BYRO_LOCK_ORDER_CHECK=1` with a frame + `skin.dump` in the same process must not panic
