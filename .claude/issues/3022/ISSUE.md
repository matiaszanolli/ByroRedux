# SAVE-D1-01: P2 combat made component removal a gameplay transition the additive-only overlay cannot replay

**Issue**: #3022
**Severity**: MEDIUM
**Dimension**: 1 — snapshot/delta model
**Labels**: `medium,ecs,bug`
**Source report**: `docs/audits/AUDIT_SAVE_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SAVE_2026-08-16.md` (Dimension 1 — snapshot/delta model).

**Location**: `byroredux/src/combat.rs`:215-241 (the kill branch), :294-311 (`disable_actor_ai`) · `crates/save/src/driver.rs`:265-275 (`apply_deltas`'s additive-only contract) · `byroredux/src/save_io.rs`:83-129 (`MUTABLE_DELTA_COLUMNS`)

**Status note**: NEW — this **invalidates the premise** of the deferred-documented note at `crates/save/src/driver.rs`:265-275 (#1847 / SAVE-04). The audit skill explicitly says to re-flag it *"once such a component lands without the promised companion despawn/hide pass"*. **It has landed.**

## Description

The P2 combat slice made component **removal** a gameplay transition. `apply_deltas` is structurally additive-only and cannot replay a removal, so a killed NPC reloads standing, animating and AI-capable.

## Evidence

The kill branch removes components:
```rust
// byroredux/src/combat.rs (kill branch)
dead.insert(target, Dead);
…
players.remove(skeleton_root);      // <- a removal, not an insert
```

The overlay's own contract says it cannot replay that:
```rust
// crates/save/src/driver.rs:265-275
/// This overlay is **additive-only** — it can update or insert a row via
/// `ApplyFn`, never remove one. The reloaded cell respawns every REFR
/// authored in the ESM regardless of what happened to it during the saved
/// session. There is currently no enable/disable/delete persistence
/// mechanism to overlay in the first place (no `Disabled`/`Deleted` marker
/// component exists), so this is a latent gap, not an active bug …
/// It becomes a real reference-break the moment such a component and its
/// "which REFRs were disabled/deleted this session" set land …
```

Re-verified 2026-08-17. The note's escape clause — *"no `Disabled`/`Deleted` marker component exists"* — is now false: `Dead` is inserted and the animation/AI components are removed.

## Impact

Kill an NPC, save, load: the actor returns alive from the ESM respawn, with its AI teardown undone. The `Dead` marker may re-apply from the delta (it is an insert), but the *removals* — the AI-behaviour teardown at `disable_actor_ai` — cannot, so the reloaded actor is in a state that never existed during play.

This is the first gameplay transition to depend on removal semantics, so it converts a documented latent gap into an active correctness bug in the P2 slice.

## Suggested Fix

Implement the companion despawn/hide pass the `driver.rs` note already specifies — run after `apply_deltas`, keyed the same way (`remap`) — and persist the set of entities whose components were removed.

Update the `driver.rs`:265-275 note either way: its "nothing regresses today" premise must not survive this change unqualified.

## Related

- #1847 / SAVE-04 (the deferred note this invalidates)
- #2976 (TD6-2026-08-16-01 — the same combat slice's `Block` gap)

## Completeness Checks
- [ ] **PREMISE**: The `driver.rs`:265-275 note is rewritten — it currently asserts a condition that is no longer true
- [ ] **REMOVAL-REPLAY**: Component removals survive a save/load round trip, not just insertions
- [ ] **SIBLING**: Any other gameplay path that removes components (AI teardown, despawn) covered by the same mechanism
- [ ] **ROUND-TRIP**: Kill → save → load leaves the actor dead with AI still torn down
- [ ] **TESTS**: A regression test pins the kill round-trip

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3022 --json state` when live state is needed.*
