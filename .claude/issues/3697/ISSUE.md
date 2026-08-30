# #3697 — ECS-2026-08-30-P2-01: combat_input_system holds the CombatState write guard across an EquippedWeapon component read

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: MEDIUM · **Dimension**: P2 Gameplay Slice / Lock Ordering
**Location**: `byroredux/src/combat.rs` (`combat_input_system` cooldown-arming branch, ~:120-135; helper `attack_cooldown_seconds` ~:382-389)
**Source**: `docs/audits/AUDIT_ECS_2026-08-30.md` (ECS-P2-01, `[P2-gameplay]`)

> **Coverage note**: `byroredux/src/combat.rs` has no owner audit skill. This finding comes from the `/audit-ecs` run's explicit P2-gameplay slice sweep and is the only audit coverage that file received.

## Description

The cooldown-arming branch runs inside `if let Some(mut state) = world.try_resource_mut::<CombatState>()` and calls `attack_cooldown_seconds(world, aggressor)` — which does `world.get::<EquippedWeapon>(aggressor)` — while that `ResourceWrite` guard is still live. This is exactly the pattern the World house rule forbids (`crates/core/src/ecs/world.rs:10-33`: snapshot into an owned local and drop your guards *before* calling a helper that locks). It is the only resource-**write**-across-component-read site in the slice.

## Evidence

```rust
// byroredux/src/combat.rs:120-129
let attack_ready = if let Some(mut state) = world.try_resource_mut::<CombatState>() {
    state.blocking = block_held;
    state.cooldown_remaining = (state.cooldown_remaining - dt.max(0.0)).max(0.0);
    if attack_pressed && in_character_mode && state.cooldown_remaining <= 0.0 {
        state.cooldown_remaining = aggressor.map_or(MELEE_COOLDOWN_SECONDS, |aggressor| {
            attack_cooldown_seconds(world, aggressor)   // -> world.get::<EquippedWeapon>()
        });
```

```rust
// byroredux/src/combat.rs:382-389
fn attack_cooldown_seconds(world: &World, aggressor: EntityId) -> f32 {
    world.get::<EquippedWeapon>(aggressor)
        .filter(|weapon| weapon.speed > 0.0)
        .map_or(MELEE_COOLDOWN_SECONDS, |weapon| MELEE_COOLDOWN_SECONDS / weapon.speed)
}
```

## Impact

Records a `CombatState(write) -> EquippedWeapon(read)` edge in the global lock-order graph. No reverse edge exists today (`CombatState`'s other readers clone and release, and `combat_input_system` is a `Stage::Update` exclusive), so this is a latent edge rather than a live deadlock — hence MEDIUM, not HIGH.

## Suggested Fix

Resolve the cooldown before opening the guard (`let armed_cooldown = aggressor.map_or(…);` above the `try_resource_mut`) and assign the precomputed value inside, flattening the hold-stack to one lock.

## Completeness Checks
- [ ] **SIBLING**: Same guard-across-helper pattern checked in the rest of `combat.rs` and `interaction.rs`
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
