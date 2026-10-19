# SAVE-SD-03: `ReferenceState` (eviction and #5054 ring park) lags the registry — extends GAME-SD-01 / GAME-SD-02 with new columns and an asymmetric permanent-stat hazard

Labels: medium,save-load,gameplay,scripting,bug

**Severity**: MEDIUM · **Dimension**: Snapshot Completeness (third list) · **Data-Loss Class**: silent-drop
**Location**: `byroredux/src/cell_loader/reference_state.rs:27-62` (struct), `:119-264` (`capture`), `:682-802` (`park_unresolved_snapshot_rows`); consumers `crates/scripting/src/magic_effects.rs:313-359`

**Extends** #6071 (GAME-SD-01: `TwoStateActivator`, `ScriptVariables`) and #6072 (GAME-SD-02: `ScriptPackage`) with distinct missing components and an asymmetric permanent-stat hazard. It does not re-report those two. Companion: #6073, #6074.

## Description (new facts)
1. **`ActiveMagicEffects` is asymmetric with the parked `ActorValues`.** A *Recover* value effect applies `mod_permanent(+amount)` once at start and `mod_permanent(-amount)` when it finishes (`magic_effects.rs:325-326`, `:356-358`). `capture` parks the actor's `ActorValues` (with the modification in it) but not the `ActiveMagicEffects` list that owns the pending revert. After an eviction round trip the respawned actor gets the modified values back and no effect to undo them, so the buff or debuff is permanent. The ring park of #5054 does the same from the snapshot columns. This is the hazard #4819 fixed for `SpellList` vs `ActorValues`, reappearing for timed effects.
2. **`WeaponDrawn` and `IgnoreCrime`** (registered replacing columns, `obscript_state::register_saved`) are not parked: a scripted `SetAlert` / `IgnoreCrime` reverts on cell exit.
3. **`ScriptTimer` and `RumbleOnActivate`** (registered columns on placed references, the latter in `MUTABLE_DELTA_COLUMNS`) are not parked: a spent trap/lever timer or `Busy` rumble state re-arms on re-entry. GAME-SD-01 names only `TwoStateActivator` and `ScriptVariables`.
4. **The ring park cannot save the actors' scripted state either.** `park_unresolved_snapshot_rows` builds `ReferenceState` from the same ten columns, so every item above (and GAME-SD-01/02) is also dropped for the `radius_load + 1` hysteresis band at save-load time.
5. **Doc drift**: `docs/engine/stream-boundary-state-continuity.md:130-135` justifies dropping script-local state at the stream boundary with "canonical save/load already owns this boundary (`ScriptVariables`/`ScriptTimer` are registered)". Those are *components*: they survive a save of the resident cell but are despawned with the entity on every ordinary unload, so save/load does not own the eviction boundary.

## Evidence
`ReferenceState` fields are exactly `inventory, equipment, weapon, actor_values, spells, perks, control, bleedout, dead, picked_up`. `grep` for `ActiveMagicEffects|WeaponDrawn|IgnoreCrime|ScriptTimer` in `cell_loader/` finds no hit. `ScriptTimer` is registered at `save_io/registry.rs:60`. Every spawned non-player actor gets an `Inventory` (`npc_spawn/resumable/runtime.rs:387,510`), so `capture`'s keep rule (`reference_state.rs:197`) does not itself hide these actors; the fields are simply absent.

## Impact
Timed buffs on streamed-out NPCs become permanent; scripted alert/ignore-crime flags and spent trap/lever timers reset on every cell exit. All of it is new M47.3 content, so the blast radius is limited to scripted FNV/FO3/Oblivion content, hence MEDIUM. Adding fields bumps `FORMAT_MAJOR` per GAME-SD-01/02's proposed fix.

## Related
#6071, #6072, #6073, #6074, #4819, #5859.

## Suggested Fix
Fold the new columns into the same `ReferenceState` extension (one `FORMAT_MAJOR` bump), and park `ActiveMagicEffects` together with `ActorValues` as one unit. A guard that diffs `ReferenceState` fields against registered per-placement columns, with an explicit allowlist, would stop the third list drifting again.

Source: docs/audits/AUDIT_SAVE_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-placement components absent from the eviction row)
- [ ] **TESTS**: A regression test pins this specific fix
