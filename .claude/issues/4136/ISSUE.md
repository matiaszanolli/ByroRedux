### SAVE-D1-2026-09-11-02: `Locked` became a runtime-mutable component this cycle (#3159's `SetLocked`/`SetLockLevel` fragment effects) but was never registered as save state, and the cell-load stamp unconditionally overwrites it

- **Severity**: HIGH
- **Dimension**: 1 — Snapshot Completeness & Determinism
- **Data-Loss Class**: silent-drop
- **Location**: `crates/core/src/ecs/components/lock.rs` (the `Locked` component: `lock_level: u8`, `key_form_id: Option<u32>` — no `FixedString`/`EntityId`, delta-safe by the same bar `MUTABLE_DELTA_COLUMNS`'s other entries pass); `crates/scripting/src/fragment/effects.rs:814-852` (`Effect::SetLocked`/`Effect::SetLockLevel`); `byroredux/src/cell_loader/spawn.rs:919-932` (the unconditional ESM-authored `Locked` stamp on every cell load/reload); `byroredux/src/save_io/registry_completeness_tests.rs:428` (the `NOT_SAVED_BY_DESIGN` entry that names the gap but does not close it); `byroredux/src/save_io.rs:324-508` (`build_save_registry` — `Locked` absent)
- **Status**: NEW. `Locked`'s mutability is new this cycle: `e26579c1` (Fix #3159) added `SetLocked`/`SetLockLevel` specifically because an authored lock was previously "a one-way door for the session." Before #3159, `Locked` was correctly write-once-from-ESM and its `NOT_SAVED_BY_DESIGN` classification was accurate.
- **Source**: `docs/audits/AUDIT_SAVE_2026-09-11.md`

**Description**: The registry-completeness guard's own allowlist entry states the gap explicitly rather than hiding it, and explicitly declines to be treated as a justification:

> `("Locked", "XLOC lock data, rederived from the plugin's parsed REFR every cell load (#3098). NO LONGER rederived *identically*: #3159 added Effect::SetLocked, so a script can now unlock a door mid-session and a reload re-stamps the authored lock over it. **This is a real save-fidelity gap, not a justification** — the component needs registering once a scripted lock change is expected to survive a save...")

No tracking issue covers it. Concretely, `Effect::SetLocked` inserts/removes `Locked` and `Effect::SetLockLevel` mutates its `lock_level` field. Nothing persists that change: it is absent from `build_save_registry`, so a full save/load silently reverts to whatever the ESM authored. The *live* reload path has the identical exposure without even needing a save — `cell_loader/spawn.rs:919-932` unconditionally re-inserts `Locked` from the placement's authored `XLOC` on every cell (re)load, with no check for an already-present (scripted) value:

```rust
if let Some(l) = lock {
    world.insert(placement_root, Locked { lock_level: l.lock_level, key_form_id: l.key_form_id });
}
```

A player who picks a lock, then triggers any cell reload — a live `load <slot>`, or simply leaving and re-entering the cell in the same session — finds the door re-locked, with no record anywhere of what happened.

**Evidence**: `lock.rs` struct fields, `fragment/effects.rs:814-852`, `cell_loader/spawn.rs:919-932`, and `registry_completeness_tests.rs:428` all directly confirmed during publish. `grep -n "Locked" byroredux/src/save_io.rs crates/save/src/validate.rs` returns nothing — no registration, no validation gate.

**Impact**: Any scripted lock/unlock — vanilla content routinely uses `Lock()`/`Unlock()`/`SetLockLevel()` on quest-gating doors and containers — does not survive a save/load and does not even survive an in-session cell revisit. A door meant to stay open after a quest event re-locking itself can strand a player or re-block content the quest already unlocked.

**Related**: #3159 (introduced the mutation with no persistence companion); #3098 (the original write-once XLOC stamp, correct at the time); the analogous, already-fixed pattern for `EquippedWeapon` (#3488) and `ReferenceEnableState` (#3278/#3789) — both show the project's established fix shape for exactly this class of gap.

**Suggested Fix**: Register `Locked` in `build_save_registry` (plain `u8`/`Option<u32>`, no session-local hazard) and add it to `MUTABLE_DELTA_COLUMNS`. Gate the cell-load stamp at `spawn.rs:919-932` on the placement root not already carrying a `Locked` component from a prior overlay/restore. Remove the `NOT_SAVED_BY_DESIGN` entry once both land.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix — save with a scripted `SetLocked`/`SetLockLevel` change, load, assert the change survives; also assert a live in-session cell revisit doesn't re-stamp over it
- [ ] **SIBLING**: Any other component that gained a runtime mutator recently (grep new `Effect::Set*` variants) for the same "mutable but never registered" gap
