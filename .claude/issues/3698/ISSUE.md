# #3698 — ECS-2026-08-30-P2-02: collect_candidates holds the scratch write guard across five component reads, every frame

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: MEDIUM · **Dimension**: P2 Gameplay Slice / Lock Ordering
**Location**: `byroredux/src/interaction.rs` (`collect_candidates` ~:863-873; helper `populate_candidates` ~:875-924)
**Source**: `docs/audits/AUDIT_ECS_2026-08-30.md` (ECS-P2-02, `[P2-gameplay]`)

> **Coverage note**: `byroredux/src/interaction.rs` has no owner audit skill. This finding comes from the `/audit-ecs` run's explicit P2-gameplay slice sweep and is the only audit coverage that file received.

## Description

`collect_candidates` opens a `ResourceWrite<InteractionCandidateScratch>` and calls `populate_candidates` *through* it; that helper then acquires five component read guards (`DoorTeleport`, `RumbleOnActivate`, `QuestAdvanceOnActivate`, `TwoStateActivator`, `MG07LabyrinthianDoor`) with the write guard still held. Same class as the `combat.rs` cooldown site, but on the per-frame path rather than an attack edge.

## Evidence

```rust
// byroredux/src/interaction.rs:863-868
fn collect_candidates(world: &World) -> FxHashMap<EntityId, InteractionKind> {
    if let Some(mut scratch) = world.try_resource_mut::<InteractionCandidateScratch>() {
        scratch.candidates.clear();
        populate_candidates(world, &mut scratch.candidates);   // 5 component locks, guard live
        std::mem::take(&mut scratch.candidates)
```

## Impact

Five scratch->component edges per frame in the debug lock-order graph. No reverse edge exists and `interaction_system` is a `Stage::Update` exclusive, so no demonstrable deadlock — MEDIUM.

## Suggested Fix

Take the map out of the resource first, populate an owned local with no guard held, and hand it back. The `std::mem::take` already present proves the map can leave the resource; capacity reuse is preserved.

## Completeness Checks
- [ ] **SIBLING**: The `else` branch and the rest of `interaction.rs` checked for the same shape
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
