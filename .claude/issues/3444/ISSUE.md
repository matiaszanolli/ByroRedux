# Issue #3444: CONC-D3-2026-08-27b-02: #2153's hold-stack reduction never happened — `let config = *config;` shadows but does not drop the `PoolRegenConfig` guard

**Finding ID**: CONC-D3-2026-08-27b-02
**Labels**: bug, medium, concurrency, character
**Filed from**: `docs/audits/AUDIT_CONCURRENCY_2026-08-27b.md`
**Audited at**: HEAD = 969d81c8

---

**Source**: `docs/audits/AUDIT_CONCURRENCY_2026-08-27b.md` — finding `CONC-D3-2026-08-27b-02` (MEDIUM, Dimension 3: ECS Lock Ordering & Deadlock). Audited at `HEAD = 969d81c8`; re-verified against current code at publish time.

**Location**: `crates/core/src/character/regen.rs` (`pool_regen_tick_system`)

## Description

#2153 asked for the `PoolRegenConfig` guard to be dropped before the `CharacterRuleset` acquire, reducing the hold-stack from 3 to 2. The implementation used **shadowing**, and the accompanying comment states the outcome as fact:

```rust
// crates/core/src/character/regen.rs — pool_regen_tick_system
let Some(config) = world.try_resource::<PoolRegenConfig>() else {
    return;
};
// Copy out and drop the guard immediately (#2153) — `PoolRegenConfig` is
// `Copy`, so nothing downstream needs the resource lock itself, only its
// three AVIF ids. Holding it across the `CharacterRuleset` acquire below
// built a 3-deep stack (`PoolRegenConfig` -> `CharacterRuleset` ->
// `ActorValues`) whose only correctness argument was "this system is
// registered exclusive" — true today, but unstated here and not enforced
// by the lock order itself. Dropping it here reduces the hold-stack to 2
// for the rest of the function, matching how `accumulator` is already
// dropped before `elapsed` is used.
let config = *config;
```

`let config = *config;` introduces a *new* binding that **shadows** the old one; the shadowed `ResourceRead<PoolRegenConfig>` is neither moved nor dropped at that point, so its `Drop` (which is what calls `lock_tracker::untrack`) runs at end of function scope.

Contrast the immediately following `accumulator`, which the same comment cites as the model and which *does* use an explicit `drop(accumulator);`:

```rust
let ticks = accumulator.advance(frame_dt);
drop(accumulator);
```

So the stack at `world.query_mut::<ActorValues>()` is still `{PoolRegenConfig(R), CharacterRuleset(R), ActorValues(W)}` — exactly what #2153 was filed against.

The identical shadowing mistake exists at `byroredux/src/combat.rs` (`melee_damage_charal_bonus`, `let config = *config;` after `try_resource::<MeleeDamageConfig>()`) and is correctly identified there by the same-day `AUDIT_ECS_2026-08-27.md`. What is new here is that the *canonical fix site* has the same bug **plus** a comment asserting it doesn't.

## Evidence

Source above, verified present at publish time in `crates/core/src/character/regen.rs` and `byroredux/src/combat.rs`.

## Trigger conditions

Every `pool_regen_tick_system` tick with a live `PoolRegenConfig` (Oblivion wiring). The defect is unconditional; only its *observability* needs `BYRO_LOCK_ORDER_CHECK=1`.

## Verification path

Source-only. Rust's drop semantics are the whole argument. A regression test can assert this directly by checking `lock_tracker` held-state after the shadowing line, or by source-asserting on an explicit `drop(...)` the way `physics_diagnostics_resolve_forms_after_storage_guards_drop` (`crates/physics/src/sync.rs`) already does for the sibling discipline.

## Impact

The hold-stack #2153 was filed to shrink is unchanged, so the risk #2153 described is unmitigated; worse, a reader (or auditor) who trusts the comment will conclude the site is clean. Combined with `CONC-D3-2026-08-27b-01` the stack now sits on one leg of a real cycle. A stale comment that asserts a *safety property* is materially worse than no comment.

## Suggested fix

One line — `drop(…)` cannot name the shadowed binding, so rename:

```rust
let config_guard = world.try_resource::<PoolRegenConfig>()…;
let config = *config_guard;
drop(config_guard);
```

Apply the same rename+drop at `byroredux/src/combat.rs` (`melee_damage_charal_bonus`). Then pin it with a source-assert test in `regen.rs`'s existing test module (it already source-asserts on `"try_resource::<CharacterRuleset>"`, so the harness is there).

## Related

#2153 (the original 3-deep stack this was supposed to close), the HIGH sibling `CONC-D3-2026-08-27b-01` (the live `ActorValues ↔ CharacterRuleset` cycle this stack now sits on), `ECS-2026-08-27-04` in `AUDIT_ECS_2026-08-27.md` (the `combat.rs` instance of the same shadowing mistake).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — at minimum `byroredux/src/combat.rs`'s `melee_damage_charal_bonus`; sweep for other `let x = *x;` copy-out-of-guard sites
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix (source-assert on the explicit `drop`, or a `lock_tracker` held-state assertion)
