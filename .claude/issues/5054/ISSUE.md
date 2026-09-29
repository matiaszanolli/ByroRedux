# #5054 — SAVE-D5-2026-09-29-01: Exterior load drops every saved row of the hysteresis-band cells — the reload bootstraps `radius_load`, but a save taken after any cell crossing holds resident cells out to `radius_unload = radius_load + 1`

**Labels**: high,save-load,bug

**Source report**: `docs/audits/AUDIT_SAVE_2026-09-29.md`
**Severity**: HIGH
**Dimension**: Live Load-Apply & Frame Boundary — data-loss class: silent-drop

## Location
- `byroredux/src/save_io.rs` — exterior reload: `build_exterior_world_context(.., ext_ctx.radius_load, ..)` and `assemble_exterior_streaming(.., ext_ctx.radius_load, exterior_reload_bootstrap_mode(), ..)`
- `byroredux/src/streaming.rs` — `radius_unload: radius_load + 1`; eviction only beyond `radius_unload`
- `byroredux/src/scene/world_setup.rs` — `stream_initial_radius` (deltas from an empty `loaded` set against `radius_load`)
- `crates/save/src/driver.rs` — `build_form_id_remap` (unresolved pairs logged, then dropped)
- `byroredux/src/cell_loader/reference_state.rs` — rows exist only for non-resident placements; `restore_resident` consults only the parked store

## Description
`CurrentExteriorContext.grid` follows the player (`app_step.rs`). After one crossing, the ring at Chebyshev distance `radius_load + 1` is still resident. The state of its placements lives only in component columns (parked rows are consumed on respawn; resident placements are never parked). `save_world` captures those rows. The load then streams only `radius_load` around the saved grid, so every band `FormIdPair` fails to resolve: `build_form_id_remap` logs one WARN block and `apply_deltas` skips the rows. Nothing turns the unresolved rows into `PersistentReferenceStates` entries, so the band cells later respawn from ESM.

## Evidence
The reload path passes `radius_load` both to `build_exterior_world_context` and to the bootstrap. `stream_initial_radius` computes deltas from an empty `loaded` set against `state.radius_load`. `restore_resident` iterates `FormIdComponent` entities and applies parked rows only, never the snapshot's unresolved rows.

## Impact
After loading an exterior save, every band placement loses `Inventory`, `EquipmentSlots`, `EquippedWeapon`, `Dead`, `ActorValues`, `SpellList`, `Transform` (moved objects), activator/script state and AI procedure markers. On return, killed NPCs are alive and looted containers are restocked while the loot is already in the loaded inventory — item duplication. Trigger: loot or kill something, move so its cell sits exactly `radius_load + 1` cells away (4 at radius 3), save, load.

## Related
#3280 (closed sibling: reload waits for every cell inside `radius_load`; the hysteresis ring was never considered), #3499, #4695; save-load-teardown theme: GAME-D7-2026-09-29-01 (#5027), SAVE-D1-2026-09-29-01 (#5052), SAVE-D5-2026-09-29-02 (#5056), SAVE-D1-2026-09-29-02 (#5058), GAME-D1-2026-09-29-02 (#5034).

## Suggested Fix
At load time, park the snapshot rows the remap could not resolve into `PersistentReferenceStates` — build `ReferenceState` from that pair's `Inventory`/`EquipmentSlots`/`EquippedWeapon`/`ActorValues`/`SpellList`/`Dead` rows (the shape `capture` writes). Alternatively bootstrap the reload to the saved resident set (`radius_unload`). Test: a snapshot with a band-cell row through an unresolved remap asserts the row is parked.

Validated at HEAD 9fcfdc3fc: exterior reload passes `ext_ctx.radius_load` to both context build and `assemble_exterior_streaming`; `radius_unload = radius_load + 1`; `ectx.grid = player_grid` on crossing; `build_form_id_remap` only logs unresolved pairs.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
