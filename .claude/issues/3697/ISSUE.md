# #3697 — ECS-P2-01: combat_input_system holds the CombatState write guard across an EquippedWeapon component read

**Severity**: MEDIUM · **Dimension**: P2 Gameplay Slice / Lock Ordering
**Location**: `byroredux/src/combat.rs` (`combat_input_system`, cooldown-arming branch; helper `attack_cooldown_seconds`)

## Fix

Resolved `attack_cooldown_seconds(world, aggressor)` into an owned local
(`armed_cooldown`) before `try_resource_mut::<CombatState>()` opens, per
the issue's own suggested fix, instead of calling it from inside the
guard. The precompute runs unconditionally (cheap — one component lookup
either way) rather than only on frames that actually arm a cooldown,
which is the simpler flattening and avoids re-deriving the eligibility
condition twice.

## SIBLING (issue's own checklist item)

Swept every `try_resource_mut`/`resource_mut` site in `combat.rs` and
`interaction.rs` for the same "component read nested inside a resource
write guard" shape:

- `combat.rs`'s other three `CombatState` writers (`combat_damage_system`,
  the queued-hit-event site, `record_miss`) and both
  `PendingDeathReconciliations` sites only assign already-resolved local
  values inside their guards — no nested reads, no fix needed.
- `interaction.rs`'s `InteractionState`, `InteractionCandidateScratch`
  (the `collect_candidates` fallback branch at line 852), and
  `InteractionTrace` write sites are likewise clean.
- **One exception found**: `interaction.rs::collect_candidates` (the
  scratch-reuse branch) calls `populate_candidates(world, &mut
  scratch.candidates)` — which does several `world.query::<T>()` calls —
  while `InteractionCandidateScratch`'s write guard is still open. This is
  the *same pattern*, but it's already tracked separately as **#3698**
  ("`collect_candidates` holds the scratch write guard across five
  component reads, every frame") — left alone here rather than folding
  an unrelated issue's fix into this one.

## LOCK_ORDER (issue's own checklist item)

No `RwLock` scope changed in shape — `CombatState`'s write guard is still
opened and held for the same span of assignments; only the value it
assigns (`armed_cooldown`) moved from a nested call to a pre-resolved
local.

## TESTS (issue's own checklist item)

Added `combat_input_system_does_not_close_combat_state_equipped_weapon_lock_cycle`,
following the `#3303`-established live-detector pattern
(`crates/physics/src/sync.rs`'s `pull_dynamic_does_not_close_...`):
guarded on `BYRO_LOCK_ORDER_CHECK=1` (no-op otherwise, so the normal test
suite stays unaffected), establishes the canonical reverse edge
(`EquippedWeapon` read, then `CombatState` write — the order every other
`CombatState` writer in this file already uses) before driving the real
`combat_input_system`, on a fixture with an aggressor entity carrying a
real `EquippedWeapon`.

Verified the guard actually catches the regression (this session's
established quality bar): reintroduced the nested
`attack_cooldown_seconds(world, aggressor)` call inside the `CombatState`
guard, reran under `BYRO_LOCK_ORDER_CHECK=1` — the test failed with the
exact expected `EquippedWeapon ↔ CombatState` cycle message, then restored
the fix and confirmed a clean pass again.

## Verification

- `cargo check -p byroredux --tests`: clean.
- `cargo test -q -p byroredux --bin byroredux`: 1,870 tests passing, 0
  failing (+1 new).
- `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux
  combat_input_system_does_not_close`: passes with the detector live.
- `cargo test -q --no-fail-fast` (full workspace): **7091 passing, 0
  failing**.
