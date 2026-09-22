# CHAR-2026-09-21-D4-01: Player populated through the NPC path — PlayerOnly ruleset rows never evaluated (FO4 Health/AP wrong, FO3/FNV AP missing)

**Severity**: MEDIUM
**Dimension**: Population Boundary
**Game**: FO4 (wrong values); FO3/FNV (missing AP); all (structural)

## Description

`build_player_character_template` (`byroredux/src/inventory.rs`) derives the player's `ActorValues` with `derive_npc_actor_values(player_npc, index)` — the NPC population function. For the player, several ruleset rows are `PlayerOnly` derived formulas: FO3/FNV/FO4 Health and AP, Skyrim Light Armor, and Oblivion's pools. The capture documents state the player's live values come from those formulas and NPCs ship baked values. The code instead gives the player the NPC answer, and nothing ever evaluates the `PlayerOnly` rows for the player:
- `GetActorValue` (`crates/scripting/src/condition.rs`) excludes `PlayerOnly` for every entity. Its comment still says "the player isn't modelled yet".
- `vitals_snapshot` (`byroredux/src/inventory.rs`) reads carried values only.
- No other consumer asks.

Real-master results (read-only probe of Player `NPC_` `0x00000007`):
- **FO4** (`Fallout4.esm`: PRPS SPECIAL 1×7, ACBS level 1): PRPS also authors `Health=40.0` and `ActionPoints=0.0`, and DNAM authors calc_health **150** and calc_ap **100**. The DNAM pairs are pushed after PRPS, and `from_pairs` is last-write-wins, so the player carries Health **150** and AP **100**. The capture's player formulas give **85** and **70** (1.76x / 1.43x off).
- **FO3/FNV** (auto-calc off, `PlayerClass` ATTR 5x7, level 1): Health 200 matches the capture only because the NPC curve equals the player formula at L1/END5. `ActionPoints` is never seeded.

## Evidence

Verified at HEAD `ee6d3fb39`. The two FO4 values come straight from `crates/plugin/src/esm/records/actor_value_derive.rs`:
```rust
out.extend_from_slice(props);                    // PRPS: (Health, 40.0), (ActionPoints, 0.0), …
for (avif_editor_id, baked) in [("Health", …calculated_health), ("ActionPoints", …)] {
    if baked > 0 { … out.push((fid, f32::from(baked))); }   // (Health, 150.0), (ActionPoints, 100.0)
}
```
The FO3/FNV AP result comes from `crates/scripting/src/condition.rs`'s `GetActorValue` arm: the carried fast path misses, the AP row is `PlayerOnly`, only `ActorGeneral && Absolute` rows route through `CharacterRuleset::derived_value`, and the function returns `0.0`.

## Impact

- **FO4**: the carried values win in `GetActorValue`, the native HUD, `combat_damage_system` (the player's Health pool now takes NPC `StartCombat` strikes) and drowning. A temporary END change cannot rescale HP, which the capture requires.
- **FO3/FNV**: `player.GetActorValue ActionPoints` reads 0.0 (should be 80 FNV / 75 FO3). The native HUD's AP bar never draws. `install_catalog_resolves_per_game_vital_keys` asserts only that the AP *key* resolves, and no test composes a production-stamped player.
- Latent: `consume_item` rejects a whole item when any effect's AV is not carried, and the plugin maps FO3/FNV MGEF AV 12 -> `ActionPoints`. Any AP-restoring ingestible would therefore be unusable by the player. No vanilla item in today's supported restorative set targets AP.

## Related

#4458 (the fix this follows from — correctly fixed, the stamp is live), #2937 (FO3/FNV AP scope), #4452 (the other `DerivedScope` consumer-contract gap), CHAR-2026-09-21-D5-01 (the docs that hide this), CHAR-2026-09-21-D4-03 (the rest of the half-populated player), CHAR-2026-09-21-D4-02 (same PRPS/DNAM collision, NPC-wide). Related but structurally distinct from `AUDIT_SCRIPTING_2026-09-22.md`'s SCR-D3-2026-09-22-01 (`resolve_entity_by_global_form_id` can never resolve the player by its `0x14` FormID — an entity-*resolution* gap; this finding is a formula-*evaluation* gap for an entity that already resolves correctly). Both stand independently.

## Suggested Fix

Seed only SPECIAL/skills (and FO3/FNV body conditions) from the Player record. Then give the player its `PlayerOnly` rows: evaluate them for `PlayerEntity` when stamping, or let `GetActorValue`/`vitals_snapshot` fall through to `derived_value` for the player. Drop the NPC-baked Health/AP from the player seed. Add a real-master leg asserting FO4 Health 85 / AP 70 and FNV AP 80.

Source: docs/audits/AUDIT_CHARACTER_2026-09-21.md (CHAR-2026-09-21-D4-01)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
- [ ] **TESTS**: A regression test pins this specific fix
