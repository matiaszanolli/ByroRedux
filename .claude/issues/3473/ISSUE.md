# Issue #3473

**Title**: ECS-2026-08-27-04 (partial): the three P2 gameplay exclusives use a bare `add_exclusive`, so `combat_input_system`'s five-deep `EquippedWeapon` hold-stack is a blank row on `sys.accesses`

**Labels**: low, ecs, combat, concurrency, bug

**Filed**: 2026-08-27 via `/audit-publish docs/audits/AUDIT_ECS_2026-08-27.md`

---

**Source**: `docs/audits/AUDIT_ECS_2026-08-27.md` — finding `ECS-2026-08-27-04` (LOW, Dimension 1: Lock Ordering + 5b: Scheduler Access Declarations). Audited at `HEAD = 969d81c8`; re-verified against current code at publish time.

> ⚠️ **Filed as the surviving half of `ECS-2026-08-27-04` only.** The original finding's impact statement asserted that "no reverse edge exists in-tree today" for the `CharacterRuleset → ActorValues` hold order. **That premise is false** — the reverse edge lives in `crates/scripting/src/condition.rs`'s `ConditionFunction::GetActorValue` arm and is tracked as **#3441** (HIGH). The lock-*cycle* half of this finding is therefore superseded by #3441 and is **not** re-filed here. What is filed here is the part #3441 does not cover: the undeclared scheduler access on the three P2 gameplay exclusives, and the five-deep guard hold-stack rooted at `EquippedWeapon`.

## Description

`attack_damage` (`byroredux/src/combat.rs`) matches on `world.get::<EquippedWeapon>()`, which yields a `ComponentRef` whose read guard is bound for the whole match arm — and the arm's body calls `melee_damage_charal_bonus(world, aggressor)`, which then acquires, and holds to function end, `MeleeDamageConfig` (read), `CharacterRuleset` (read), `ActorValues` (read) and `CharacterLevel` (read). Note that `let config = *config;` **shadows** but does not drop the `MeleeDamageConfig` guard — the original binding lives to the end of the function scope. That is a five-deep nested hold-stack, established across a helper call, which is the exact pattern `crates/core/src/ecs/world.rs`'s "snapshot before you iterate" house rule was written to prohibit.

This is the same shape as #2153's `pool_regen_tick_system` (a 3-deep stack: `PoolRegenConfig` read → `PoolRegenAccumulator` write → `CharacterRuleset` read → `ActorValues` write). That one was resolved under #2391 by moving it to `add_exclusive_with_access` **specifically so the disputed types appear on the `sys.accesses` report instead of a blank row** — the rationale is spelled out in `byroredux/src/boot.rs`. The three P2 gameplay exclusives (`interaction_system`, `combat_input_system`, `combat_damage_system`) were added afterwards with plain `add_exclusive` and got no such treatment.

## Evidence

```rust
// byroredux/src/combat.rs — attack_damage
match world.get::<EquippedWeapon>(aggressor) {
    …
    Some(weapon) => weapon.damage.max(0.0) + melee_damage_charal_bonus(world, aggressor),
    None => UNARMED_DAMAGE,
}
```

```rust
// byroredux/src/combat.rs — melee_damage_charal_bonus
fn melee_damage_charal_bonus(world: &World, aggressor: EntityId) -> f32 {
    let Some(config) = world.try_resource::<MeleeDamageConfig>() else { return 0.0; };
    let config = *config;                                   // shadows; guard still live
    let Some(ruleset) = world.try_resource::<CharacterRuleset>() else { return 0.0; };
    let Some(avs) = world.get::<ActorValues>(aggressor) else { return 0.0; };
    let level = world.get::<CharacterLevel>(aggressor).map_or(1, |level| level.level);
```

```rust
// byroredux/src/boot.rs
scheduler.add_exclusive(Stage::Update, crate::interaction::interaction_system);
scheduler.add_exclusive(Stage::Update, crate::combat::combat_input_system);
scheduler.add_exclusive(Stage::Update, crate::combat::combat_damage_system);
```

## Impact

No live deadlock at these sites: all three are `Stage::Update` exclusives, so they hold the world serially and no parallel partner exists. (The original finding went on to argue that the `CharacterRuleset → ActorValues` ordering is uncontested in-tree — **that is wrong; see #3441**, which records the live reverse edge in `condition.rs` and the `BYRO_LOCK_ORDER_CHECK=1` abort it produces.)

The cost tracked here is diagnostic: the deepest hold-stack in the newest, least-reviewed subsystem is invisible to `sys.accesses`, and `BYRO_LOCK_ORDER_CHECK` will record four new edges out of `EquippedWeapon` with no declaration to compare them against. A future move of either combat system into a parallel lane would silently bypass the analyzer — and given #3441, that lane would be moving code that is already on a cycle.

## Suggested fix

Drop the `EquippedWeapon` guard before computing the CHARAL bonus — snapshot `weapon.damage` into an `f32` and let the `ComponentRef` die at the end of that statement — and give `melee_damage_charal_bonus` the same treatment for `MeleeDamageConfig` (`drop(config_guard)` after the copy). Then register all three P2 exclusives via `add_exclusive_with_access` with their real read/write sets, matching `pool_regen_tick_system`. Coordinate the `ActorValues` / `CharacterRuleset` half with #3441 so the two fixes agree on the surviving direction.

## Related

#3441 (the live `ActorValues ↔ CharacterRuleset` reverse edge — supersedes this finding's lock-cycle half), #2153 (the identical 3-deep stack), #2391 / ECS-D5B-03 (the `add_exclusive_with_access` remedy and its stated rationale), #2270 (the `world.rs` house rule).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (`interaction_system`'s own guard stack in `byroredux/src/interaction.rs`; other `world.get::<…>()`-in-match-scrutinee sites in the P2 slice)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved; the surviving `CharacterRuleset` / `ActorValues` direction must agree with #3441 and land in `docs/engine/ecs.md`'s canonical order
- [ ] **TESTS**: A regression test pins this specific fix (an `access_report` assertion that the three P2 exclusives declare non-empty read/write sets)
