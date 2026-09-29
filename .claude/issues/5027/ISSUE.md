# #5027 — GAME-D7-2026-09-29-01: A dead player stays `Dead` after loading a save — the additive overlay never clears the marker from the process-lifetime player

**Labels**: high,gameplay,save-load,bug

**Source report**: `docs/audits/AUDIT_GAMEPLAY_2026-09-29.md`
**Severity**: HIGH
**Dimension**: 7 — Gameplay-state coverage (saved-or-rederived)

## Location
- `byroredux/src/save_io.rs` — `.register_component::<Dead>("Dead")` (plain additive registration) and `"Dead"` in `MUTABLE_DELTA_COLUMNS`
- `byroredux/src/save_io.rs` — post-overlay reconcilers after `apply_deltas` in the load drain (`reconcile_dead_actor_runtime_state`, `reconcile_player_equipped_weapon`)
- `crates/save/src/registry.rs` — `register_component` vs `register_replacing_component`

## Description
`Dead` has a plain, additive save registration. Only `register_replacing_component` makes a saved *absence* authoritative for FormID-matched entities; `Perks` and `TimedRestorations` use it precisely so the overlay clears a saved absence on the live player. The player entity is process-lifetime and survives the cell reload untouched. After the overlay only two reconcilers run: `reconcile_dead_actor_runtime_state` (acts only on entities that *have* `Dead`) and `reconcile_player_equipped_weapon`. Nothing removes the pre-load session's `Dead` from the player.

## Evidence
- `grep -rn 'remove::<Dead>'` finds no production site; all five hits are inside `#[cfg(test)]` modules (`inventory.rs`, `interaction.rs`, `loot_appearance.rs`, `npc_dialogue.rs`).
- `round_trip_tests::delta_columns_removed_at_runtime_have_a_load_reconciler` stays green because it scans runtime *removal* sites; this gap is a runtime *insert* on an entity that outlives the reload.
- #4701's body states there is no game-over or reload flow — loading a save is the only recovery path.

## Impact
After any player death (drowning via `character.rs`, or an NPC kill via `combat_damage_system`), loading a save where the player was alive restores `ActorValues` (health > 0) but keeps `Dead`: `player_controller` refuses movement, `player_can_act` blocks attack/activate/loot/equip, and `faction_hostility` treats the player as a non-target. No in-session recovery short of restarting the engine. Secondary: the death teardown stripped the capsule's idle `AnimationPlayer` (attached by `db8351587`); nothing re-attaches it after a "revival".

## Related
#3488 (closed, `EquippedWeapon` only), #4701, #3022. Same process-lifetime-player-survives-load class: SAVE-D1-2026-09-29-01 (#5052) (`ActorControlState`), SAVE-D5-2026-09-29-02 (#5056) (cinematic state), SAVE-D1-2026-09-29-02 (#5058) (alias factions), GAME-D1-2026-09-29-02 (#5034) (worn meshes), SAVE-D5-2026-09-29-01 (#5054) (outer-ring rows). One sweep over the additive-registration components is warranted.

## Suggested Fix
Register `Dead` with `register_replacing_component` (saved absence on a FormID-matched actor is authoritative; resident NPCs respawn alive anyway), or add a post-overlay player reconciler that clears `Dead` and re-runs the body's locomotion attach. Pin with a dead-player → load-alive-save test.

Validated at HEAD 9fcfdc3fc: `Dead` registered via plain `register_component`; post-`apply_deltas` block runs only the dead-actor and equipped-weapon reconcilers; every `remove::<Dead>` hit is below the file's `#[cfg(test)]`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
