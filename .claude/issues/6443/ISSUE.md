# ECS-2026-10-27-01: apply_pending_group_swaps re-enters AnimationPlayer under the PendingGroupSwap guard

Labels: high, ecs, concurrency, animation, bug
Source: docs/audits/AUDIT_ECS_2026-10-27.md

**Source**: `docs/audits/AUDIT_ECS_2026-10-27.md` finding `ECS-2026-10-27-01` (Dimension 1, lock ordering) · Severity HIGH · Status NEW (introduced by 92173d61f, #4128 `PlayGroup` flag 0).

**Location**: `crates/scripting/src/obscript_anim.rs:185-212` (`apply_pending_group_swaps` collect block), `:84-94` (`sequence_player`).

## Description
The collect block opens `players = world.query::<AnimationPlayer>()` and then `pendings = world.query::<PendingGroupSwap>()`, which records the edge `AnimationPlayer -> PendingGroupSwap`. Inside `for (object, pending) in pendings.iter()` it calls `sequence_player(world, object)`. That function takes `NamedSequences` (via `world.get`) and then `world.query::<AnimationPlayer>()` again, while `PendingGroupSwap` is still held. This records `PendingGroupSwap -> AnimationPlayer` and a recursive read of `AnimationPlayer`. The block comment above says it scopes reads to avoid the read/write self-deadlock, but the helper call inside the loop reintroduces the nested acquisition.

## Evidence
```rust
let Some(players) = world.query::<AnimationPlayer>() else { return; };
let Some(pendings) = world.query::<PendingGroupSwap>() else { return; };
for (object, pending) in pendings.iter() {
    let Some(player_entity) = sequence_player(world, object) else { continue; };   // re-queries AnimationPlayer
    let Some(player) = players.get(player_entity) else { continue; };
```
`BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux-scripting`: 1278 passed, 2 failed. The detector panics in `obscript_anim::tests::a_held_request_applies_at_a_loop_wrap_and_the_latest_wins` and `flag_zero_waits_for_the_cycle_end_before_the_swap`: "attempted acquisition of `AnimationPlayer` while holding `PendingGroupSwap` … `AnimationPlayer` -> `PendingGroupSwap` -> `AnimationPlayer`". The same two failures appear in CI at `c7463c04d` and at `fa6be4a6b` (the ABBA lock-order lane has been red on `main` for 29 runs).

## Impact
The system is an Update exclusive, so no live cross-thread deadlock is reachable today. But under a writer-preferring `RwLock`, a recursive read of the same storage blocks once a writer queues. The red lane also hides every later inversion.

## Related
ECS-2026-10-27-02, ECS-2026-10-27-03 (same CI lane), #4128, #6253.

## Suggested Fix
Snapshot the pending list first: `(object, clip_handle)` pairs, with the guard dropped. Then resolve each object's `sequence_player` and read the player with no other guard held. Alternatively, have `sequence_player` take the already-held `&QueryRead<AnimationPlayer>`.

## Completeness Checks
- [ ] **SIBLING**: Other `obscript_anim.rs` helpers that call `sequence_player` / re-query `AnimationPlayer` under a held guard
- [ ] **LOCK_ORDER**: TypeId-sorted acquisition preserved; no nested acquisition remains in the collect block
- [ ] **TESTS**: `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux-scripting` is green (the two failing tests pass)

