# Player Spellcasting — Product Requirements

**Status**: ACTIVE (2026-11-22). Revision 16 records the two tests review round 2 left before SHIPPED; revision 15 the owner's cycle keys; the history is §12.

**Tracking**: #7077 (parent). Phases: #7078 (0), #7079 (1), #7080 (2), #7081 (3). Phase 0 residuals: #7082. Follow-up it closes: #7053.

**Scope**: The player casts a spell they know: choosing which spell is ready, the cast input, the checks that gate the cast, the Magicka and Fatigue it spends, the target a Self, Touch or Target spell resolves, and the one consumer that motivates it, the skill use a cast with a valid target gives its school. Wave 1 is Oblivion (the only game whose magic skills the engine does not yet feed). Skyrim's per-hand casting (Q1), Fallout and Starfield are outside it (§7.1). Cast animation, cast and impact visuals, scrolls, powers, spell making and the magic menu are explicitly not here (§4.2).

**Related**: this document states the *promise*; the designs below own the *how*. Every mechanism promised here has an owner in this table, or Phase 0 writes the missing design.

| Mechanism | Owning design |
|---|---|
| Skill-use emitter contract and the fed-skill list | [Skill use](../engine/skill-use.md#4-the-emitter-contract-osl-xp-007) |
| Oblivion's ruleset, where the cast rules (cost equation, authored mastery, the Fatigue point) belong (PSC-CST-008) | [Oblivion ruleset, "Spell casting"](../engine/charal-oblivion-ruleset.md#spell-casting-7078-provenance-relabelled-2026-11-20) |
| Sneak stance and the detection tick, should a cast prove audible (PSC-NG-007) | [Sneak and detection](../engine/sneak-detection.md) |
| The magic menu that will replace the console's ready command | [Game UI integration](game-ui-integration.md) |
| Spell catalog, effect application, the cast runtime | [Scripting](../engine/scripting.md), the **Player cast** paragraph |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119 meanings, as in the related designs. Requirement IDs use the `PSC-` prefix.

---

## 1. Product thesis

Oblivion's magic skills level by casting. The engine already runs a spell once something calls `cast_spell`, but only the console, an ObScript `Cast` and a perk's activate choice do (§3 row 2), so a player who plays a mage cannot cast and their six school skills never move. The skill-use PRD refuses to feed those skills from a script's forced cast, because that would be a stand-in for the real action ([Oblivion skill-use leveling](oblivion-skill-use-leveling.md), OSL-USE-006). This PRD builds the real action.

The action is the original's, not an invention: the player knows spells (a list), readies one, and casts it; the cast is gated by the spell's school skill and the player's Magicka, costs both Magicka and one Fatigue (UESP *Oblivion:Magic Overview*: "Casting any spell costs one point of Fatigue"), and cannot fail (the same page, "Comparison to Morrowind"). What the engine adds is only the plumbing between a key press and the cast runtime that already exists.

The corollary that shapes every requirement: **a cast is the player's, or it is not a cast.** The one path that gates, charges and feeds is the player's; a script's `Cast`, the console's `magic.cast` and a perk's activate-choice cast stay what they are, a spell started without a caster's cost and without skill use. The engine will not feed a school from any other path.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Readied spell** | The one spell the player has chosen to cast next; it persists until another is chosen or it leaves the spell list. |
| **Cast action** | The input edge that casts the readied spell. |
| **Range** | Where an effect lands, per effect: Self, Touch or Target. Oblivion's `EFIT` carries it as `Type` (0, 1, 2; xEdit `wbEffectTypeEnum`), and the repository's decode calls it the range (`ranges`, `oblivion_effect_chain`). |
| **Effect cost** | One effect's magicka cost before the caster's skill adjusts it: `0.1 × MGEF base cost × D × A × M × T`, a float, where `D` is the duration or 1 when it is 0, `A = max(1, area × fMagicAreaBaseCostMult)`, `M = magnitude^fMagicCostScale` or 1 when the magnitude is 0, and `T = fMagicRangeTargetCostMult` for a Target effect, else 1. The MGEF flags no-magnitude, no-duration and no-area zero `M`, `D` and `A`, and a Self effect has no area (UESP *Spell Making*, "Spell Cost"; CS wiki *fMagicDurMagBaseCostMult*, *fMagicCostScale*, *fMagicAreaBaseCostMult*, *fMagicRangeTargetCostMult*; confirmed in the executable, #7078). |
| **Base cost** | A spell's magicka cost before skill: the sum over its effects of the effect cost floored to a whole number of at least 1, or the `SPIT` `Cost` when the `Manual Spell Cost` flag is set (CS wiki *Spell*, "Auto-Calculate"; xEdit TES4 `SPIT` flag bit 0). The stored `Cost` of an auto-calculated spell is stale and is not used (§9.1). |
| **Actual cost** | The Magicka a cast spends. For an auto-calculated spell, the sum over its effects of the effect cost times the modifier of that effect's own school, floored, at least 1 each; for a Manual Spell Cost spell, the stored `Cost` times the modifier of its primary school, not rounded; for a Power or Lesser Power, no modifier. The modifier is `fMagicCasterSkillCostBase + fMagicCasterSkillCostMult × (1 − S ÷ 100)` (`Oblivion.esm`: 0.2 and 1.2, so `1.4 − 0.012 × S`), with `S = clamp(Skill + fActorLuckSkillMult × Luck + iActorLuckSkillBase, 0, 100)` (executable defaults 0.4 and −20, so Luck adds `0.4 × (Luck − 50)`) (UESP *Magic Overview*; CS wiki *fMagicCasterSkillCostBase*; #7078). |
| **Mastery** | The tier a spell requires, Novice to Master (0 to 4). An auto-calculated spell's tier is that of its base cost against `fMagicSpellLevelApprenticeMin`, `JourneymanMin`, `ExpertMin` and `MasterMin` (`Oblivion.esm`: 25.5, 62.5, 149.5, 399.5); a Manual Spell Cost spell's is its stored `SPIT` `Level`. The player's tier is that of their unfortified skill against `iSkillApprenticeMin`, `JourneymanMin`, `ExpertMin` and `MasterMin` (25, 50, 75, 100). A cast needs the player's tier to reach the spell's (CS wiki *Spell*, *fMagicSpellLevelXXMin*; UESP *Magic Overview*, "Skill levels and training"; #7078). |
| **Castable spell** | A spell of type Spell in the player's `SpellList`. Powers, Lesser Powers, diseases, abilities and poisons are not castable here (PSC-NG-002). |
| **School** | Alteration, Conjuration, Destruction, Illusion, Mysticism or Restoration: MGEF `magic_school` 0..5 (xEdit `wbMagicSchoolEnum`; 6 is None), actor value `20 + school`. A script effect carries its own school in `SCIT` (xEdit TES4 `SCIT`, "Magic school"). |
| **Primary school** | A multi-effect spell's school: that of its single effect with the highest base cost, the first of equal ones (CS wiki *Spell*: "The school is always the same as that of the single most expensive effect"; #7078). |
| **Valid target** | A target the cast affected: Self always; for Touch and Target, a target an effect of the spell landed on (UESP: casting a fireball on a chest gives no experience, nor does an open spell on an NPC). |
| **Cast intent** | The result of the player's cast action, before any effect lands: refused (with a reason) or started. |

---

## 3. Core model

A cast has four stages, in order: **ready** (a spell is selected), **gate** (the player may cast it now), **resolve** (each effect finds its target by range), **settle** (cost paid, effects started, skill use queued). A refusal at the gate pays nothing and queues nothing, and says why.

The inventory below is the state at HEAD on 2026-11-22, after Phases 0 to 3 (§9.1 to §9.4). The state before the work, on 2026-11-19, is revision 13's §3.

| # | Piece | Class | At HEAD |
|---|---|---|---|
| 1 | Starting a spell's effects on a target | exists, shared | `cast_spell` and `start_spell_effects` (`crates/scripting/src/magic_effects.rs`) start the effects that land, filtered by range; `raise_cast_event` raises the Story Manager `CAST` event once per cast. They spend nothing and feed nothing |
| 2 | Uncharged callers | script, console, perk | the console's `magic.cast` (`byroredux/src/commands/gameplay.rs`), ObScript `Cast` (`crates/scripting/src/obscript_effects/presentation.rs`) and a perk's activate-choice *Apply Spell* (`crates/scripting/src/perk_entry_points/activate.rs`) call `cast_spell` unchanged (PSC-CST-007) |
| 3 | The spell list | exists | `SpellList` (`crates/scripting/src/magic.rs`) is the actor's known spells; the readied spell and the cycle actions choose from it |
| 4 | Ready state | built (Phase 1) | `ReadiedSpell` (`crates/scripting/src/player_cast.rs`), a saved player component (`FORMAT_MAJOR` 140) |
| 5 | Input | built (Phase 1) | `InputAction::CastSpell`, `NextSpell` and `PreviousSpell` in the engine's input map (`byroredux/src/interaction.rs`), bound to `C`, Right arrow and Left arrow; the extension-facing `InputAction` (`crates/sdk/src/event.rs`) does not carry them (§9.2) |
| 6 | Magicka and Fatigue cost | built (Phase 1) | `player_cast_system` (`byroredux/src/player_cast.rs`) applies both through `ActorValues::apply_damage`, with the rules of `SpellCastModel` (`crates/core/src/character/spell_cast.rs`) |
| 7 | Skill and cost data per spell | built (Phase 1) | `CastEffect` carries the `EFIT` range and area; `CanonicalSpell.cast` (`SpellCastData`) carries the header and each effect's cost inputs and school (MGEF or `SCIT`) |
| 8 | The projectile a spell rides | built (Phase 2) | `ActiveProjectiles` (`byroredux/src/systems/projectiles.rs`) flies a `Projectile` with `spell: Some(..)`; on a live actor it calls `land_target`, otherwise `missed_target` (`crates/scripting/src/player_cast.rs`) |
| 9 | Feeding a skill | built (Phase 3) | `queue_skill_use` (`crates/scripting/src/oblivion_skill_use.rs`); `FED_SKILLS` holds the six schools (20 to 25), and `levelup` no longer lists them unfed |
| 10 | The authored spell header | built (Phase 1) | `decode_spit` (`crates/plugin/src/esm/records/misc/magic.rs`) keeps the `SPIT` `Level` and the `Manual Spell Cost` flag; a script effect's `SCIT` school is decoded |

---

## 4. Goals and non-goals

### 4.1 Goals

- **PSC-G-001 — A mage can cast.** On Oblivion, the player readies a known spell and casts it with an input, and the spell's effects land by range.
- **PSC-G-002 — The six schools level by casting.** A cast with a valid target feeds the spell's primary school by the `SKIL` use value, and nothing else feeds a school.
- **PSC-G-003 — The cost is the original's.** The Magicka a cast spends follows the effect-cost equation and the skill adjustment, and a cast the player cannot afford or is too unskilled for is refused with the reason.
- **PSC-G-004 — One path.** The player's cast, a script's `Cast`, the console's `magic.cast` and a perk's activate choice reach the same effect-start code; the player's path alone gates, charges and feeds.

### 4.2 Non-goals

- **PSC-NG-001.** Cast animation, casting sound and the cast, travel and impact visuals. The engine has no spell animation or effect-visual system; the cast action's feedback is the log and the console report until one exists.
- **PSC-NG-002.** Scrolls, Powers and Lesser Powers. A scroll costs no Magicka and a power gives no skill use (UESP *Magic Overview*), and a Lesser Power is not scaled by skill or mastery (CS wiki *Spell*), so none belongs to the feed this PRD exists for; they get a PRD when their menus and item path do.
- **PSC-NG-003.** Spell making and the spell-making altar, and enchanting. They create spells; this PRD casts them.
- **PSC-NG-004.** The magic menu and its hotkeys. They are the Game UI PRD's; the readied spell is chosen through a command and a cycle action until the menu exists.
- **PSC-NG-005.** Non-player casters. The AI casting spells at the player is the AI packages PRD's; the shared `cast_spell` path (PSC-G-004) is all this PRD asks of it.
- **PSC-NG-006.** Spell effectiveness reduced by worn armor (Q3, decided: out of Wave 1). It is real (UESP *Magic Overview*, "Using Armor": wearing armor lowers the magnitude, rounded down; CS wiki *fMagicArmorPenaltyMin* 0.05 and *fMagicArmorPenaltyMax* 0.3 bound it). The [armor casting penalty notes](../engine/charal-oblivion-ruleset.md#armor-casting-penalty-sourced-2026-11-20-not-built) record the documented interpolation and remaining ambiguities for a later wave.
- **PSC-NG-007.** The engine omits a detection noise event for stationary casting. The [original-game trial](../engine/charal-oblivion-ruleset.md#casting-noise-trial-7082) found no score change on Self, empty Touch or scenery-bound Target casts with a guard out of sight. The trial varied `fSpellCastingDetectionMod` and used movement as a positive control. Sounds from a spell striking another actor lie outside that trial.
- **PSC-NG-008.** The refusals the original also has for a silenced caster, a ranged cast underwater and a spell with several bound effects (#7078 decode, `0x5f44a0`). They depend on states (a Silence effect, water depth, bound-weapon effects) this PRD does not add; the cast gate (PSC-CST-002) is built so each can join it.

---

## 5. Requirements by area

### 5.1 Ready and choose (`PSC-RDY`)

- **PSC-RDY-001.** The player MUST have a readied spell, a form id of a spell in their `SpellList` or none. A readied spell that leaves the list (removed, or a load that no longer has it) MUST clear.
- **PSC-RDY-002.** `spell.ready <spell form id>` MUST ready a spell the player knows and report its name, its primary school and its actual cost; it MUST refuse a spell not in the list and say so.
- **PSC-RDY-003.** An `InputAction` pair (`NextSpell`, `PreviousSpell`) SHOULD cycle the readied spell through the player's castable spells in list order and report the new one.
- **PSC-RDY-004.** The readied spell MUST survive a save and a load, and a loaded save that names a spell no longer present MUST clear it without an error (PSC-RDY-001). The readied spell is saved, which needs a save-format bump (Q5, decided): the component and its format bump land together in Phase 1, with the save round-trip test, and README T2's save-format trigger is met by this PRD.

### 5.2 The cast action (`PSC-CST`)

- **PSC-CST-001.** An `InputAction::CastSpell` MUST exist, with the default binding `C` (`KeyCode::KeyC`, named by the owner on 2026-11-19 for Oblivion, the original's PC default per UESP *Oblivion:Controls*; Q2, decided). `C` was the keyboard Block binding, so Phase 1 moved it to `X` (the right mouse button stays Block); `ActionBindings` is one default for every game, so the move applies to all of them while the spell actions act only where the profile carries cast rules (§9.2), and its press edge MUST be the only input that casts.
- **PSC-CST-002.** A cast MUST pass the gate in this order and refuse at the first failure, naming it: the player can act and is in character mode (`player_can_act`, the check `combat_input_system` already uses); a spell is readied and is still in the list; the spell is castable (§2); the player's tier reaches the spell's mastery (PSC-CST-003); the player's current Magicka is at least the actual cost (PSC-CST-004), a cost equal to Magicka being allowed. A refusal MUST spend nothing and queue no skill use. The original refuses on Magicka before mastery in its message priority (code 1 then 3, `0x5f44a0`); the gate's order is the engine's and reports every refusal by name.
- **PSC-CST-003.** The mastery a spell requires MUST be derived as §2 defines it: from the spell's base cost against the four `fMagicSpellLevel*Min` settings for an auto-calculated spell, and from the stored `SPIT` `Level` for a Manual Spell Cost spell (UESP's per-level tables match the cost thresholds for 321 of 330 matched spells and the authored `Level` for 286, §9.1; the executable derives the tier from the cost, `0x41d3c0`). The decode MUST carry the base-cost inputs and the `SPIT` `Level` and flags into the spell catalog (§3 row 10). The player's tier MUST come from the unfortified skill, so a Fortify effect does not raise it.
- **PSC-CST-004.** The actual cost MUST be the one §2 defines. The constants MUST be read from `GameSettings` (`crates/scripting/src/obscript_state.rs`), with the executable's defaults where the plugin authors none (`fMagicCasterSkillCostBase` 0.1 in the executable and 0.2 in `Oblivion.esm`; `fMagicCostScale` 1.25 and 1.28; `fActorLuckSkillMult` 0.4, `iActorLuckSkillBase` −20 only in the executable), not hard-coded. The Luck term applies to the cast itself and not only to the displayed cost (`0x5f4570` and `0x5fc8fa` call the same cost function).
- **PSC-CST-005.** A cast that passes the gate MUST apply the actual cost as damage to the player's Magicka and `fFatigueCastBase + fFatigueCastMult × the Magicka cost` as damage to their Fatigue (executable defaults 15 and 0, `Oblivion.esm` base 1.0, so 1 Fatigue; UESP *Magic Overview*: "Casting any spell costs one point of Fatigue"), through `ActorValues::apply_damage` (`crates/core/src/ecs/components/actor_values.rs`), the layer the HUD's current value reads, then call the shared effect start (PSC-CST-007). A spell cannot fail once the gate passes (UESP *Magic Overview*, "Comparison to Morrowind").
- **PSC-CST-006.** A cast MUST NOT repeat before the casting interval has elapsed. The stock cast animations supply release and end markers by range (§9.1). The [original-game trial](../engine/charal-oblivion-ruleset.md#live-cast-repeat-timing-7082) measured 2.02–2.12 s for Self, 1.42–1.43 s for Touch, and 1.33–1.37 s for Target while casting unarmed and unblocked. Until the engine plays a cast animation the interval MUST be the engine constant `MELEE_COOLDOWN_SECONDS` (0.45 s, `byroredux/src/combat.rs`; `CAST_INTERVAL_SECONDS` in `byroredux/src/player_cast.rs`), `cast.status` MUST label it provisional, and a refused repeat MUST report `cooldown` with the seconds left (#7082).
- **PSC-CST-007.** The player's cast MUST call `cast_spell` with the player as the caster and the resolved target (PSC-TGT-001..003), and MUST NOT duplicate its effect-start or `CAST` event code. The console's `magic.cast`, ObScript `Cast` and a perk's activate-choice cast MUST keep calling it unchanged and MUST neither charge nor feed (PSC-G-004); they are §3 row 2.
- **PSC-CST-008.** The Oblivion cast rules (§2's effect cost, base cost, actual cost, mastery tiers and the Fatigue cost) MUST live in the character ruleset (`crates/core/src/character`, beside `skill_use.rs`) as functions of the rules profile, and the cast code MUST ask them rather than hold the constants (CLAUDE.md invariant 1: no per-game branch past the translation boundary). They are not a derived stat; the ruleset design's earlier "not a CHARAL derived stat" routing is corrected where Phase 0 writes the section (#7078).
- **PSC-CST-009.** One system MUST own the spend: it reads the readied spell and the cast edge, writes the Magicka and Fatigue damage and queues the skill use, and it MUST declare its component and resource access to the scheduler in the way `skill_use_system` does (CLAUDE.md invariants 3 and 4; [skill-use §4](../engine/skill-use.md#4-the-emitter-contract-osl-xp-007)). It MUST NOT lock the player's `ActorValues` from the input path.

### 5.3 Where a cast lands (`PSC-TGT`)

- **PSC-TGT-001.** An effect of range Self MUST land on the player. The range is decoded for Oblivion (`oblivion_effect_chain`, `crates/plugin/src/esm/records/misc/magic.rs`), and the runtime's `CastEffect` MUST carry it (§3 row 7).
- **PSC-TGT-002.** An effect of range Touch MUST land on a live actor if it owns the first solid collider hit by the camera ray. The ray length MUST come from the load order's `iActivatePickLength` with the stock PC default of 150 BU. The [original-game trial](../engine/charal-oblivion-ruleset.md#touch-distance-trial-7082) verified the setting by varying it. The stock ray hit a stationary scale-one guard at 183 BU of reference distance and missed at 184 BU (#7082). A Touch cast that finds no actor in reach applies nothing; with the `Touch Spell Explodes w/ no Target` flag (xEdit TES4 `SPIT` flags bit 7) and an effect of area above 0 the original explodes it at the caster's aim position, which is area casting (PSC-TGT-005). What a Touch spell does to a door, a chest or a corpse is settled in part by UESP *Magic Overview* ("Ranged and touch spells need to strike a target that can be affected by the spell effect. Casting a fireball on a chest won't give you experience; casting an open spell on an NPC won't, either"): an effect lands on a target that can be affected by it. Open acts on a locked door or container (UESP *Open*, *Spell Effects*: "always targeted, with several exceptions") and Reanimate on the corpse of an NPC (UESP *Reanimate*); the engine has neither mechanic, so it lands on live actors only and feeds nothing otherwise, which agrees with every documented negative case.
- **PSC-TGT-003.** An effect of range Target MUST fly from the player along the camera's forward and land on the first live actor it meets, or on nothing at the end of its range, reusing the flying-projectile path the player's gun already uses (`ActiveProjectiles`, PSC-G-004's single path). The speed MUST be `fMagicProjectileBaseSpeed` (1000 in the CS wiki) times the effect's `MGEF` projectile speed, which is "a multiplier of the base speed for the projectile" (CS wiki *Magic Effects*, *fMagicProjectileBaseSpeed*), and the range `fMagicProjectileMaxDistance` (10000, CS wiki), both read from `GameSettings` through the cast model; the `MGEF` multiplier is 1 on Flare. The `MGEF` forms Bolt, Fog and Spray have their own distance settings (`fMagicBoltMaximumDistance`, `fMagicFogMaximumDistance`, `fMagicSprayMaximumDistance`) and the form is authored in the effect's `MGEF.DATA` flag bits 25 and 26 (§9.1). ByroRedux uses ball flight for all Target effects in this wave (#7082). Gravity, the projectile model and the impact explosion are the projectile system's declared gaps and stay so (`systems/projectiles.rs` module docs).
- **PSC-TGT-004.** A multi-range spell MUST resolve each effect by its own range (UESP *Spell Making*: effects of one spell may differ in range), and a spell none of whose effects found a target MUST still pay its cost (PSC-CST-005) and report `no target`.
- **PSC-TGT-005.** An effect with an area greater than 0 SHOULD be reported as `area ignored` until area casting exists; it MUST NOT silently land on one actor as though the area were 0. The original's rule, for the later requirement: the radius is `fMagicUnitsPerFoot` (22) × the caster's scale × the area in feet, it applies to every actor within it but the directly-hit target, and it needs line of sight unless the spell has `Area Effect Ignores LOS` (CS wiki *fMagicUnitsPerFoot*, *Spell*; the executable, `0x6992f0`).

### 5.4 Feeding the school (`PSC-USE`)

- **PSC-USE-001.** A cast that started at least one effect on a valid target MUST queue exactly one skill use for the spell's primary school, through `queue_skill_use`, at usage 1 (the `SKIL` second use value, [skill-use §1](../engine/skill-use.md): the executable's three spell sites pass skill = the primary school's actor value 20 to 25, usage 1 and no multiplier, #7078) and the same multiplier convention the other emitters use.
- **PSC-USE-002.** A spell with no Target-range effect MUST award once, when at least one effect applied: Self always applies, and a Touch cast that found no actor in reach applies nothing and awards nothing. A spell with a Target-range effect MUST award once, at impact, when a Target-range effect lands on a target that accepts it (Telekinesis excepted); a missed projectile awards nothing, and the release of such a spell awards nothing. A cast that landed on nothing, or on a target no effect could affect, MUST feed nothing (UESP *Magic Overview*, "Skill levels and training"; the executable, `0x69af30`, `0x69ba90`). In the original, a direct hit with Fire Ball awarded one use in both the single-target trial and the trial with two additional splash victims ([original-game results](../engine/charal-oblivion-ruleset.md#area-experience-trial-7082), #7082). Wave 1 ignores area (PSC-TGT-005).
- **PSC-USE-003.** The primary school MUST be the school of the effect with the highest base cost (§2: caster-less, floored), the first of equal ones, and the cast feeds only it (CS wiki *Spell*). An effect's school is its `SCIT` school for a script effect and otherwise its MGEF `magic_school`, mapped 0 to 5 to the actor values 20 to 25 (executable `0x415360`, `0x41b770`); a spell none of whose effects carries a school (MGEF school 6, None, and no `SCIT`) MUST feed nothing and report `no school`.
- **PSC-USE-004.** The six codes (20 to 25) MUST join `FED_SKILLS` in the same commit that adds the emitter, and not before (the OSL-USE-006 rule), so `levelup` stops listing them unfed only when a cast feeds them.
- **PSC-USE-005.** A Power, a Lesser Power, a disease, an ability, a scroll, ObScript `Cast`, the console's `magic.cast` and a perk's activate-choice cast (`activate.rs:195`, which can run with the player as caster) MUST feed nothing (UESP *Magic Overview*: casting a power "does not contribute any experience points"; scrolls give none; the executable awards only for item types Spell and Poison, `0x6994aa`).

### 5.5 Observability (`PSC-DX`)

- **PSC-DX-001.** `cast.status` MUST report the readied spell, the last cast intent (`started` or the refusal reason), its actual cost, whether it fed a school and which, the cast interval and the touch reach in force (PSC-CST-006, PSC-TGT-002), with the interval labelled provisional until the engine plays the casting animation.
- **PSC-DX-002.** `spell.cast` MUST run the same gate and settle as the key (PSC-CST-001..007) with the player as caster, so a smoke script drives the real path; `magic.cast` MUST stay the uncharged, unfed script path.
- **PSC-DX-003.** Every refusal MUST log one line naming the spell and the failed gate, the same wording `cast.status` reports.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | The cast plays an animation whose timing sets the casting speed (UESP *Magic Overview*: blocking while casting shortens it) | A fixed interval, no animation (PSC-CST-006, PSC-NG-001) | none | A player cannot cast faster by blocking → PSC-CST-006 names the interval and `cast.status` reports it |
| 2 | A Target spell is a projectile with its own travel and impact | The weapon projectile path, a straight line, with its declared gaps (PSC-TGT-003) | none | Arcing and exploding spells miss their original behaviour → the gaps are stated in the projectile system and PSC-TGT-005 reports an ignored area |
| 3 | Worn armor lowers spell effectiveness (UESP *Magic Overview*) | Not applied in Wave 1 (PSC-NG-006, Q3) | none | Armored mages are stronger than the original's → the formula is sourced ([armor casting penalty](../engine/charal-oblivion-ruleset.md#armor-casting-penalty-sourced-2026-11-20-not-built)) and Q3 keeps it out of Wave 1; a later wave adds the requirement |
| 4 | The magic menu readies a spell | A command and a cycle action (PSC-RDY-002, -003) | none | Not discoverable in play → the menu is the Game UI PRD's, PSC-NG-004 |
| 5 | The tested stationary casts produced no detection score change (§9.1) | Casting emits no detection noise (PSC-NG-007) | none | The test covers casts without an actor impact |
| 6 | Cast repeat intervals vary by spell range in the original-game trial (§9.1). UESP *Magic Overview* says "Casting speed is fixed for every kind of spell"; the measured per-range intervals take precedence for the later requirement | A fixed 0.45 s interval (PSC-CST-006) | none | Spells repeat faster than in the original → the interval is labelled provisional in `cast.status` |
| 7 | A Target effect flies in its authored form: Ball, Bolt, Fog or Spray (`MGEF.DATA` flag bits 25 and 26), each form with its own maximum distance (`fMagicBoltMaximumDistance`, `fMagicFogMaximumDistance`, `fMagicSprayMaximumDistance`; §9.1) | Every Target effect flies as a ball to `fMagicProjectileMaxDistance` (PSC-TGT-003) | none | Shock Damage (Bolt) and Frost Damage (Fog) reach a different distance than the original's → PSC-TGT-003 names the forms and their settings for a later wave |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | Wave 1. The six magic skills level only by casting and the engine feeds none of them (§3 row 9). |
| Fallout 3 | no (Q1) | Out by Q1. xEdit's `SPIT` has no `Spell` type here, only `Actor Effect`, Disease, Power, Lesser Power, Ability, Poison and Addiction, with `Cost` and `Level` marked unused, and the GECK calls the record an *Actor Effect* with a `PC Start Effect` flag (GECK wiki *Actor Effect*). |
| Fallout: New Vegas | no (Q1) | As Fallout 3. |
| Skyrim (LE / SE / AE) | no (Q1) | Its cast action is a different design: a spell is assigned to the left or right hand, concentration spells are held and dual-casting is a per-school perk (UESP *Skyrim:Magic Overview*: "Unlike in Oblivion, magic must be equipped to a specific hand"). At HEAD no skill-advance emitter (`PendingSkillAdvances`) names a magic school. A Wave 2 PRD owns it. |
| Fallout 4 | no (Q1) | Out by Q1, not by a claim that the game lacks spellcasting: xEdit's FO4 `SPIT` carries a `Spell` type and the flags `PC Start Spell` and `Instant Cast`, and the Fallout 4 CK wiki's *Spell* page states no cast rule. |
| Fallout 76 | no | Online, out of the engine's single-player scope. |
| Starfield | no (Q1) | Out by Q1, on the same record shape as Fallout 4 (xEdit SF1 `SPIT`). |

### 7.2 Acceptance

Data: `Oblivion.esm`, read from the directory `crates/plugin/src/esm/test_paths.rs` resolves. No game file is checked in. A scenario passes when:

1. **Unit.** The gate refuses in the PSC-CST-002 order, each with its reason, and spends nothing (`cast_refuses_each_gate_in_order`); the actual cost at skill 5 and skill 100 equals §2's actual cost for three named `Oblivion.esm` spells (`cast_cost_follows_the_equation_not_the_stored_cost`, `crates/scripting/tests/player_cast_real_data.rs`): `StandardFireShield2Apprentice` (000A9806, Heat Shell, Alteration, Self, equation 54.3), `StandardFireDamageTarget1Novice` (000A97DF, Flare, Destruction, Target, stored 9 and equation 11.15, so the test fails if the stored cost is used) and `SE37CommandCreature4Expert` (0007B21B, Mirili's Expert Command, Illusion, Touch, Manual Spell Cost, stored 368 and Expert); a spell's required mastery is the tier of its base cost, or its stored `Level` when Manual (`cast_mastery_is_the_tier_of_the_base_cost`: Flare Novice, `StandardFireDamageTouch3Journeyman` 000A97E5 Journeyman, Mirili's Expert Command Expert); a multi-effect spell's primary school is its highest-cost effect's, the first of equal ones (`the_primary_school_is_the_costliest_effects_and_ties_go_to_the_first`, `crates/core/src/character/spell_cast.rs`, synthetic effects), and a script effect's is its `SCIT` school (`a_script_effects_school_is_its_scit_school`, `crates/scripting/tests/player_cast_real_data.rs`): in `SE13LpJyggalag8AreaTouch` (00083EA6) the script effect reads Destruction from `SCIT` where its `SEFF` is Alteration, and costs nothing, so Absorb Health makes the spell Restoration; in `SE09FakeSummonFleshAtronach` (0004FC63, one Self script effect) the `SCIT` school, Conjuration, is the cast's school and the skill it feeds.
2. **Unit, the feed.** A Self cast feeds its school once; a Touch cast on an unaffectable target feeds nothing; a script's `Cast`, the console's `magic.cast` and a power feed nothing (`player_cast_feeds_only_a_valid_target`, `other_casters_feed_nothing`: the console, ObScript `Cast`, a perk's activate choice and a power).
3. **Live, new script `oblivion-spellcasting.sh`** (indexed in [docs/smoke-tests/README.md](../smoke-tests/README.md)): Oblivion in `AnvilTheCountsArms` (the cell `oblivion-level-up.sh` uses), with the target actor the first patron `stealth.status` lists, as that script picks its fight, and Flare (000A97DF) as the Destruction spell: `spell.ready` on it, `spell.cast` at the actor, and `levelup` shows Destruction's banked experience rise by the use value of skill-use §1 while `levelup` no longer lists the school as unfed. The negative leg: a Touch cast at a container lands nothing and feeds nothing, the object rule PSC-TGT-002 settles. The script casts Burning Touch (`StandardFireDamageTouch1Novice`, 0004C91E) at the room's middle cupboard once `interaction.status` names it as a Container, and the touch ray reaches the cupboard's collider within the 150 BU pick length.
4. **Existing gates stay green.** `oblivion-level-up.sh` still passes (Blade 5 to 7, Sneak 5 to 6) and `combat.status` is unchanged by a spell cast.

---

## 8. Metrics

The numbers live in the [ROADMAP compatibility matrix](../../ROADMAP.md#compatibility-matrix), Oblivion row, beside the skill-use PRD's.

| Metric | Definition | Granularity |
|---|---|---|
| Magic schools fed | schools a player cast feeds ÷ 6 | Oblivion |
| UESP actions fed | UESP skill-use actions with an emitter ÷ 26 | Oblivion |
| Refusal coverage | gates with a named refusal test ÷ gates | Oblivion |

---

## 9. Delivery sequence

Phases are ordered by what they unblock; each ends with a measurement. The feed (Phase 3) depends on the skill-use drain and `FED_SKILLS` of [Oblivion skill-use leveling](oblivion-skill-use-leveling.md) (#7047, shipped phases, PRD ACTIVE); the readied-spell choice by menu waits on [Game UI integration](game-ui-integration.md) (M48, #7048) and is not required for any exit here.

**Phase 0 — Decode and verify (no engine code).** Decode the original executable's own spell-cast site, the one the skill-use work left open (`0x5fca20` is the item-use path, not this); write the "Player cast" section of `docs/engine/scripting.md` and the cast-rules section of the Oblivion ruleset design (PSC-CST-008); name the two spells, the cell and the target actor of §7.2 by EditorID; resolve §10. Close the **Phase 0: verify** items (results in §9.1):

| Item | Requirement |
|---|---|
| The executable's spell-cast site and what it feeds | PSC-USE-001, -002 |
| Mastery on authored spells: the cost tier against the stored `Level` | PSC-CST-003 |
| `Manual Spell Cost` honoured as in §2; Area and magnitude rounding in the effect cost | PSC-CST-004 |
| Refusal, or a cast anyway, on short Magicka | PSC-CST-002 |
| Primary school rule on authored spells; script-effect school | PSC-USE-003 |
| The skill-use value index ("Alteration 4.0 = value 2") | PSC-USE-001 |
| Whether a cast is heard by the detection tick | PSC-NG-007 |
| Luck term applies to the cast, not only the display | PSC-CST-004 |
| Casting interval | PSC-CST-006 |
| Touch reach; what Touch does to a door, a chest, a corpse, and a `Touch Spell Explodes w/ no Target` spell with no target | PSC-TGT-002 |
| Area rule | PSC-TGT-005 |
| Armor effectiveness formula (Q3, for the later requirement; sourced in revision 12) | PSC-NG-006 |
| No player-castable magic type on Fallout 3, New Vegas, Fallout 4; Starfield powers only; Skyrim's hand model and skill feed | §7.1 |

Exit: each item closed with a source or filed as an issue, and the Oblivion cast site recorded in the design.

**Phase 1 — Ready, gate, cost, Self.** PSC-RDY (with the saved readied spell and its format bump), PSC-CST, PSC-TGT-001, PSC-DX. Carry range, effect cost inputs, the `SPIT` `Level`, `Cost` and `Manual Spell Cost` flag, and the school (MGEF or `SCIT`) into `CastEffect` / `CanonicalSpell`. Exit: scenario 1 passes and a Self cast spends Magicka and Fatigue on `Oblivion.esm`.

**Phase 2 — Touch and Target.** PSC-TGT-002 to -005. Exit: the Touch and Target legs of scenario 3 land on a live actor and a missed Target cast pays its cost.

**Phase 3 — Feed the schools.** PSC-USE. Exit: scenarios 2, 3 and 4 pass; the six codes are in `FED_SKILLS`; the ROADMAP matrix row shows the metrics of §8.

### 9.1 Phase 0 results (2026-11-19, #7078)

Sources: UESP *Oblivion:Magic Overview* and *Oblivion:Spell Making* and the CS wiki (read as wikitext from `/mnt/data/src/reference/` on 2026-11-19), xEdit's `wbDefinitionsTES4.pas`, `Oblivion.esm` read with `crates/plugin/examples/spell_cast_census.rs`, and a decode of `Oblivion.exe` 1.2.0.416 (addresses in `docs/engine/charal-oblivion-ruleset.md`, "Spell casting"). The census read 1,137 `SPEL` and 145 `MGEF` records. A claim that could not be verified was marked **Phase 0: verify** and is a row of the table below or of #7082.

| Item | Result | Requirement |
|---|---|---|
| The executable's spell-cast site | Three sites (`0x6994cc`, `0x69b9ea`, `0x69bc51`) call the player's skill-use entry with skill = the primary school's actor value, usage 1 and no multiplier; the earlier scan missed them (the encoding is `add reg, 0x39c`). The spell spender `0x5fc890` awards nothing. Only item types Spell and Poison award. | PSC-USE-001, -002 |
| Skill-use value index | Usage 1 reads the second use value, so "Alteration 4.0 = value 2" is consistent: 4, 6, 1.2, 3, 3, 0.6. | PSC-USE-001 |
| Stored cost against the equation | UESP's in-game Magicka equals the equation for 312 of 315 matched auto-calculated spells and the stored `Cost` for 174: the stored cost is stale. Flare: stored 9, equation 11.15, UESP 11. | §2 base cost |
| Manual Spell Cost | Honoured: stored `Cost` times the modifier, **rounded down** with no minimum (CS wiki *Category:Spell Cost*; rev 11 corrected the decode's "not rounded"), skill of the primary school. | PSC-CST-004 |
| Effect cost, rounding, Luck | Floored per effect, at least 1; magnitude, duration and area zeroed by MGEF flags; Luck applies to the cast (`0.4 × (Luck − 50)` to the skill). | PSC-CST-004 |
| Mastery | Derived from the base cost for an auto-calculated spell (thresholds 25.5 / 62.5 / 149.5 / 399.5 in `Oblivion.esm`), the stored `Level` for a Manual one; the player's tier from the base skill at 25 / 50 / 75 / 100; NPCs are not gated. | PSC-CST-003 |
| Short Magicka | Refused when the cost is greater than current Magicka (equal allowed), code 1; insufficient skill is code 3. | PSC-CST-002 |
| Primary school, script effects | The costliest effect's school, ties to the first; a `SCIT` school when the effect has one, else the MGEF school; schools 0 to 5 map to actor values 20 to 25. | PSC-USE-003 |
| Fatigue | `fFatigueCastBase + fFatigueCastMult × Magicka cost`: 1 on `Oblivion.esm`. | PSC-CST-005 |
| Area | Radius `fMagicUnitsPerFoot` (22) × scale × area; every actor within it but the hit target. | PSC-TGT-005 |
| Touch with no target | Nothing, unless the SPIT flag `0x80` is set and the area is above 0, which explodes at the aim position. | PSC-TGT-002 |
| Per-game scope | FO3 and FNV have no `Spell` type; FO4 and Starfield records keep it, so their scope rests on Q1; Skyrim's hand model sourced. | §7.1 |
| Unbound keys | Listed in `.claude/issues/7078/INVESTIGATION.md`; the original's PC default for Cast is `C`, which the engine binds to Block. The owner named `C` for `CastSpell`, which displaces the keyboard Block binding. | PSC-CST-001 |
| **Settled from documents, 2026-11-20 (#7082)** | The owner selected 105 BU from the CS wiki as a provisional hand-reach approximation; Touch on objects follows UESP's rule (an effect lands on a target it can affect; Open and Reanimate are not built); the armor casting penalty is sourced (CS wiki *Casting penalty*, §9.1), still out of Wave 1. The 105 BU reach is superseded by the Touch-reach trial row below (revision 13). | PSC-TGT-002, PSC-NG-006 |
| **Authored data (#7082)** | The shipped cast clips give the Self, Touch and Target release markers at 0.366667, 0.266667 and 0.400000 s. The clip lengths vary by range and variant. The [Oblivion ruleset](../engine/charal-oblivion-ruleset.md#what-a-cast-spends) lists the end markers. Of 72 Target-capable effects in the stock master, Frost Damage uses Fog, Shock Damage uses Bolt, and 70 use Ball, from the [xEdit TES4 schema](https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.6/Core/wbDefinitionsTES4.pas) and the master. | PSC-CST-006, PSC-TGT-003 |
| **Original-game area XP (#7082)** | Fire Ball awarded 1.2 Destruction XP on a direct hit alone and 1.2 with two additional splash victims. A separate Capstone Destruction cast that hit a wall and damaged a nearby actor awarded 0. See the [trial conditions](../engine/charal-oblivion-ruleset.md#area-experience-trial-7082). | PSC-USE-002 |
| **Original-game Touch reach (#7082)** | Cold Touch hit a stationary scale-one guard at 183 BU and missed at 184 BU. The trial verified that `iActivatePickLength` controls the ray. See the [trial conditions and setting overrides](../engine/charal-oblivion-ruleset.md#touch-distance-trial-7082). | PSC-TGT-002 |
| **Original-game repeat timing (#7082)** | Unarmed, unblocked repeats in first person took 2.02–2.12 s for Self, 1.42–1.43 s for Touch, and 1.33–1.37 s for Target. See the [test conditions](../engine/charal-oblivion-ruleset.md#live-cast-repeat-timing-7082). | PSC-CST-006 |
| **Original-game detection trial (#7082)** | The guard’s score stayed at −31 when the stationary player cast. Movement raised it to −12 or −13 without line of sight. See the [spell and setting controls](../engine/charal-oblivion-ruleset.md#casting-noise-trial-7082). | PSC-NG-007 |

#### Provenance of the cast rules (rev 11, 2026-11-20)

The owner ruled out decoding the executables for rules, so each §2 rule is held to a documented source. The full table is in the [Oblivion ruleset](../engine/charal-oblivion-ruleset.md#spell-casting-7078-provenance-relabelled-2026-11-20). In short: the cost equation, rounding, skill factor, Luck term, mastery, primary school, Fatigue point, feed rules, projectile speed and range are **documented** (CS wiki *Category:Spell Cost*, *Effective Skill Level*, *Fatigue Game Settings*, *Spell*; UESP *Magic Overview*); the Manual Spell Cost rounding the decode got wrong is now the documented floor. Five items stay **executable-derived** and are declared, not hidden (the Touch ray now comes from the original-game `iActivatePickLength` trial): `MGEF` flag bit 22, the Magicka refusal boundary, the `SCIT` school precedence, the feed count and use value beyond UESP's figures, and the refusals PSC-NG-008. They stay in force until a documented source replaces them (#7082).

### 9.2 Phase 1 results (2026-11-19, #7079)

The cast action is built for Self-range spells on Oblivion. Verified three ways: unit tests (core 11, scripting 8, binary 5, save round-trip 1), the real master (`crates/scripting/tests/player_cast_real_data.rs`, five `--ignored` tests against `Oblivion.esm`: Flare costs 11 not its stored 9, the tiers, Mirili's Manual Spell Cost, a Self cast spending Magicka and Fatigue), and a live run (Oblivion, `AnvilTheCountsArms`, `--player --bench-hold`: `input.press next_spell` readied Heal Minor Wounds, `input.press c` took Magicka from 100 to 81 and Fatigue by 1 with the effect applied, a repeat inside the interval was refused with `cooldown`, Flare was refused for its Target range, and a saved readied spell came back after a `load`).

| Item | Result | Requirement |
|---|---|---|
| Rules | `SpellCastModel` (`crates/core/src/character/spell_cast.rs`) on the profile's `spell_cast` row (`Some` on Oblivion), retuned by the load order's `fMagic*`, `iSkill*Min`, `fActorLuck*` and `fFatigueCast*` settings. | PSC-CST-003, -004, -008 |
| Data | `MagicEffectItem` carries the `EFIT` range and the `SCIT` school, `SpelRecord` the `SPIT` `Level`; `CanonicalSpell.cast` holds the header and every effect. | §3 rows 7, 10; PSC-TGT-001 |
| Spend | One system, `player_cast_system` (Update, after `combat_input_system`, access declared), applies the Magicka and Fatigue damage through `ActorValues::apply_damage`, then calls `cast_spell`. | PSC-CST-005, -007, -009 |
| Gate | In the stated order, each refusal named: cannot act, cooldown, nothing readied, not known, no rules, not castable, range (Phase 1 only), mastery, Magicka. | PSC-CST-002 |
| Keys | `C` casts (owner), keyboard Block moved to `X`, `Right` and `Left` arrows cycle (confirmed by the owner on 2026-11-22; Q2), all remappable through `controls.bind.cast_spell`, `next_spell` and `previous_spell`. `input.press` takes `cast`, `next_spell` and `previous_spell`; its `c` shorthand is now Cast and `x` is Block. The action masks widened from 16 to 32 bits. | PSC-CST-001, PSC-RDY-003 |
| Extensions | The three actions are not in the host contract's `input-action` enum (`crates/mod-runtime/wit/host.wit`), so no extension observes them; adding them is a host-contract change the PRD does not promise. | PSC-CST-001 |
| Save | `ReadiedSpell` is a saved player component; `FORMAT_MAJOR` 140. A readied spell that left the list clears (PSC-RDY-001). | PSC-RDY-001, -004 |
| Commands | `spell.ready`, `spell.cast`, `spell.list`, `cast.status` (provisional interval and reach labelled). | PSC-RDY-002, PSC-DX-001 to -003 |
| Not built | Touch and Target resolution (Phase 2), the skill feed and `FED_SKILLS` (Phase 3), the cast-rule refusals PSC-NG-008. | PSC-TGT-002 to -005, PSC-USE |

### 9.3 Phase 2 results (2026-11-20, #7080)

Touch and Target resolution is built on Oblivion. Verified three ways: unit tests (core 13, scripting 14, binary: the flight and cast-key tests), the real master (`crates/scripting/tests/player_cast_real_data.rs`: Searing Grasp lands on the actor in reach and a miss pays the same cost, Flare launches at `1000 × 1.0` units per second to a range of 10000, an area spell reports `area ignored`), and a live run (Oblivion, `AnvilTheCountsArms`, `--player --bench-hold`: Burning Touch `0004C91E` took a patron from Health 16 to 6 with `touched` in the report; Flare took another from 72 to 66 with `impact=landed ... (effects applied)`; a Flare cast at the ceiling ended `impact=missed` and paid its 14 Magicka).

| Item | Result | Requirement |
|---|---|---|
| Range | `CastEffect` carries the `EFIT` range and the area (zero where the `MGEF` flags no area); the effect start takes a filter (`start_effects_where`, `start_spell_effects`) and each effect keeps its position in the spell as its `effect_index`, so a subset starting on another actor still finds its own conditions. The story `CAST` event is raised once per cast (`raise_cast_event`), by `cast_spell` and by the player's cast. | PSC-TGT-001, PSC-CST-007 |
| Touch | The cast resolves the first live actor hit along the camera aim through `combat::actor_in_reach`. Revision 13 uses the activation pick length for a ray (PSC-TGT-002, #7082). A corpse or a non-actor in the way is no target. | PSC-TGT-002 |
| Target | `settle` returns a `TargetLaunch`; the binary flies one `Projectile` with `spell: Some(..)` on `ActiveProjectiles`, which queues no damage: on a live actor other than the shooter it calls `land_target` (the spell's Target effects start as the player's cast) and otherwise `missed_target`. `cast.status` reports `impact=flying`, `landed on <entity>` or `missed`. A corpse stops it without landing. | PSC-TGT-003 |
| Speed and range | `fMagicProjectileBaseSpeed` times the `MGEF` multiplier, and `fMagicProjectileMaxDistance`, on `SpellCastModel` (CS wiki; retuned by the load order's settings). The `MGEF` field is a multiplier, not a speed: Flare's is 1.0, so reading the field as a speed flies a one-unit-per-second projectile that never lands. | PSC-TGT-003 |
| Multi-range | Self lands at the release on the player, Touch on the touched actor, Target on impact; a spell none of whose effects found a target pays and reports `no target`. | PSC-TGT-004 |
| Area | An authored Touch or Target effect with an area above 0 (and an `MGEF` that does not flag no area) is counted `area ignored` in the report and is not started on anyone. | PSC-TGT-005 |
| Not built | The skill feed, the `FED_SKILLS` change and the smoke script (Phase 3); Touch on doors, chests and corpses (#7082); the Touch-that-explodes flag; Bolt, Fog and Spray forms. | PSC-USE, PSC-TGT-002 |

### 9.4 Phase 3 results (2026-11-20, #7081)

The six schools are fed. Verified by unit tests (scripting `player_cast_feeds_only_a_valid_target`, `a_spell_with_no_school_feeds_nothing`, `other_casters_feed_nothing`, and the `FED_SKILLS` report test), by the real master (a Self cast of Heat Shell on `Oblivion.esm` queues Alteration, code 20, at 4.0), and by the Phase 2 live run for the landing the award hangs on. The live gate `docs/smoke-tests/oblivion-spellcasting.sh` passed on 2026-11-20 (Oblivion, `AnvilTheCountsArms`): Flare landed on a patron (Health 16 to 10) and fed Destruction, a ceiling shot missed and fed nothing, Heal Minor Wounds fed Restoration, `magic.cast` fed nothing and the `combat.status` counters did not move (scenarios 3 and 4 of §7.2).

| Item | Result | Requirement |
|---|---|---|
| Emitter | `player_cast::settle` queues one use for the primary school (actor value 20 to 25, usage 1, multiplier 0) through `queue_skill_use`, which now says whether it queued; a spell with no Target-range effect awards at the release when an effect applied (Self always, Touch on an actor in reach), a spell with one awards in `land_target` at impact and never at the release, a miss or a refusal awards nothing. | PSC-USE-001, -002 |
| School | The primary school of Phase 0 (`SpellCastModel::primary_school`); a spell with none reports `no school` and feeds nothing. | PSC-USE-003 |
| Fed list | 20 to 25 joined `FED_SKILLS` in the same commit: on 2026-11-20 `levelup` reported 14 of 21 fed and no longer listed the schools, and `oblivion-level-up.sh` expected 14/21 (later emitters have moved both; the current count is the ROADMAP's). | PSC-USE-004 |
| Other casters | `magic.cast`, ObScript `Cast` and a perk's activate choice call the shared effect start, which queues nothing. | PSC-USE-005 |
| Report | `cast.status` and the cast report name the school fed (`fed Destruction`), `no school`, and the impact's award. | PSC-DX-001 |
| Metrics | Published in the [ROADMAP Oblivion row](../../ROADMAP.md#compatibility-matrix), which holds the current numbers; on 2026-11-20: 6 of 6 schools fed, 8 of 8 refusals with a named test, magic 6 / 8 UESP actions. | §8 |
| Not built | Telekinesis's exemption (a projectile lands on actors only, so no Telekinesis effect lands). Review round 2's two items before SHIPPED are built (revision 16): scenario 3's container leg in `oblivion-spellcasting.sh` (two passing runs on 2026-11-22) and scenario 1's real-data `SCIT` test. | PSC-USE-002, PSC-TGT-002, PSC-USE-003 |

---

## 10. Decisions required

1. **Q1 — Which games' casting is in this PRD.** Recommended: Oblivion only (Wave 1). Skyrim's per-hand casting and Fallout's absence of it are separate designs; folding them in doubles the Phase 0 verify work for a feed that Skyrim does not need. Alternative: Oblivion and Skyrim together, which makes this a two-dialect PRD and moves Skyrim's hand model into Wave 1.
   **Decided** (2026-11-19): as recommended: Oblivion only. Skyrim's cast action is a separate PRD (§7.1).
2. **Q2 — The default key.** Recommended: the owner names the `CastSpell` key from the keys Phase 0 lists as unbound, with `NextSpell` / `PreviousSpell` on neighbouring keys. Alternative: the `CastSpell` key only, with `spell.ready` as the sole way to choose, which leaves a mage unable to switch spells without the console until the menu lands. (The `spell.ready` command and the cycle action are not in question: PSC-RDY-002 and -003 settle them.)
   **Decided** (2026-11-19): as recommended: the owner names the `CastSpell` key from the unbound keys Phase 0 lists, with `NextSpell` / `PreviousSpell` on neighbouring keys. The owner named `C` for `CastSpell` on Oblivion (2026-11-19), the original's PC default; the engine binds `C` to Block, so the keyboard Block binding moves in Phase 1 (PSC-CST-001). Phase 1 proposed two unbound keys for `NextSpell` and `PreviousSpell` and built the Right and Left arrows (§9.2); the owner confirmed them on 2026-11-22.
3. **Q3 — Spell effectiveness from armor.** Recommended: out of Wave 1; a later requirement once Phase 0 sources the armor-skill formula. Alternative: in Wave 1, which delays the cast action behind an unsourced formula.
   **Decided** (2026-11-19): as recommended: out of Wave 1. The armor-effectiveness formula stays a Phase 0 verify item, and a later revision adds the requirement once it is sourced. Sourced in revision 12 (§9.1); it stays out of Wave 1.
4. **Q4 — How a Target spell flies.** Recommended: the weapon projectile path (`ActiveProjectiles`) at the MGEF's speed, a straight line. Alternative: an instant ray cast first, with no visible flight, and the projectile as a later phase; the skill use is identical either way.
   **Decided** (2026-11-19): as recommended: the weapon projectile path at the MGEF's speed, a straight line (PSC-TGT-003).
5. **Q5 — The saved ready spell.** Recommended: save it, with the save format bump that needs (the `FORMAT_MAJOR` rule, README T2). Alternative: do not save it, so a loaded game starts with nothing readied, no bump, and a mage re-readies after every load.
   **Decided** (2026-11-19): as recommended: the readied spell is saved, with a save-format bump in the phase that adds the component (PSC-RDY-004).

---

## 11. Document map

| Need | Go to |
|---|---|
| Skill-use emitter contract and the fed list | [skill-use.md](../engine/skill-use.md) |
| The skill-use PRD this one unblocks | [oblivion-skill-use-leveling.md](oblivion-skill-use-leveling.md) |
| The sneak stance and detection | [sneak-detection.md](../engine/sneak-detection.md) |
| Oblivion's ruleset | [charal-oblivion-ruleset.md](../engine/charal-oblivion-ruleset.md) |
| The magic menu | [game-ui-integration.md](game-ui-integration.md) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-19 | First draft, from the cast-action follow-up of the Oblivion skill-use PRD | — |
| 2 | 2026-11-19 | Review round 1 folded in: cast callers (console, ObScript `Cast`, perk activate choice), spell cost equation and `SPIT` `Level` / `Manual Spell Cost`, script-effect school, ownership of the Oblivion cast rules and scheduler access (PSC-CST-008, -009), measurable interval and reach, §7.1 citations, scenario data, phase dependencies, term fixes (range, mastery, castable spell), detection noise (PSC-NG-007), Q2 and Q4 wording | — |
| 3 | 2026-11-19 | Owner decisions Q1 to Q5 recorded, each as recommended (Oblivion only; the owner names the key; armor effectiveness out of Wave 1; Target spells on the projectile path; readied spell saved with a format bump), and folded into §7.1, PSC-RDY-004, PSC-CST-001, PSC-NG-006, §6 row 3 and Phase 1 | — |
| 4 | 2026-11-19 | Approved by the owner with the five decisions of rev 3 taken as recommended; no content change | owner: approved |
| 5 | 2026-11-19 | Tracking issues filed (parent #7077, phases #7078 to #7081); state ACTIVE; no content change | — |
| 6 | 2026-11-19 | Phase 0 results recorded (§9.1, #7078) and the requirements they contradict corrected: mastery from the base cost (the stored `Level` only for Manual Spell Cost), effect costs floored per effect, Fatigue from `fFatigueCastBase`, one award at release or at impact (PSC-USE-002), Powers and Lesser Powers not castable here (PSC-NG-002, §2), the decoded Touch reach, §7.1 evidence, test spells named, PSC-NG-008 added; residual verify items filed as #7082 | — |
| 7 | 2026-11-19 | The owner named `C` for `CastSpell` on Oblivion (Q2): PSC-CST-001 and §9.1 record it and the keyboard Block binding it displaces; the `NextSpell` and `PreviousSpell` keys stay open for Phase 1 to propose | — |
| 8 | 2026-11-19 | Phase 1 results recorded (§9.2, #7079): the keys as built (`C` casts, keyboard Block on `X`, the arrow keys cycle), the spell actions kept out of the host contract's `input-action` enum, `FORMAT_MAJOR` 140, and PSC-CST-001's text | — |
| 9 | 2026-11-20 | Phase 2 results recorded (§9.3, #7080): Touch and Target resolution as built, PSC-TGT-003's speed and range from the CS wiki's `fMagicProjectileBaseSpeed` and `fMagicProjectileMaxDistance` with the `MGEF` field as a multiplier, and the Bolt, Fog and Spray forms noted as unmodelled (#7082) | — |
| 10 | 2026-11-20 | Phase 3 results recorded (§9.4, #7081): the feed as built, the six codes in `FED_SKILLS`, the metrics published; the live gate `oblivion-spellcasting.sh` written and not yet run | — |
| 11 | 2026-11-20 | Provenance relabelled after the owner ruled out executable decoding for rules (§9.1): documented rules cited, six executable-derived items declared; Manual Spell Cost rounding corrected to the CS wiki's floor | — |
| 12 | 2026-11-20 | #7082 worked from documents: the Touch reach moves from the executable-derived 76.8 BU to the CS wiki's 105 BU hand reach (owner decision; `SpellCastModel::hand_reach`, no longer GMST-retuned); Touch on objects follows UESP *Magic Overview* (no mechanic built for Open or Reanimate); the armor casting penalty is sourced (§9.1); the interval, the detection noise and the area awards have no documented source and stay open | — |
| 13 | 2026-11-20 | #7082: record original-game trials, use the verified activation-pick ray for Touch, and verify projectile-form flags against the original master | — |
| 14 | 2026-11-22 | Review round 2 folded in: §3 and the "Today" clauses restated as built (symbols, not lines); the Related rows link the written designs; scenario 1's test names corrected and its `SCIT` case, with scenario 3's container leg, listed as the work left before SHIPPED; UESP's fixed casting speed set against the trial (§6 row 6); the Fatigue citation; the 105 BU row marked superseded; §6 rows for armor (sourced) and ball-only flight (row 7); Q2's cycle keys recorded as awaiting the owner; phase definitions moved before §9.1; status line trimmed, its source note moved to §9.1; §9.4 metrics dated | claude: revise |
| 15 | 2026-11-22 | The owner confirmed the Right and Left arrows for `NextSpell` and `PreviousSpell` (Q2, §9.2); that SHIPPED blocker is closed | — |
| 16 | 2026-11-22 | The two tests review round 2 left before SHIPPED are written: scenario 1's `SCIT` school test on `SE13LpJyggalag8AreaTouch` and `SE09FakeSummonFleshAtronach` (the first's script effect costs nothing, so a second spell shows the `SCIT` school deciding the cast), and scenario 3's container leg with Burning Touch at the middle cupboard | — |
