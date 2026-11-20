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

## Skyrim (documented sources only, 2026-11-20)

Sources: UESP *Skyrim:Archery* and *Skyrim:Leveling* (local dump), the CK wiki's *Projectile* page and its *Category:Settings* game-setting list, the UESP Mod File
Format pages for `PROJ`, `AMMO` and `WEAP`, and a census of `Skyrim.esm` (SE). No executable was read.

| Topic | What the sources give | Status |
|---|---|---|
| Draw timing | Nock about 0.4 s, draw `1.66 s / (weapon speed × (1 + Quick Shot 0.3))`, release and rest about 0.6 s: a Long Bow (speed 1) takes 2.66 s (UESP *Archery*, "appears to"). Bow weapon speeds are authored (0.5 Daedric to 1.0). | Documented (observational) |
| Damage | "The sum of the quality and material of the bow and the arrow, augmented by the bow's draw length. Misfired arrows launched before fully drawing the bow result in a damage penalty"; `+0.5 %` damage per Archery skill point; the Overdraw perk +20 % per rank up to ×2; Critical Shot 10 / 15 / 20 %; Power Shot staggers 50 %; Bullseye 15 % paralysis at 100. **The draw-length scaling has no formula**; the settings `fArrowBowMinTime` 0.8, `fArrowMinPower` 0.35, `fArrowMinBowVelocity` 0.7, `fArrowMinVelocity` 0.2, `fArrowBowFastMult` 1, `fArrowBowSlowMult` 0.5 are listed with no description of how they enter. `fArrowSpeedMult` is listed "No effect". | Partly documented; the partial-draw rule is a **gap** |
| Trajectory | The arrow leaves tilted up: `f1PArrowTiltUpAngle` 2, `f3PArrowTiltUpAngle` 2.5 (bolts 1), "shoots slightly above the crosshair"; speed and gravity come from the arrow's `PROJ` (`DATA`: gravity, speed, range); "slower bows shoot higher with less drop"; arrows are independent of the skill. Beyond `fVisibleNavmeshMoveDist` (4096) arrows "phase through targets without doing any damage". The CK wiki says only "gravity: how much gravity has an effect on the projectile": **the unit is unspecified**. | Inputs documented; **the gravity unit is a gap** |
| `PROJ` / `AMMO` | `Skyrim.esm`: 128 `PROJ`, 20 of type Arrow; 17 of those have gravity 0.35, speed 3600 and range 60000 (the others: a quest arrow 0 / 15000 / 60000, and speeds 1000 and 2000); 35 `AMMO` (`DATA`: projectile, flags, damage, value, weight). | Census (plugin data) |
| Skill XP | `Skill Use Mult × (base XP × skill-specific multipliers) + Skill Use Offset`, Archery use mult **9.3**, offset 0, improve mult 2; the base XP of a bow shot is "Base Weapon Damage of the Bow" with "additional multipliers may apply"; the page marks it "additional verification needed", and the Archery page calls its own equation "thought to resemble". Weapon XP needs a *damaging* hit on a valid target. | **Provisional** at best |
| Hands and handling | The bow is equipped like any weapon; "arrows remain equipped even if you unequip the bow"; Block bashes with the bow (stamina), Eagle Eye zoom on Block, no blocking with a bow equipped. | Documented |

### Skyrim: the engine today (re-measured at HEAD `2d9bfe7e9`)

- A Skyrim bow in the player's hand **also falls through to the melee swing**: `fire_player_gun` needs `ammo_form != 0`, and the `WEAP` arm that sets `ammo_form` is FO3 / FNV only ("Skyrim uses ETYP for ammo type and per-arrow NAM7; not yet decoded", `items.rs`). Skyrim's ammunition is the *equipped* arrow, which has no engine concept either (`rg EquippedAmmo` is empty).
- `resolve_shot` would take a Skyrim arrow (`PROJ` is parsed; `AMMO` carries `projectile_form`) but **refuses gravity** unless the projectile is hitscan, and all 17 common arrows have gravity 0.35: ballistic Skyrim arrows hit the same declared gap as Oblivion's, and a straight mode simply ignores the field.
- `MELEE_ANIM_TYPES = [1, 2]` and `TWO_HANDED_ANIM_TYPES` are the **FO3 / FNV** `wbWeaponAnimTypeEnum` (1 one-hand, 2 two-hand); Skyrim's `DNAM` byte 0 is a different enum (1 sword, 2 dagger, 3 axe, 4 mace, 5 two-hand sword, 6 two-hand axe, **7 bow**, 8 staff, 9 crossbow), so the melee-or-ranged split is wrong for Skyrim weapons generally (an axe or a mace counts as not melee). A cross-game fix the bow depends on, not a bow rule.
- No Skyrim weapon-hit skill emitter exists: `PendingSkillAdvances` is fed only by scripts and crafting, so Archery XP is a new emitter on Skyrim's XP model, with the same hit gate as Oblivion's.
- The perks (Overdraw, Critical Shot, Power Shot, Quick Shot, Bullseye) ride the perk entry-point work ([Perk Entry Points PRD](../../docs/prd/perk-entry-points.md)); their hooks would be the places they land, not bow code.

### What the two games share (the PRD's common core)

An equipped-ammo concept and the arrow-per-shot spend; hold-to-draw with a release edge (`ActionState::is_held`, `was_released`); a straight-line arrow on the shared projectile
flight with the gravity branch left as a declared gap; the trajectory setting in the settings registry; the damage handed to the existing `HitEvent`; the Marksman / Archery feed
on a damaging hit to a living target. Per game: Oblivion's draw timer and settings (CS wiki), damage rating (UESP) and Marksman 0.8; Skyrim's draw time, `PROJ` speed, tilt angle,
Archery use mult 9.3 (provisional) and the perk layer.
