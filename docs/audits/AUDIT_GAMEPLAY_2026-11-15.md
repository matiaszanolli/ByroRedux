**HEAD**: c0a91d8fa (audited) · **Baseline**: AUDIT_GAMEPLAY_2026-10-31.md · **Scope**: the gameplay layer's changes since 10-31. Inventory and equipment, interaction, combat with VATS and limbs, AI producers (Find, packages, travel), perks' gameplay consumers and chargen. The fixes earlier tonight were excluded.

# Gameplay Audit: 2026-11-15

**Baseline run**: the skill's Phase 1 filter set passed (697 passed, 0 failed, 32 ignored).

| ID | Sev | Finding | Outcome |
|---|---|---|---|
| GAME-1 | MEDIUM | A Find package walked to disabled placements and took phantom items. A quest item was tombstoned before its quest enabled it. This is the sibling #6539 missed. | 7a969f688: `still_valid` checks the enable ledger. Test `a_find_passes_over_a_disabled_placement`. |
| GAME-2 | MEDIUM | The Melee Damage bonus (STR × 0.5) was added to every weapon, so VATS gun shots got it too. GECK *Weapon Damage Formula*: melee weapons only. | b6315be1c: gated on the weapon's animation type. Test `the_melee_damage_bonus_is_for_melee_weapons_only`. |
| GAME-3 | MEDIUM | The player's `WeaponDrawn` is never cleared, so `IsWeaponOut` stays 1 after the first attack. | #6942: needs a holster rule. |
| GAME-4 | LOW | Unarmed and hand-to-hand swings skip the crippled-arm penalty. | #6943: which arm counts is unsourced. |
| GAME-5 | LOW | The doc comment for `record_killing_blow` sat on `draw_weapon`. | 006770e3a |

**Checked clean**:
- `vats_sequence_end_system` runs after `combat_damage_system`, and `HitParts` / `HitShots` are drained.
- The held perk activate-choice resolves in `interaction_system`.
- `CrossSpaceHop` is in the ambient removers, so a death mid-fade calls the fade off.

**Not covered**:
- Dimension 3, which changed only in afd260aeb.
- Dimension 6 beyond chargen. `chargen::apply` does not enforce the SPECIAL total or the trait count; only the debug-UI page does. That path is console-only and is not filed.
- Dimension 7 beyond its guards.
