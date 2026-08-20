# SAVE-D1-2026-08-20-01: CharacterController gained two mutable gameplay fields whose allowlist reason predates them and whose own doc claims they are saved — a live load leaks the pre-load breath value

**Issue**: #3165 — https://github.com/matiaszanolli/ByroRedux/issues/3165
**Finding ID**: `SAVE-D1-2026-08-20-01`
**Severity**: MEDIUM
**Dimension**: 1 — Snapshot Completeness & Determinism
**Audit**: `/audit-save` — 2026-08-20 comprehensive suite, HEAD `bb0b92f2`
**Labels**: medium, ecs, bug

---

**Audit**: `/audit-save` — `docs/audits/AUDIT_SAVE_2026-08-20.md` (HEAD `bb0b92f2`)
**Finding ID**: `SAVE-D1-2026-08-20-01`
**Severity**: MEDIUM
**Dimension**: 1 — Snapshot Completeness & Determinism
**Data-Loss Class**: silent-drop

## Location

- `crates/physics/src/components.rs:146-152` — `breath_remaining`, `drowning_damage_accumulator` + the doc claim
- `byroredux/src/save_io/registry_completeness_tests.rs:224` — the allowlist reason
- `byroredux/src/save_io.rs:500-508` — `apply_player_pose`'s momentum clear (the three fields the reason actually describes)
- `byroredux/src/systems/character.rs:239-241`, `:474-484`, `:1027-1045` — the drowning runtime

## Description

`CharacterController` is allowlisted as not-saved with the reason:

> "mutable per-frame fields (velocity/grounded/jump) are deliberately zeroed on reload by the
> pose-restore path, not carried over"

That is an accurate description of `vertical_velocity` / `is_grounded` / `wants_jump`, and
`apply_player_pose` does zero exactly those three. It is **not** a description of the two fields
the water/drowning delta added:

```rust
/// Remaining breath while the player's head is submerged. Seconds.
pub breath_remaining: f32,
/// Accumulated drowning damage is kept on the controller so save/load and
/// fixed-step updates do not lose fractional damage between ticks.
pub drowning_damage_accumulator: f32,
```

**The second field's own doc asserts that keeping it on the controller is what makes it survive
save/load. It does not.** `CharacterController` is in no registry column, and `apply_player_pose`
touches neither field.

The live-load behaviour is *worse than a reset*, because the player body is **not cell-owned**.
`scene.rs`'s player spawn (`:1168-1218`) runs *after* `load_cell_with_masters` and is never
covered by `stamp_cell_root`'s entity-id range; `World::spawn` is strictly monotonic with no id
recycling (`crates/core/src/ecs/world.rs:85-92`), so the body's id is always below the reloaded
cell's `first`. `unload_cell` drains victims from `CellRootIndex` only, so the player entity —
and its `CharacterController` — **survives every live load untouched**.

The result: pressing F9 while drowning at `breath_remaining = 0.2` reloads a save taken with a
full 15-second reserve and leaves the player at 0.2 seconds, drowning again immediately in the
restored world. Across a process restart plus `--load`, the same fields reset to `HUMAN`'s
15.0 / 0.0 instead. **Neither outcome is the saved value, because the saved value does not
exist.**

## Second, distinct defect in this block: two doc claims describe a mechanism that does not exist

Separable from the unsaved-fields defect, and worth fixing in the same pass:

- `grep -rn "CellRoot" byroredux/src/scene.rs` → **zero hits**. The `PlayerEntity` allowlist
  reason at `registry_completeness_tests.rs:279` ("cleared by cell unload — … it's stamped with
  `CellRoot`") and the identical claim at `byroredux/src/systems/character.rs:39-41` are both
  **factually wrong about the mechanism** — even though the resource itself stays correct,
  because the entity simply persists.
- `grep -rn "spawn_player_character"` returns **only those two doc comments** — the function does
  not exist.

## Evidence

- `grep -rn "CharacterController" byroredux/src/save_io.rs` → two hits, both inside
  `apply_player_pose` (an `eye_height` read at `:485` and the three-field momentum clear at
  `:502`); **no registration**.
- The allowlist entry at `registry_completeness_tests.rs:224` is unchanged since 2026-08-05,
  while `git log` shows the breath pair arriving with the water/drowning work.
- The `WaterContact` allowlist reason's trailing parenthetical *"drowning accumulation is not yet
  wired"* is now stale prose for the same reason — drowning shipped in
  `byroredux/src/systems/character.rs` — though that reason's *conclusion* is unaffected,
  because the accumulator lives on `CharacterController`, not on `WaterContact`.

## Impact

A bounded but genuine gameplay-state gap in exactly the surface the delta introduced — and the
first instance of a **general hazard**: because the player body outlives the cell, *any* unsaved
mutable component on it leaks its pre-load value through a live load rather than resetting. The
additive overlay cannot correct that, by design.

Zooming out, the mechanism that let it through is the guard's **granularity**: the SAVE-D1-12
allowlist is keyed by *type*, so once a type is allowlisted for reason X, every field added to it
afterwards is invisible forever, however thoroughly X stops describing it.

## Related

- `SAVE-D1-2026-08-20-02` — the sibling guard-reach gap.
- `SAVE-D2-2026-08-20-01` — the same "field added to an already-classified type" shape, on the
  schema side.

## Suggested Fix

Register `CharacterController` — it is delta-safe (nine `f32`s, three `bool`s, one field-less
enum; no `FixedString`/`EntityId`/handle) — and add it to `MUTABLE_DELTA_COLUMNS`, letting
`apply_player_pose` keep zeroing the three momentum fields *after* the overlay so the existing
#2018 behaviour is preserved. That requires a `FORMAT_MAJOR` bump, which `SAVE-D2-2026-08-20-01`
already calls for.

If it is instead left unsaved, correct `crates/physics/src/components.rs:150`'s doc claim and
narrow the allowlist reason to name the breath pair explicitly.

Separately, correct the two `CellRoot` / `spawn_player_character` doc claims — they describe a
mechanism that does not exist.

## Completeness Checks
- [ ] **SIBLING**: every other component on the player body is re-checked for the same "outlives the cell, so an unsaved mutable field leaks" hazard
- [ ] **SIBLING**: the stale `WaterContact` allowlist parenthetical ("drowning accumulation is not yet wired") is corrected in the same pass
- [ ] **TESTS**: a round-trip test pins `breath_remaining` / `drowning_damage_accumulator` across a live load (or, if left unsaved, pins the documented reset)
- [ ] **TESTS**: if registered, `delta_columns_carry_only_session_stable_fields` is extended for the new column
