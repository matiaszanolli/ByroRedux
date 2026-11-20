# #7055 Phase 0 — the Marksman bow shot path (Oblivion)

Status: Phase 0 research, no engine code. There is **no PRD yet**: README T1 (a new gameplay subsystem across the plugin, core, scripting and
the binary, with a save-format question) puts this under the PRD gate, so the sequence is this research, then `/prd new`, review, owner
approval, then the implementation issues. Sources below are cited; anything not traced is marked **verify**.

## Sources and what each settles

| Source | Settles |
|---|---|
| UESP *Oblivion:Marksman* (local dump) | Marksman feeds on "a successful hit on a living target", **0.8 per hit regardless of damage** (matches `SKIL` usage 0 = 0.8, skill-use §1); the perks (Novice fatigue while drawn, Apprentice none, Journeyman zoom, Expert 5 % knockdown, Master 5 % paralysis); a bow *and* an arrow must both be equipped to shoot; total damage is the bow plus the arrow; every arrow shot damages the bow; arrows cannot be reflected; a quick tap drops the arrow at the feet, a held attack draws; the trajectory is ballistic. |
| UESP *Oblivion:The Complete Damage Formula* | `Damage = WeaponRating × (Fatigue/MaxFatigue + 1)/2 × max(SneakMult, PowerMult) × ArmorRating × WeaponResistance`; for bows `WeaponRating` = the bow's plus the arrow's, `Attribute` = Agility, `ModifiedSkill` = Marksman + 0.4 (Luck − 50); `SneakMultiplier` for bows is 2 at Sneak 0–24 and 3 at 25+. |
| CS wiki settings pages (`FArrow*`, local dump) | `fArrowSpeedMult` 1500 u/s; the full speed formula `ArrowSpeed × fArrowSpeedMult × (fArrowWeakSpeed + (1 − fArrowWeakSpeed) × DrawMult)` with `DrawMult = min(1, fArrowBowTimerBase + fArrowBowTimerMult × DrawTime)` (base 0.25, mult 0.4, weak speed 0.01: full speed after 1.875 s); "arrow speed does NOT depend on the speed property of the bow"; `fArrowGravityBase` 0.3, `fArrowGravityMult` 0.002, `fArrowGravityMin` 0.05 (no observed effect), `fArrowWeakGravity` 1.75 (for an arrow not drawn to full), `fArrowFakeMass` 0.25, `fArrowAgeMax` 90 s on the ground or in a world object, `iArrowInventoryChance` 50 % recovered into a struck actor's inventory, `iArrowMaxRefCount` 15, `fArrowMaxDistance` 2000 and `fArrowOptimalDistance` 1000 (AI ranged combat), `iPerkMarksmanKnockdownChance` 5, `iPerkMarksmanParalyzeChance` 5. Also from the CS wiki: `fArrowBounceLinearSpeed` 0.1 (an arrow that bounces comes back at that fraction of its speed), `fArrowFOVZoom` 30 (the Journeyman zoom, gated by Marksman rank). **The gravity integration is not in any local source** (searched the whole of `/mnt/data/src/reference`: the CS wiki, the GECK wiki, the UESP dump for Oblivion and Skyrim, the Gamebryo 2.6 and 3.2 docs and Havok): the four `fArrowGravity*` / `fArrowWeakGravity` pages describe the effect qualitatively ("the larger the multiplier, the longer the arrow flies straight") and give no equation; Skyrim's *Archery* page describes loft, a different game. The CS wiki's `GetLocalGravity` says the cell has a Havok gravity acceleration, which suggests (does not establish) the arrow's gravity is a scale of it. |
| GECK wiki *Complete List of GameSettings* (FO3 / FNV defaults) | A **cross-game divergence to respect**: the same settings read `fArrowBowTimerBase` **0.5** and `fArrowBowTimerMult` **0.6** there (FO3 / FNV), against the CS wiki's 0.25 and 0.4 for Oblivion; the other arrow settings agree (`fArrowSpeedMult` 1500, `fArrowGravityBase` 0.3, `fArrowGravityMult` 0.002, `fArrowWeakGravity` 1.75, `fArrowWeakSpeed` 0.01). Oblivion's draw numbers must come from the Oblivion sources or the executable, never from the FNV list. |
| xEdit / UESP *AMMO* and *WEAP* layouts | Oblivion `AMMO` `DATA` (18 bytes): speed f32, flags u32 (bit 0 *Ignores Normal Weapon Resistance*), value u32, weight f32, damage u16. `WEAP` `DATA` (30 bytes): type u32 (5 = Bow), speed f32, reach f32, flags, value, health, weight, damage u16. Oblivion has **no `PROJ` record**: an arrow's flight is the engine's, from `AMMO` speed and the settings above. |
| `Oblivion.esm` census (this session, raw `DATA` scan) | 128 `AMMO`, **all with speed 1.0** and damage from 2 to 20; 22 carry the *Ignores Normal Weapon Resistance* flag; 131 bows (type 5), all with speed 1.0. **None of the `fArrow*`, `iArrow*` or `iPerkMarksman*` settings is authored in `Oblivion.esm`**, so the executable's defaults are the values in force; the CS wiki's numbers are those defaults and must be confirmed in the executable (below). `fDamageWeaponMult` 0.5, `fDamageStrengthBase` 0.75, `fDamageStrengthMult` 0.5, `fDamageSkillMult` 1.5 are authored. |

## The engine today (re-measured at HEAD `e287dc77f`)

| Fact | Evidence |
|---|---|
| A bow in the player's hand **has no shot**: the attack falls through to the melee swing. `fire_player_gun` takes a weapon only when `ammo_form != 0` and its animation type is not melee; an Oblivion bow has `ammo_form` 0 and `anim_type` 0 (the `WEAP` type is `oblivion_type` 5). | `byroredux/src/combat.rs` `fire_player_gun`; `crates/plugin/src/esm/records/items.rs` (`anim_type` is 0 on Oblivion) |
| **There is no equipped-ammo concept**: no `EquippedAmmo`, no arrow slot, nothing that picks an arrow for a bow. The gun path picks the first accepted ammo the player carries (`player_ammo_for`). | `rg EquippedAmmo` finds nothing; `systems/projectiles.rs` `player_ammo_for` |
| Oblivion `AMMO` parses, but **the arrow's speed and flags are dropped** (`let _speed`, `let _flags`); only damage, value and weight are kept. | `items.rs` AMMO arm for `GameKind::Oblivion` |
| The weapon projectile path flies a straight line and **refuses gravity** ("the gravity unit is unspecified"); it is driven by a `PROJ` record Oblivion does not have. The spell shot of the cast PRD bypasses `PROJ` with a speed and range and no gravity. | `systems/projectiles.rs` `resolve_shot`, `spell_shot` |
| A hit on a living actor queues a `HitEvent` through `PendingTrapHits`, and the skill-use emitter reads the `HitEvent` site (`on_landed_hit`): a projectile hit from the player already reaches it, but **`weapon_skill` returns `None` for type 5**, so the bow would feed nothing even with a shot. | `skill_use_emit.rs` `weapon_skill`, `on_landed_hit` |
| `oblivion_weapon_damage_multiplier` (`crates/core/src/combat.rs`) implements UESP's rating shape and is documented BUILT, **but nothing calls it**: the player's attack uses the weapon's flat base damage plus the FO3/FNV melee bonus. A bow shot's damage rule is therefore also the melee damage rule that is not wired, an inherited gap. | `rg oblivion_weapon_damage_multiplier` has no caller outside `core/src/combat.rs` |
| Input is ready: `ActionState` has `is_held` and `was_released`, the draw needs the held time and the release edge. `inv.add`, `inv.equip` and `inventory.status` exist for a smoke script. | `interaction.rs`, `commands/gameplay.rs` |

## Executable decode: dropped by the owner (2026-11-20)

A delegated decode of `Oblivion.exe` was started for this phase and **stopped before it reported; nothing from it is used**. The owner's ruling: no
executable decode for this work, "exactly the kind of shortcuts we want to avoid". Phase 0 therefore rests on the documented sources above only.

What that leaves **unsettled** (each becomes a PRD decision, a declared gap or a documented-source rule, never a guess):

1. **Gravity** of a ballistic arrow: no local source gives the equation. Straight mode (owner's setting, below) needs none; ballistic stays a declared gap until a
   documented source gives the rule.
2. **Which draw numbers Oblivion uses**: the CS wiki says timer base 0.25 and mult 0.4, the GECK wiki's FO3 / FNV list says 0.5 and 0.6. The CS wiki is the Oblivion
   source and the PRD should take it, labelled as documented and not measured.
3. **The Marksman award site** (once per hit or per shot, the precondition): the documented rule is UESP's "0.8 per successful hit on a living target, regardless of
   damage" (matches the `SKIL` usage-0 value); the engine's existing hit-on-a-living-target gate is that rule.
4. **What a shot spends**: UESP gives the arrow (one per shot, equipped), the bow's wear (every shot) and the Novice fatigue loss while drawn; the *rates* of bow
   wear and of the hold's Fatigue are not documented locally.
5. **Knockdown and paralysis**: the settings (5 % each) are documented; how the roll combines with the target is only UESP's prose.

## Owner decisions taken during Phase 0

- **Arrow trajectory is a setting (owner, 2026-11-20).** The player chooses between a **ballistic** arrow (gravity as the original applies it) and a **straight** one
  (the arrow flies where the crosshair points; any arc is cosmetic), for both Oblivion and Skyrim, "to fit both kinds of players in both games". This is a
  per-player preference, not a per-game rule, so it belongs in the settings registry (`gameplay.*`, like `controls.*` in `interaction.rs`), not in the ruleset
  profile, and it is not saved with the game. Consequences for the PRD: (1) the **straight** mode needs no gravity decode and is always available, the **ballistic**
  mode needs the executable's gravity rule (Open item 3) or a declared gap until it is read; (2) the gravity support goes in the shared projectile flight
  (`systems/projectiles.rs`), which refuses a non-zero gravity today, so Skyrim's later bow reuses it; (3) the default per game is still open (recommended:
  ballistic where the rule is decoded, because UESP's Oblivion *Marksman* page tells the player to aim above the target; straight otherwise); (4) the setting
  changes where an arrow lands, so the PRD's scenarios must pin the mode they run in.
- **Scope: Oblivion and Skyrim together (owner, 2026-11-20).** One PRD covers both bows. Phase 0 therefore also needs Skyrim's documented sources (UESP *Skyrim:Archery*,
  the CK wiki's weapon / projectile / ammunition pages, xEdit's Skyrim `PROJ`, `AMMO` and `WEAP` definitions), which Skyrim's bow needs where Oblivion has none: Skyrim has a
  `PROJ` record (speed, gravity, range, impact data) the engine already parses for FNV. **Not yet researched.**
- **Ballistic is a declared gap in the first release (owner, 2026-11-20).** The setting carries both values; ballistic shows as unavailable until a documented gravity rule
  exists, and every arrow flies straight meanwhile. (Skyrim's `PROJ` gravity may be that documented source for Skyrim; Oblivion has none.)
- **Default is per game (owner, 2026-11-20):** ballistic for Oblivion, straight for Skyrim, each player free to change it. Until ballistic exists the effective trajectory is
  straight in both, and the default of the Oblivion setting resolves to straight with a visible note.
