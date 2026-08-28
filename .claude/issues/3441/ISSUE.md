# Issue #3441: CONC-D3-2026-08-27b-01: live `ActorValues ↔ CharacterRuleset` lock-order cycle — the CTDA `GetActorValue` arm is the reverse edge `pool_regen_tick_system` and `melee_damage_charal_bonus` assume does not exist

**Finding ID**: CONC-D3-2026-08-27b-01
**Labels**: bug, ecs, high, scripting, concurrency, character
**Filed from**: `docs/audits/AUDIT_CONCURRENCY_2026-08-27b.md`
**Audited at**: HEAD = 969d81c8

---

**Source**: `docs/audits/AUDIT_CONCURRENCY_2026-08-27b.md` — finding `CONC-D3-2026-08-27b-01` (HIGH, Dimension 3: ECS Lock Ordering & Deadlock). Audited at `HEAD = 969d81c8`; re-verified against current code at publish time.

## Description

`evaluate_function`'s `GetActorValue` arm binds an `ActorValues` **storage read guard** and keeps it live across a `CharacterRuleset` **resource read**, because the guard is used again at the end of the arm:

```rust
// crates/scripting/src/condition.rs — ConditionFunction::GetActorValue arm (~:457-510)
let Some(avs) = world.get::<ActorValues>(entity) else {
    return 0.0; // no `ActorValues` → absent-AV default
};
if avs.get(condition.param_1).is_some() {
    return avs.current(condition.param_1);
}
…
if let Some(rs) = world.try_resource::<CharacterRuleset>() {     // ← ActorValues → CharacterRuleset
    if let Some(formula) = rs.derived_formula(condition.param_1) {
        …
        let level = world.get::<CharacterLevel>(entity).map_or(0, |l| l.level);
        return rs
            .derived_value(condition.param_1, &avs, level)        // ← `avs` still live
            .unwrap_or(0.0);
```

Two in-tree sites acquire the same pair in the **opposite** order, and one takes `ActorValues` for **write**:

```rust
// crates/core/src/character/regen.rs — pool_regen_tick_system
let Some(ruleset) = world.try_resource::<CharacterRuleset>() else { return; };
let Some(mut avs_q) = world.query_mut::<ActorValues>() else { return; };   // ← CharacterRuleset → ActorValues (W)
```

```rust
// byroredux/src/combat.rs — melee_damage_charal_bonus
let Some(ruleset) = world.try_resource::<CharacterRuleset>() else { return 0.0; };
let Some(avs) = world.get::<ActorValues>(aggressor) else { return 0.0; };  // ← CharacterRuleset → ActorValues (R)
```

`lock_tracker` keys one thread-local `LOCKS` map and one global `GRAPH` by `TypeId` for **both** storages and resources (`crates/core/src/ecs/lock_tracker.rs`), so this is a genuine 2-cycle in the detector's graph, not a category confusion.

## ⚠️ Supersedes a contradictory claim in a same-day sibling report

`docs/audits/AUDIT_ECS_2026-08-27.md` § `ECS-2026-08-27-04` states of the `CharacterRuleset → ActorValues` hold order that **"no reverse edge exists in-tree today"**. **That premise is false and is superseded by this issue.** The reverse edge has been in `crates/scripting/src/condition.rs` since `2b9147ae` (2026-07-01), predating the `combat.rs` site (`08434727`, 2026-08-19) that the ECS report examined. If an issue was filed from `ECS-2026-08-27-04`, its impact statement should be reconciled against this one rather than treated as independent.

## Evidence

- Forward edge: `crates/scripting/src/condition.rs`, `ConditionFunction::GetActorValue` arm — `world.get::<ActorValues>(entity)` guard `avs` is still live at the `rs.derived_value(…, &avs, level)` call inside the `try_resource::<CharacterRuleset>()` block.
- Reverse edge (write): `crates/core/src/character/regen.rs`, `pool_regen_tick_system`.
- Reverse edge (read): `byroredux/src/combat.rs`, `melee_damage_charal_bonus`.
- Both reverse-edge sites document their own correctness as resting on the *absence* of this edge (`regen.rs`'s `#2153` comment: the 3-deep stack's "only correctness argument was 'this system is registered exclusive'").

## Trigger conditions

Any session that evaluates a `GetActorValue` CTDA for an actor-value the subject does not carry (the arm falls through to the ruleset branch) **and** either runs `pool_regen_tick_system` with a live `PoolRegenConfig` (Oblivion) or resolves one melee swing through `melee_damage_charal_bonus` (FNV/FO3, `MeleeDamageConfig` + `CharacterRuleset` present). Both halves are ordinary gameplay: CTDA `GetActorValue` gates AI packages (`byroredux/src/npc_spawn/ai_package.rs`), quest stages, triggers and scenes; melee is the P2 vertical slice's core loop.

## Verification path

Reproducible without a GPU. Run any FNV/FO3 cell with `BYRO_LOCK_ORDER_CHECK=1` (debug build — `global_order::record_and_check` is `#[cfg(debug_assertions)]`), let one AI-package CTDA evaluate `GetActorValue`, then swing at an NPC: the second-observed edge closes the cycle and `record_and_check` panics. The static `access_report` KPIs cannot see it — all sites involved are `Stage::Update` **exclusives**, and exclusives are never paired by `analyze_pair`.

## Impact

No live deadlock **today** — every site (`pool_regen_tick_system`, `combat_damage_system`, and every condition-evaluating dispatcher: `trigger_detection_dispatch`, `quest_advance_dispatch`, `scene_playback_system`, `ambient_ai_package_system`) is a `Stage::Update` exclusive on the main thread, and an ABBA deadlock needs two threads. The concrete damage is:

1. Any `BYRO_LOCK_ORDER_CHECK=1` FNV/FO3/Oblivion session aborts once both edges are observed — the same failure mode as the closed #3260.
2. The invariant that would make a future parallelisation safe is already broken, silently, with a same-day report on record asserting the opposite. That premise is what makes a future move of `combat_damage_system`, `pool_regen_tick_system` or any condition-evaluating dispatcher into a parallel lane look like a one-line change. It is not.

## Suggested fix

Break the edge at the `condition.rs` end, where it is cheapest and where the guard is only read: snapshot what the arm needs out of `ActorValues` before touching the ruleset. `ActorValues::get`/`current` already return `Copy` scalars, so the only real dependency is `derived_value(&avs, …)` — give it an owned clone (or restructure to compute `derived_value` from a copied SPECIAL/skill slice). Then pick the surviving direction (`CharacterRuleset → ActorValues`, matching `regen.rs` and `combat.rs`) and add it to the canonical acquisition order in `docs/engine/ecs.md`, which currently names neither type — the same gap #3261 closed for `CharacterController`/`RapierHandles`.

## Related

#2153 (the original 3-deep stack), #2391 / ECS-D5B-03 (`add_exclusive_with_access` remedy), #3260 (identical class, rated HIGH, since closed), #2270 (`world.rs` "snapshot before you iterate" house rule), `ECS-2026-08-27-04` in `AUDIT_ECS_2026-08-27.md` (the falsified premise, superseded here), and the sibling `CONC-D3-2026-08-27b-02` (the same `regen.rs` function's inert guard drop).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other `condition.rs` arms that hold a storage guard across a resource acquire; other CHARAL consumers of `CharacterRuleset`)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved; the surviving direction is added to `docs/engine/ecs.md`'s canonical order
- [ ] **TESTS**: A regression test pins this specific fix (source-assert on the acquisition order, in the shape of `debug_evaluator_acquires_locks_in_canonical_order` / `camera_follow_does_not_close_character_lock_cycle`)
