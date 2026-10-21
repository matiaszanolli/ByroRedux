# #6313: PERF-D1-02: Two new per-frame systems do whole-structure work with no change gate: `encumbrance_system` clones the player's inventory, `head_look_system` walks the skeleton per looking actor

**Labels**: low,performance,gameplay,inventory,bug

**Source**: docs/audits/AUDIT_PERFORMANCE_2026-10-21.md · **Severity**: LOW · **Dimension**: CPU Hot Paths · **Location**: `byroredux/src/encumbrance.rs (encumbrance_system); byroredux/src/systems/head_look.rs (find_head)`

- **Location**: `byroredux/src/encumbrance.rs:114-163` (`world.get::<Inventory>(player).map(|i| i.clone())` at `:125`, `catalog.inventory_weight`, `compose_max_carry_weight` → `compose_entry_point` perk evaluation at `:150`, every frame); `byroredux/src/systems/head_look.rs:36-44` + `:73` (`find_head` → `collect_subtree(world, root)` plus a `Name` scan, every frame per actor carrying `HeadLook`).
- **Status**: NEW (M47.3 / M42 work since the baseline). Searched "encumbrance", "head_look": only #6062 (lock order, closed).
- **Description**: the player's weight and capacity change only on inventory, actor-value or perk changes, yet the system deep-clones `Inventory.items` (one allocation plus N `ItemStack` copies) and recomposes perk entry points every frame. `HeadLook` persists until `StopLook`, and each frame re-collects the whole skeleton subtree (~80 entities, one `Vec`) to find `bip01 head`, an entity that never changes.
- **Evidence**: code read. Derived: a few µs per frame each. Small, but the shape the skill lists ("per-entity component deep-clone inside a per-frame loop", "whole-world scans with no change-detection key").
- **Suggested Fix**: borrow the inventory instead of cloning it (compute the weight under the guard, or keep a running total updated by the inventory mutators), and recompose only when the inventory, `ActorValues` or perk generation moves. Cache the head entity in `HeadLook` at `LookAt` time.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
