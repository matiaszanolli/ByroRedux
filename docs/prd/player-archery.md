# Player Archery — Product Requirements

**Status**: APPROVED (2026-11-20). Revision 8 records the live gates passing. Revision 7 records Phase 3 as built. Revision 6 records Phase 2 as built. Revision 5 records Phase 1 as built. Revision 4 closes Phase 0. Revision 3 records the owner's approval and the recommended answers to Q3 to Q6. Revision 2 folds in review round 1 (the spend-moment citation, the settled Fallout weapon types, the per-game trajectory default, the missing owning designs, the Skyrim anim-type source, the arrow's condition term, the §8 metrics, Q4's deviation row). Revision 1 is the first draft, from the Phase 0 research in `.claude/issues/7055/INVESTIGATION.md`. Statements about how an original engine behaves cite UESP, the CS wiki or the CK wiki (the local dumps under `/mnt/data/src/reference/`) and `Oblivion.esm` / `Skyrim.esm` censuses; what they do not settle is marked **Phase 0: verify** or recorded as a declared gap. No executable was decoded for this PRD (owner ruling, 2026-11-20).

**Scope**: The player shoots a bow: the arrows they have equipped, drawing and releasing, the arrow that flies and the damage it deals to a living target, and the one consumer that motivates it for Oblivion, the Marksman skill use of a hit. Oblivion and Skyrim are both in Wave 1 (owner, 2026-11-20): the shot, the damage and the trajectory setting are shared; the Marksman feed is Oblivion's, and Skyrim's Archery experience waits on a Skyrim weapon-skill emitter (Q3). Crossbows, NPC archers, enchanted and poisoned arrows, arrow recovery, the bow's perks, zoom, animation, sound and visuals are not here (§4.2).

**Related**: this document states the *promise*; the designs below own the *how*. Every mechanism promised here has an owner in this table, or Phase 0 writes the missing design.

| Mechanism | Owning design |
|---|---|
| The skill-use drain, `FED_SKILLS` and the emitter contract (BOW-USE) | [Oblivion skill use](../engine/skill-use.md) |
| Oblivion's damage rating and the per-game rules profile (BOW-MOD, BOW-DMG) | [Oblivion ruleset](../engine/charal-oblivion-ruleset.md) ("The Complete Damage Formula"); [Skyrim ruleset](../engine/charal-skyrim-ruleset.md) |
| The player-cast PRD's pattern for a ruleset row, an input action, a console report and a smoke gate (BOW-MOD, BOW-DX) | [Player spellcasting](player-spellcasting.md) |
| Scheduler access and the combat chain the shot joins (BOW-DRW) | [Game loop](../engine/game-loop.md) |
| Sneak damage of a shot from a hidden player (BOW-DMG-004) | [Sneak and detection](../engine/sneak-detection.md) |
| The equipped-ammo component, its save and the save-format bump (BOW-AMM-002) | [Save and load round trip](../engine/save-load-roundtrip.md); the equip slot's section is "Equipped ammo" there (Phase 0) |
| The shared projectile flight and hit path (BOW-G-004, BOW-FLT-005, BOW-DMG-003) | [Game loop](../engine/game-loop.md), "The projectile path" (Phase 0) |
| The settings registry the trajectory setting joins (BOW-SET) | [Launcher](../engine/launcher.md) §4.4, the registration contract (Phase 0) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119 meanings, as in the related designs. Requirement IDs use the `BOW-` prefix.

---

## 1. Product thesis

A bow is the engine's second player ranged weapon and the first one a game other than Fallout has: today the attack key with a bow in hand swings it like a club (§3). The owner wants both Oblivion's and Skyrim's bows to work, wants the player to choose between an arrow that goes where the crosshair points and one that falls as the original's does, and wants nothing invented where the documentation is silent.

The corollary that shapes every requirement: **a documented rule is built as documented, and a missing rule is a declared gap, never a plausible guess.** Where the documents give a rule (Oblivion's draw timer and speed, both games' damage terms, Skyrim's draw time) the engine follows it; where they do not (the gravity unit, the partial-draw penalty, bow wear, the Fatigue of a held draw) the first release says so in `bow.status` and does not fake it.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Bow** | A weapon the engine fires arrows from: an Oblivion `WEAP` whose `DATA` type is 5, a Skyrim `WEAP` whose animation type is 7 (xEdit `wbWeaponAnimTypeEnum`, `wbDefinitionsTES5.pas:2619`: 7 Bow, 9 Crossbow; copies under `/mnt/data/tmp/`). Oblivion's type 5 is UESP *Oblivion Mod:Mod File Format/WEAP*. |
| **Equipped ammo** | The arrow stack the player has chosen to shoot: Oblivion's "must both be equipped" (UESP *Oblivion:Marksman*) and Skyrim's arrows that "remain equipped even if you unequip the bow" (UESP *Skyrim:Archery*). |
| **Draw** | The time the attack input is held with a bow readied; the release edge fires. |
| **Draw fraction** | Oblivion's `DrawMult = min(1, fArrowBowTimerBase + fArrowBowTimerMult × draw time)` (CS wiki *fArrowSpeedMult*); Skyrim's draw is complete after `1.66 s / (weapon speed × (1 + Quick Shot))` (UESP *Skyrim:Archery*). |
| **Straight arrow** | An arrow that flies along the aim at its speed with no drop. |
| **Ballistic arrow** | An arrow that falls under gravity as the original's does. A declared gap in the first release (§5.4). |
| **Valid hit** | A hit that damages a living actor: not a miss, a wall, a corpse or a hit that deals nothing (UESP *Oblivion:Marksman*: "successfully hit a living target"; UESP *Skyrim:Archery*: "valid targets, those whose health can be damaged"). |

---

## 3. Core model

A shot has five stages: **ready** (a bow in hand and an arrow equipped), **draw** (the input held), **release** (the arrow leaves, one is spent), **flight** (a straight or ballistic path) and **hit** (a damage event and, on a valid hit, the skill use). The per-game numbers belong to one rules row, `BowModel`, on the character rules profile (as `SpellCastModel` is for casting); the shot code asks it and holds no constant (CLAUDE.md invariant 1: no per-game branch past the translation boundary).

The inventory below is the state at HEAD on 2026-11-20.

| # | Item | Class | Today |
|---|---|---|---|
| 1 | A shot from a bow | built (Phase 1, 2026-11-20) | `bow::player_bow_system` draws and releases and `bow::shoot_now` gates, spends and launches; a wielded bow no longer swings (`bow_in_hand` in `combat_input_system`). Before it: the attack fell through to the melee swing: `fire_player_gun` (`byroredux/src/combat.rs:455`) takes a weapon only when `ammo_form != 0`; an Oblivion bow has `ammo_form` 0 and the arm that sets it for Skyrim does not exist (`crates/plugin/src/esm/records/items.rs:641`, "Skyrim uses ETYP for ammo type … not yet decoded") |
| 2 | Equipped ammo | built (Phase 1) | `EquippedAmmo(Option<u32>)`, saved (format v141), set by `inv.equip` on an arrow where the game has a bow row. Before it: no slot existed; the gun path picks the first accepted ammunition the player carries (`player_ammo_for`, `byroredux/src/systems/projectiles.rs:192`) |
| 3 | Arrow data | decoded (Phase 1) | Oblivion `AMMO` `DATA` speed and flags are kept (`ItemKind::Ammo::{speed, flags}`) and Skyrim's `WEAP` `DNAM` speed is kept (BOW-DRW-003; a side effect: Skyrim melee weapons' authored speed now scales their attack cooldown, as Fallout 4's already does); Skyrim `AMMO` carries `projectile_form` and `PROJ` carries gravity, speed and range |
| 4 | The flying projectile | exists, straight | `ActiveProjectiles` flies a straight line and **refuses a non-hitscan projectile with gravity** (`systems/projectiles.rs:99`, "the gravity unit is unspecified"); 19 of Skyrim's 20 arrow `PROJ` records have gravity 0.35 (census 2026-11-20), so those would not fly. Oblivion has no `PROJ` |
| 5 | Oblivion's weapon damage rating | built, unwired | `oblivion_weapon_damage_multiplier` (`crates/core/src/combat.rs:62`) has no caller outside its module; the player's attack uses the weapon's base damage |
| 6 | The skill feed | built (Phase 3, Oblivion) | `weapon_skill(Some(5))` is Marksman, code 28 is in `FED_SKILLS`, and `on_landed_hit` feeds from `HitEvent::arrow` (BOW-USE-005). Before it: `weapon_skill` returned `None` for type 5 and the emitter read the wielded weapon; Skyrim has no weapon-skill emitter (`PendingSkillAdvances` is fed by scripts and crafting only) |
| 7 | Weapon classification | bow built (Phase 1) | A bow is named by the rules row (`BowModel::is_bow`: Oblivion type 5, Skyrim animation type 7), so a Skyrim bow no longer falls to the melee swing; `MELEE_ANIM_TYPES = [1, 2]` stays the FO3 / FNV enum (`byroredux/src/limbs.rs:120`); Skyrim's byte 0 differs (7 bow, 3 axe, 4 mace) |
| 8 | Hold and release input | exists | `ActionState::is_held` and `was_released` (`byroredux/src/interaction.rs:785`, `:793`) |
| 9 | Settings | exists | The settings registry carries `controls.*`; no arrow-trajectory entry |

---

## 4. Goals and non-goals

### 4.1 Goals

- **BOW-G-001 — A bow shoots.** On Oblivion and Skyrim the player readies a bow, equips arrows, draws and releases, and an arrow flies and damages the living actor it meets.
- **BOW-G-002 — Oblivion's Marksman levels by shooting.** A valid hit feeds Marksman through the skill-use drain, and nothing else feeds it.
- **BOW-G-003 — The player chooses the trajectory.** A setting selects a straight arrow or, when it exists, a ballistic one, with a default per game.
- **BOW-G-004 — One path.** The player's shot, a script's projectile and the gun's shot share the one flight and one damage path; the player's shot alone gates, spends and feeds.

### 4.2 Non-goals

- **BOW-NG-001.** Crossbows (Skyrim, Dawnguard) and Fallout's guns beyond what already fires; a crossbow is the same model with its own `BowModel` row and is a later PRD.
- **BOW-NG-002.** NPC archers. An NPC firing at the player is the AI packages PRD's; the shared flight and damage path (BOW-G-004) is all this PRD asks of it.
- **BOW-NG-003.** Enchanted and poisoned bows and arrows, bound bows, and arrow-borne spell effects. They add effects, not a shot.
- **BOW-NG-004.** Arrow recovery from corpses and the ground, arrows sticking in or bouncing off scenery and actors (`fArrowAgeMax`, `fArrowBounce*`, `iArrowInventoryChance`, Skyrim's Hunter's Discipline). The arrow that hits or misses ends.
- **BOW-NG-005.** The bow's perks and skill bonuses beyond the damage terms of §5.5: Oblivion's Novice Fatigue hold, zoom and the knockdown and paralysis rolls, Skyrim's Overdraw, Critical Shot, Power Shot, Quick Shot, Bullseye, Eagle Eye, Steady Hand and Ranger. Skyrim's perks ride the perk entry-point work, not bow code.
- **BOW-NG-006.** Animation, sound, quiver and arrow visuals and the draw-back camera. The shot's feedback is the log and the console report until the engine has them.
- **BOW-NG-007.** Bow wear (UESP *Oblivion:Marksman*: "every arrow shot damages the bow") and the Fatigue of a held draw. Both exist in the original and neither has a documented rate (Q5).
- **BOW-NG-008.** Mounted archery, bow bashing and Skyrim VR.

---

## 5. Requirements by area

### 5.1 Equipped ammo (`BOW-AMM`)

- **BOW-AMM-001.** The player MUST have an equipped-ammo slot holding the form id of an arrow type in their inventory, or none. Equipping an `AMMO` item through `inv.equip` MUST set it; an arrow type that leaves the inventory (spent, dropped, removed) MUST clear it. Built in Phase 1 (§3 row 2).
- **BOW-AMM-002.** The equipped ammo MUST be saved with the game and a loaded save that names an arrow no longer present MUST clear it without an error. The save-format version MUST change with the component (README T2) and the registry's round-trip and fingerprint guards MUST cover it.
- **BOW-AMM-003.** A shot MUST spend exactly one arrow of the equipped type, and a shot with none equipped or none left MUST be refused with `no arrows`, spending nothing. The moment of the spend per game (Oblivion's "once nocked, it is impossible to put the arrow back into the quiver", UESP *Oblivion:Combat*; *Oblivion:Marksman* instead lists three ways to remove a nocked arrow "preserving the enchantment charge or poison": a conversation, a Journeyman Acrobatics dodge and a stagger; Skyrim's cancel of a drawn bow with Sheath Weapon, UESP *Skyrim:Archery*) is a `BowModel` field. Phase 0 result: the documents do not settle the trigger. *Oblivion:Marksman* says of the Dodge removal that "a look at your ammunition and/or enchantment charges should show that no shot was actually taken", while *Oblivion:Combat* says only that a nocked arrow cannot return to the quiver; for Skyrim, UESP says a drawn bow "can be cancelled" with Sheath Weapon and says nothing about the arrow. The spend is therefore at the release on both games, a **declared gap** that `bow.status` names, and none of the three Oblivion removals (a conversation, a Journeyman Dodge, a stagger) is modelled by this PRD.
- **BOW-AMM-004.** The Oblivion `AMMO` speed and the "Ignores Normal Weapon Resistance" flag MUST be kept by the record decode (xEdit / UESP *Oblivion Mod:Mod File Format/AMMO*), and Skyrim's equipped-ammo choice MUST read the `AMMO` `DATA` the decode already carries.

### 5.2 The draw and the release (`BOW-DRW`)

- **BOW-DRW-001.** With a bow readied, holding the attack action MUST start a draw and its release edge MUST fire the arrow; a bow MUST NOT swing as a melee weapon (§3 row 1). The edge reads `ActionState::is_held` and `was_released`.
- **BOW-DRW-002.** Oblivion's draw fraction MUST be `min(1, fArrowBowTimerBase + fArrowBowTimerMult × draw time)` with the settings read from the load order's `GameSettings` and the CS wiki's defaults (0.25 and 0.4) where the plugin authors none; the GECK wiki's 0.5 and 0.6 are Fallout's and MUST NOT be used (CS wiki *fArrowBowTimerBase*, *fArrowSpeedMult*; GECK *Complete List of GameSettings*). Full draw is reached at 1.875 s with the defaults. UESP *Oblivion:Marksman* also says "a quick press and release will only drop the arrow immediately in front of you"; the formula gives a 0 s release 386 units per second, and the PRD takes the formula, which the CS wiki says was worked out against OBSE's `GetProjectileSpeed`, the quantitative source, and treats the prose as qualitative (Phase 0 result).
- **BOW-DRW-003.** Skyrim's draw MUST be complete after `1.66 s / (weapon speed × (1 + Quick Shot))` and its nock and rest MUST add about 0.4 s and 0.6 s (UESP *Skyrim:Archery*, observational); until a Quick Shot perk exists the factor is 1. A release before the draw is complete follows Q4.
- **BOW-DRW-004.** The interval between two shots MUST be an engine constant labelled provisional in `bow.status` until a documented value exists: Oblivion ties it to the bow's speed ("the speed property of the bow apparently determines the length of the firing animation", CS wiki) with no number, and Skyrim's rest stage is about 0.6 s. A refused repeat MUST report `cooldown` with the seconds left.
- **BOW-DRW-005.** One system MUST own the draw, the release and the spend, declaring its component and resource access to the scheduler as `skill_use_system` does (CLAUDE.md invariants 3 and 4), and it MUST NOT lock the player's `ActorValues` from the input path.

### 5.3 The rules row (`BOW-MOD`)

- **BOW-MOD-001.** The per-game numbers (the draw rule, the speed rule, the damage terms, the spend moment, the range, the default trajectory) MUST live in a `BowModel` row on the character rules profile, `Some` for Oblivion and Skyrim, and the shot code MUST ask it rather than hold the constants. A game with no row (Fallout) has no bow action.
- **BOW-MOD-002.** The model's settings MUST be read from `GameSettings` with the documented defaults; no number in the shot code is a literal for a game.

### 5.4 Flight (`BOW-FLT`)

- **BOW-FLT-001.** An arrow MUST leave the camera along its aim and fly at the model's speed: Oblivion `arrow speed × fArrowSpeedMult × (fArrowWeakSpeed + (1 − fArrowWeakSpeed) × draw fraction)` (CS wiki; defaults 1500 and 0.01; every one of the 128 vanilla arrows has speed 1.0, census 2026-11-20); Skyrim the arrow's `PROJ` speed at a full draw (the `Skyrim.esm` arrows are 3600 mostly; the CK wiki: "speed: velocity of the projectile").
- **BOW-FLT-002.** Skyrim's arrow MUST leave tilted up by `f1PArrowTiltUpAngle` (2) in first person and `f3PArrowTiltUpAngle` (2.5) in third (UESP *Skyrim:Archery*); Oblivion has no documented tilt and none is applied (Phase 0 result: no tilt setting exists among the CS wiki's `fArrow*` pages and neither UESP *Oblivion:Marksman* nor *Oblivion:Combat* mentions one; closed as none).
- **BOW-FLT-003.** An arrow MUST end on the first solid or actor and at the model's range: Skyrim the lesser of the `PROJ` range and `fVisibleNavmeshMoveDist` (4096, past which arrows "phase through targets without doing any damage", UESP); Oblivion an engine constant labelled provisional in `bow.status`, none being documented (Phase 0 result: the only range settings, `fArrowMaxDistance` 2000 and `fArrowOptimalDistance` 1000, are the combat AI's firing distances (CS wiki: "maximum distance for firing this projectile type", modifiable by a Combat Style Range Mult), not a projectile's range, so neither is used; closed as a declared gap).
- **BOW-FLT-004.** The first release MUST fly every arrow straight. The ballistic mode (§5.7) MUST NOT be offered until a documented gravity rule exists: the CS wiki gives Oblivion's four `fArrowGravity*` settings with no equation, and Skyrim's `PROJ` gravity (0.35) has no documented unit. Today the flight refuses any non-zero gravity (§3 row 4); a straight arrow MUST ignore the `PROJ` gravity field and MUST NOT call that refusal a failure of the shot.
- **BOW-FLT-005.** The arrow MUST ride the weapon projectile path (`ActiveProjectiles`, BOW-G-004); the gravity branch is added to that shared flight when the gap closes, so Skyrim's and Oblivion's arrows and Fallout's projectiles all gain it at once.

### 5.5 Damage (`BOW-DMG`)

- **BOW-DMG-001.** Oblivion's shot damage MUST be `(bow WeaponRating + arrow WeaponRating) × (Fatigue / MaxFatigue + 1) / 2 × max(sneak multiplier, power-attack multiplier) × armor factor × weapon resistance`, the power-attack multiplier being 1 for a shot (BOW-NG-005 leaves power attacks out), with `WeaponRating = BaseWeaponDamage × (0.75 + Agility / 200) × (0.2 + ModifiedSkill × 0.015) × (WeaponHealth / BaseWeaponHealth + 1) / 4` and `ModifiedSkill = Marksman + 0.4 × (Luck − 50)` clamped to 0 to 100 (UESP *Oblivion:The Complete Damage Formula*). UESP gives no condition for an arrow (Phase 0 result: the page defines the bow's rating as "the WR for the Bow + the WR for the Arrow" without stating the arrow's health term, and `AMMO` `DATA` has speed, flags, value, weight and damage only, no health, UESP *Oblivion Mod:Mod File Format/AMMO*); the arrow's `WeaponHealth / BaseWeaponHealth` is taken as 1 and the bow's as 1 (bow wear is BOW-NG-007), a **declared gap** that `bow.status` names. The rating MUST use `oblivion_weapon_damage_multiplier`, which no caller used before the bow (§3 row 5); wiring it into melee is not part of this PRD (Q6). Built in Phase 2: `BowModel::shot_damage` over the shooter's Agility, Marksman, Luck and Fatigue. The armor factor and the weapon resistance are the target's: no Oblivion weapon, melee or ranged, has either at HEAD (`armor::mitigate` serves Skyrim only and no Resist Normal Weapons term exists), so they are a **declared gap** that `bow.status` names, and not a bow-local copy.
- **BOW-DMG-002.** Skyrim's shot damage MUST be the bow's base damage plus the arrow's, times `1 + 0.5 % × Archery` (UESP *Skyrim:Archery*: "each skill point grants a +0.5 % bonus"). The draw-length scaling and the misfire penalty have no documented formula and follow Q4.
- **BOW-DMG-003.** The damage MUST enter the existing damage pipeline as a `HitEvent` through the projectile hit path, so armor, Damage Threshold, ghosts, death, `OnHit` and the story events all follow (`systems/projectiles.rs`); the arrow's own flag "Ignores Normal Weapon Resistance" (Oblivion) MUST reach the resistance term. Phase 2: no resistance term exists at HEAD, so the flag is decoded (BOW-AMM-004) and waits for it (a declared gap, BOW-DMG-001).
- **BOW-DMG-004.** A shot from a sneaking, undetected player MUST take the sneak multiplier from `StealthRules::sneak_attack` (Oblivion bows: 2 at Sneak 0 to 24, 3 from 25, UESP), not a bow-local copy. Phase 2: the player's arrow hit already takes it, because the shared flight applies `sneak_strike` to the shooter's hit and the catalog names the wielded bow's class (`a_bow_hit_takes_the_bow_sneak_multiplier`).

### 5.6 Feeding Marksman (`BOW-USE`)

- **BOW-USE-001.** On Oblivion, a valid hit by the player's arrow MUST queue exactly one skill use for Marksman (actor value 28) at usage 0, the `SKIL` first use value 0.8 ([skill-use §1](../engine/skill-use.md); UESP *Oblivion:Marksman*: 0.8 per successful hit "regardless of damage inflicted"), through `queue_skill_use`.
- **BOW-USE-002.** A miss, a hit on a wall or a corpse, and a hit that deals nothing MUST feed nothing; the arrow's flight MUST NOT feed at the release.
- **BOW-USE-003.** Code 28 MUST join `FED_SKILLS` in the same commit that adds the emitter, and not before (OSL-USE-006), so `levelup` stops listing Marksman as unfed only when a hit feeds it; `weapon_skill` MUST return it for type 5 (§3 row 6).
- **BOW-USE-005.** The feed MUST be tied to the player's arrow, not inferred from the weapon the player wields: a hit from the player's arrow carries its provenance to `on_landed_hit`, and a hit from anything else (a script's projectile, the gun's shot, the player's spell) MUST feed nothing even while a bow is wielded. Today `on_landed_hit` derives the skill from the wielded weapon's type (`wielded_type`, `skill_use_emit.rs:90`) because `HitEvent` carries no projectile provenance (`projectile` names an entity and projectiles are not entities), so adding `weapon_skill(5) = 28` alone would credit Marksman for those hits (found in Phase 0).
- **BOW-USE-004.** Skyrim's Archery experience MUST NOT be fed by this PRD: Skyrim has no weapon-skill emitter, and a lone Archery emitter would be the stand-in OSL-USE-006 forbids (Q3). `bow.status` MUST report Archery `unfed`.

### 5.7 The trajectory setting (`BOW-SET`)

- **BOW-SET-001.** A registry entry `gameplay.arrow_trajectory` MUST let the player choose `straight` or `ballistic`, handled like every other setting (one typed entry, shown by `settings.status`, not saved with the game). Ballistic MUST be listed as unavailable while BOW-FLT-004 stands and the setting MUST resolve to straight with that stated.
- **BOW-SET-002.** The default MUST be per game: ballistic for Oblivion and straight for Skyrim (owner, 2026-11-20), carried as `BowModel.default_trajectory` and resolved when the registry entry is installed, so no shot or setting code branches on the game (CLAUDE.md invariant 1). Both are effective as straight until ballistic exists.

### 5.8 Observability (`BOW-DX`)

- **BOW-DX-001.** `bow.status` MUST report the readied bow, the equipped arrow and its count, the last shot (`released` with its draw fraction and speed, or the refusal reason), the trajectory in effect, the interval and the range, and each number labelled `documented` or `provisional`, and the Marksman feed (`fed` or `unfed`).
- **BOW-DX-002.** `bow.shoot [draw seconds]` MUST run the same gate, spend and flight as the key, with the player as the shooter, so a smoke script drives the real path.
- **BOW-DX-003.** Every refusal MUST log one line naming the bow and the failed gate, the same wording `bow.status` reports.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Arrows fall under gravity (UESP *Oblivion:Marksman*: "aim somewhat higher"; UESP *Skyrim:Archery*: "significant loft") | Every arrow flies straight in the first release (BOW-FLT-004) | none | Long shots do not need aim-up → the setting shows ballistic as unavailable and `bow.status` names the gap (BOW-SET-001, BOW-DX-001) |
| 2 | Skyrim damage scales with the draw length and a misfire is penalised | Q4's recommendation: a release before a full draw fires nothing and keeps the arrow nocked; no partial-draw formula is invented | none | An early release does nothing where the original fires a weak arrow → Q4 and the `provisional` label |
| 3 | A bow loses condition every shot and a held draw costs a Novice Fatigue | Neither is modelled (BOW-NG-007) | none | Bows last forever; no fatigue pressure on archers → Q5, declared in `bow.status` |
| 4 | Arrows stick, bounce and can be recovered (50 %) | The arrow ends at the hit (BOW-NG-004) | none | The arrow economy is harsher than the original's → a later phase |
| 5 | Oblivion's firing interval follows the animation | An engine constant (BOW-DRW-004) | none | Fire rate differs → labelled provisional |
| 6 | Oblivion's arrow range has no documented value | An engine constant (BOW-FLT-003) | none | Very long shots differ → labelled provisional |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | Wave 1. Bows (type 5) and arrows (`AMMO`, speed and damage); the draw timer, speed, damage rating and Marksman feed are documented (UESP, CS wiki). |
| Fallout 3 | no | Out by scope: the engine's gun path already fires, and xEdit's `wbWeaponAnimTypeEnum` (`wbDefinitionsFO3.pas:2817`) lists 0 to 12 (hand to hand, melee, pistols, rifles, handle, launcher, grenade, mines) with no bow. |
| Fallout: New Vegas | no | As Fallout 3: `wbDefinitionsFNV.pas:2844` lists 0 to 13, adding Thrown, with no bow. |
| Skyrim (LE / SE / AE) | yes | Wave 1 (owner, 2026-11-20): the shot, damage and trajectory setting; Archery experience is out (Q3). `Skyrim.esm`: 128 `PROJ`, 20 of type Arrow; 35 `AMMO`. |
| Fallout 4 | no | Out by scope. xEdit's FO4 `wbWeaponAnimTypeEnum` (`wbDefinitionsFO4.pas:7090`) lists 7 Bow and 9 Crossbow as Skyrim's does, so a later wave may reuse this model; this PRD claims no Fallout 4 bow. |
| Fallout 76 | no | Online, out of the engine's single-player scope. |
| Starfield | no | Out by scope; no bow weapon is claimed. |

### 7.2 Acceptance

Data: `Oblivion.esm` and `Skyrim.esm`, read from the directory `crates/plugin/src/esm/test_paths.rs` resolves. No game file is checked in. A scenario passes when:

1. **Unit, the model.** Oblivion's draw fraction and speed equal the CS wiki's formula at 0 s, 0.5 s and 1.875 s (`bow_speed_follows_the_draw_timer`: 386, 683 and 1500 units per second for an arrow of speed 1.0 with the defaults); the rating of `WeapIronBow` (00025231, Oblivion, damage 8) with `Arrow1Iron` (00017829, damage 8) at Marksman 5 and Agility 50 equals the formula's value (`bow_damage_follows_the_rating`); Skyrim's full-draw time of `HuntingBow` (00013985) equals `0.4 + 1.66 / speed + 0.6` (`skyrim_draw_time`).
2. **Unit, the shot.** The gate refuses with `no arrows`, `cooldown` and `cannot act`, spending nothing (`shot_refuses_each_gate`); a release spends one arrow of the equipped stack and clears the slot when it empties (`a_shot_spends_one_arrow`); the equipped ammo round-trips a save and a save naming an absent arrow clears it (`equipped_ammo_round_trips`); a bow in hand does not swing (`a_bow_does_not_melee`).
3. **Unit, the feed.** A valid hit queues one Marksman use of 0.8, and a miss, a wall, a corpse, a zero-damage hit and the release itself queue nothing (`bow_feeds_only_a_valid_hit`); `levelup` lists Marksman fed (`marksman_leaves_the_unfed_list`); a script's projectile and the gun's shot feed nothing.
4. **Live, new script `oblivion-archery.sh`** (indexed in [docs/smoke-tests/README.md](../smoke-tests/README.md)): Oblivion in `AnvilTheCountsArms`, the target the first patron `stealth.status` lists, `WeapIronBow` and `Arrow1Iron` added and equipped: `bow.shoot 2` at the patron lowers their Health, spends one arrow, and `levelup` shows Marksman's banked experience rise by 0.8 (or its rank); a ceiling shot misses and feeds nothing; `bow.status` names the trajectory straight. A Skyrim leg, `skyrim-archery.sh` in the Bannered Mare with `HuntingBow` and `IronArrow` (0001397D), checks the damage, the arrow spent and `Archery unfed`.
5. **Existing gates stay green.** `oblivion-level-up.sh` and `oblivion-spellcasting.sh` still pass and `combat.status` counts a shot as no melee attack.

---

## 8. Metrics

The numbers live in the [ROADMAP compatibility matrix](../../ROADMAP.md#compatibility-matrix), Oblivion row, beside the skill-use PRD's. Marksman's feed moves that PRD's own rows (*Fed skills*, *UESP actions fed*, [OSL §8](oblivion-skill-use-leveling.md)); this PRD adds none.

| Metric | Definition | Granularity |
|---|---|---|
| Bows shooting | games whose bow shoots and damages ÷ 2 | Oblivion, Skyrim |
| Refusal coverage | shot gates with a named refusal test ÷ gates | per game |

---

## 9. Delivery sequence

Phases are ordered by what they unblock; each ends with a measurement. The feed (Phase 3) depends on the skill-use drain and `FED_SKILLS` of [Oblivion skill-use leveling](oblivion-skill-use-leveling.md) (#7047).

**Phase 0 — Sourcing (no engine code). Closed 2026-11-20** (rev 4): the verify items are closed in their requirements (BOW-AMM-003, BOW-FLT-002, BOW-FLT-003, BOW-DMG-001), the projectile path is written in [Game loop](../engine/game-loop.md), the equipped-ammo slot in [Save and load round trip](../engine/save-load-roundtrip.md), and the settings registration contract in [Launcher](../engine/launcher.md) §4.4; Phase 0 found BOW-USE-005. The research is `.claude/issues/7055/INVESTIGATION.md`. Close the **Phase 0: verify** items that documents can settle: the arrow-spend trigger per game and the disagreement between the two Oblivion pages (BOW-AMM-003), the arrow's condition term (BOW-DMG-001), whether Oblivion tilts the arrow (BOW-FLT-002) and an Oblivion arrow range (BOW-FLT-003); write the settings registry's design section (BOW-SET) and the projectile and equipped-ammo sections (Related); resolve §10. Exit: each item closed with a source or filed as a declared gap.

**Phase 1 — Ammo, draw, release and a straight arrow.** BOW-AMM, BOW-DRW, BOW-MOD, BOW-FLT and BOW-DX. A prerequisite in the same phase: Skyrim's weapon classification (§3 row 7), because a Skyrim bow is not a melee weapon. The equipped ammo is a saved component with its format bump. Exit: scenario 2 passes and a bow shot spends an arrow and flies on both games.

**Phase 2 — Damage and the hit.** BOW-DMG. Exit: scenario 1 passes and an arrow lowers a living actor's Health on both games.

**Phase 3 — Feed Marksman and the setting.** BOW-USE, BOW-SET. Exit: scenarios 3, 4 and 5 pass; code 28 is in `FED_SKILLS`; the ROADMAP matrix row shows the metrics of §8.

---

## 10. Decisions required

1. **Q1 — Which games.** Recommended: Oblivion only (Wave 1), because Skyrim's feed and partial draw have the most gaps. Alternative: both together.
   **Decided** (2026-11-20): both together; the shot, damage and the setting are shared, Skyrim's experience waits on Q3.
2. **Q2 — The trajectory.** Recommended: a setting between straight and ballistic with ballistic a declared gap until a documented rule exists. Alternative: a provisional parabola labelled provisional.
   **Decided** (2026-11-20): the setting (`gameplay.arrow_trajectory`), straight first, ballistic a declared gap, default per game (ballistic for Oblivion, straight for Skyrim, both straight in effect for now).
3. **Q3 — Skyrim's Archery experience.** Recommended: unfed until Skyrim has a weapon-skill emitter family (One-Handed, Two-Handed and the rest), because UESP's own equation ("base weapon damage of the bow × use mult 9.3", marked "additional verification needed") is provisional and a lone emitter is the stand-in OSL-USE-006 forbids. Alternative: feed Archery with that formula labelled provisional, which gives Skyrim archers progress now and a number the documents do not confirm.
   **Decided** (2026-11-20): the recommended answer; Skyrim's Archery stays unfed until Skyrim has a weapon-skill emitter family.
4. **Q4 — Skyrim's release before a full draw.** Recommended: a release before the draw is complete fires nothing and keeps the arrow nocked until it is (a declared deviation), because the documents give no damage or speed rule for a partial draw. Alternative: fire at once with damage and speed scaled linearly by the draw fraction, labelled provisional, which is closer to play but invents the scaling.
   **Decided** (2026-11-20): the recommended answer; a release before the draw is complete fires nothing and keeps the arrow nocked (the §6 row 2 deviation).
5. **Q5 — Bow wear and the Fatigue of a held draw (Oblivion).** Recommended: not modelled and declared (BOW-NG-007), because both exist in the original (UESP *Oblivion:Marksman*) with no documented rate. Alternative: provisional constants, which would make the bow wear and tire the Novice but with invented numbers.
   **Decided** (2026-11-20): the recommended answer; bow wear and the held draw's Fatigue are not modelled and are declared (BOW-NG-007).
6. **Q6 — Wiring Oblivion's damage rating.** Recommended: for bows only; melee keeps its base damage until its own issue, because wiring it into the Blade and Blunt attack changes Oblivion's melee balance, a promise this PRD does not make. Alternative: wire the rating for every Oblivion weapon in Phase 2, which fixes the inherited gap and moves melee damage in the same release.
   **Decided** (2026-11-20): the recommended answer; Oblivion's damage rating is wired for bows only, melee keeps its base damage.

---

## 11. Document map

| Need | Go to |
|---|---|
| The Phase 0 research and sources | `.claude/issues/7055/INVESTIGATION.md` |
| The skill-use drain, the fed list and the emitter contract | [skill-use](../engine/skill-use.md) |
| The Oblivion damage rating and the rules profile | [Oblivion ruleset](../engine/charal-oblivion-ruleset.md) |
| The pattern this PRD follows for a rules row, input, report and smoke gate | [player-spellcasting](player-spellcasting.md) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-20 | First draft, from the Phase 0 research; Q1 and Q2 recorded from the owner's answers | — |
| 2 | 2026-11-20 | Review round 1 folded in: the spend-moment quote cited to *Oblivion:Combat* with the Marksman disagreement as a verify item, FO3 / FNV / FO4 weapon types settled from xEdit, `BowModel.default_trajectory` for the per-game trajectory default, owning designs for the equipped-ammo save and the projectile path, the Skyrim anim-type source, the arrow's condition term, the §8 metrics left to the skill-use PRD, Q4 named as the §6 row 2 deviation | claude: revise |
| 3 | 2026-11-20 | Review round 2: strict gate clean; nine "Today" claims re-measured at HEAD and true (§3 rows 1, 2, 3, 4, 5, 6, 7, 8); owner approves the PRD and takes the recommended answers to Q3 to Q6 (selected in the `/fix-issue 7055` session) | claude: ready for the owner's approval; owner: approved |
| 4 | 2026-11-20 | Phase 0 closed: BOW-AMM-003, BOW-FLT-002, BOW-FLT-003 and BOW-DMG-001 verify items closed from UESP and the CS wiki (the spend trigger, Oblivion's arrow tilt and range, and the arrow's condition are declared gaps); Oblivion's early-release prose versus the speed formula recorded in BOW-DRW-002; BOW-USE-005 added (the feed needs the arrow's provenance); the design sections written | — |
| 5 | 2026-11-20 | Phase 1 built (`bow.rs`, `BowModel`, `EquippedAmmo` and format v141, `bow.status` / `bow.shoot`, `gameplay.arrow_trajectory`): §3 rows 1, 2, 3 and 7 updated; Skyrim `WEAP` `DNAM` speed decoded (documented: xEdit, UESP), which also scales Skyrim melee cooldowns; no requirement changed | — |
| 6 | 2026-11-20 | Phase 2 built (`DamageRule`, `BowModel::shot_damage`, the shooter's actor values read by name): BOW-DMG-001 and -002 as written; BOW-DMG-003's resistance term and BOW-DMG-001's armor factor recorded as declared gaps (no Oblivion weapon has either at HEAD); BOW-DMG-004 satisfied by the shared hit path; no requirement removed | — |
| 7 | 2026-11-20 | Phase 3 built: `HitEvent::arrow` / `TrapHit::arrow` / `Projectile::arrow` carry the provenance (BOW-USE-005), Marksman joins `FED_SKILLS` with its emitter (BOW-USE-001 to -003), Skyrim's Archery stays unfed (BOW-USE-004); an arrow that dealt nothing (a ghost, a blocked shot) is no hit; `oblivion-archery.sh` / `skyrim-archery.sh` added; ROADMAP row and skill-use metrics updated (15 of 21 skills, 16 of 26 actions); no requirement changed | — |
| 8 | 2026-11-20 | Phase 3 verified live: `oblivion-archery.sh` (arrow hit a patron, Health 16 to 14.02, Marksman 0.8, ceiling shot fed nothing, `attacks` unchanged) and `skyrim-archery.sh` (Health 75 to 60.02, `Archery unfed`) pass, as do `oblivion-spellcasting.sh` and `oblivion-level-up.sh` at 15 of 21; the gates count against the player's starting arrows (Skyrim's carries 23) and read `hits` as rising on a landed arrow; no requirement changed | — |
