# TD6-2026-08-16-01: InputAction::Block is bound to two inputs and a console command but has no gameplay effect; its consumer arm is unreachable

**Issue**: #2976
**Severity**: MEDIUM
**Dimension**: 6 — Stub & Placeholder Implementations
**Labels**: `medium,gameplay,combat,bug`
**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-08-16.md` (Dimension 6 — Stub & Placeholder Implementations). Effort: small.

**Location**: `byroredux/src/combat.rs`:46, :74-81, :168, :203-207 · `byroredux/src/interaction.rs`:141, :146, :542
**Age**: `4a404f5c` / `eb5d76fe`, 2026-08-15/16

## Description

`Block` is a **fully-wired input action** — `KeyCode::KeyC`, `MouseButton::Right`, and the console tokens `"block" | "c"` reachable through `input.press` / `input.hold`. `combat_input_system` reads it into `CombatState.blocking`.

Nothing else reads `blocking` except the `combat.status` display string.

Meanwhile the **sole** `HitEvent` producer in the workspace hardcodes `blocked: false`, so `combat_damage_system`'s zero-damage arm is unreachable from any live path.

Blocking therefore costs the player nothing and gains them nothing; the only observable is a debug string. This is a stub reachable from a shipped console command, which the severity table promotes to MEDIUM.

## Evidence

`combat.rs`:74-81
```rust
actions.is_held(InputAction::Block),
…
state.blocking = block_held;
```

`combat.rs`:157-169 — the only `HitEvent` construction outside tests:
```rust
byroredux_scripting::HitEvent {
    aggressor, source: aggressor, projectile: 0,
    power_attack: false, sneak_attack: false,
    bash_attack: false, blocked: false,
}
```

`combat.rs`:203-207 — the arm that can never be taken:
```rust
let damage = if event.blocked { 0.0 } else { attack_damage(world, event.aggressor) };
```

Workspace grep confirms `combat.rs` is the only non-test `HitEvent` producer (the other hits are the struct definition, `register`, the Late-stage drain, and a recognizer-table doc comment). The only reader of `blocking` is `byroredux/src/commands/view.rs`:102-104.

**Four sibling `HitEvent` fields** — `projectile`, `power_attack`, `sneak_attack`, `bash_attack` — are likewise constant at the producer with no reader anywhere.

## Impact

A player (or the p2 smoke gate, or a future combat test) who holds Block **takes full damage**.

Because `CombatState.blocking` *is* surfaced by `combat.status`, the console reports a defensive state the damage pipeline does not honour — which is worse than reporting nothing, since it reads as working.

The unreachable arm also means any future `blocked`-aware regression test is **green by construction** until a producer sets the flag.

## Suggested Fix

Set `blocked: state.blocking` at the producer and pin it with a `combat_damage_system` unit test asserting a blocked hit applies zero damage and still counts as a hit.

Or, if damage mitigation is deferred: delete the `Block` bindings and the consumer arm and say so in `docs/engine/playable-vertical-slice.md` rather than shipping an inert binding.

Shipping a bound action that provably does nothing is the worse of the two states.

## Related

- `AUDIT_ECS_2026-08-16` § ECS-2026-08-16-04 (the parallel `EquippedWeapon` write-path gap in the same slice)
- `AUDIT_CHARACTER_2026-08-16` § CHAR-2026-08-16-D1-01 (`attack_damage` bypasses CHARAL entirely)
- TD3-2026-08-16-03 (the damage-recompute comment in the same file)

## Completeness Checks
- [ ] **SIBLING**: The four other constant-at-producer `HitEvent` fields (`projectile`, `power_attack`, `sneak_attack`, `bash_attack`) resolved the same way — wired or removed
- [ ] **NO-GREEN-BY-CONSTRUCTION**: The new test fails if the producer stops setting `blocked`
- [ ] **CONSOLE-TRUTH**: `combat.status` no longer reports a defensive state the damage pipeline ignores
- [ ] **SMOKE**: `docs/smoke-tests/p2-melee-core.sh` covers whichever direction is taken
- [ ] **TESTS**: A regression test pins this specific fix

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state —
query `gh issue view 2976 --json state` when live state is needed.*
