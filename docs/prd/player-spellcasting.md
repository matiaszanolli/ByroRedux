# Player Spellcasting — Product Requirements

**Status**: PROPOSED (2026-11-19). Revision 2 folds in review round 1: the three callers of `cast_spell`, the spell cost equation and the authored mastery level, script-effect schools, ownership of the Oblivion cast rules, measurable interval and reach, and the per-game citations. Revision 1 was the first draft: the player's own cast action, which Oblivion's skill-use PRD leaves open and which feeds Oblivion's six magic skills. Statements about how an original engine behaves cite the UESP *Oblivion:Magic Overview* and *Oblivion:Spell Making* pages (read as wikitext from `/mnt/data/src/reference/uesp-wiki/` on 2026-11-19) and xEdit's `wbDefinitionsTES4.pas` (`/mnt/data/tmp/`); a claim that could not be verified is marked **Phase 0: verify** instead of being stated as fact, and every such item is a row of the Phase 0 verify table (§9).

**Scope**: The player casts a spell they know: choosing which spell is ready, the cast input, the checks that gate the cast, the Magicka and Fatigue it spends, the target a Self, Touch or Target spell resolves, and the one consumer that motivates it, the skill use a cast with a valid target gives its school. Wave 1 is Oblivion (the only game whose magic skills the engine does not yet feed). Skyrim's two-hand casting, Fallout and Starfield are outside it (§7.1). Cast animation, cast and impact visuals, scrolls, powers, spell making and the magic menu are explicitly not here (§4.2).

**Related**: this document states the *promise*; the designs below own the *how*. Every mechanism promised here has an owner in this table, or Phase 0 writes the missing design.

| Mechanism | Owning design |
|---|---|
| Skill-use emitter contract and the fed-skill list | [Skill use](../engine/skill-use.md#4-the-emitter-contract-osl-xp-007) |
| Oblivion's ruleset, where the cast rules (cost equation, authored mastery, the Fatigue point) belong (PSC-CST-008) | [Oblivion ruleset](../engine/charal-oblivion-ruleset.md); Phase 0 adds the section |
| Sneak stance and the detection tick, should a cast prove audible (PSC-NG-007) | [Sneak and detection](../engine/sneak-detection.md) |
| The magic menu that will replace the console's ready command | [Game UI integration](game-ui-integration.md) |
| Spell catalog, effect application, the cast runtime | none at HEAD; Phase 0 writes the "Player cast" section of `docs/engine/scripting.md` |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119 meanings, as in the related designs. Requirement IDs use the `PSC-` prefix.

---

## 1. Product thesis

Oblivion's magic skills level by casting. The engine already runs a spell once something calls `cast_spell`, but only the console, an ObScript `Cast` and a perk's activate choice do (§3 row 2), so a player who plays a mage cannot cast and their six school skills never move. The skill-use PRD refuses to feed those skills from a script's forced cast, because that would be a stand-in for the real action ([Oblivion skill-use leveling](oblivion-skill-use-leveling.md), OSL-USE-006). This PRD builds the real action.

The action is the original's, not an invention: the player knows spells (a list), readies one, and casts it; the cast is gated by the spell's school skill and the player's Magicka, costs both Magicka and one Fatigue, and cannot fail (UESP *Oblivion:Magic Overview*, "Comparison to Morrowind"). What the engine adds is only the plumbing between a key press and the cast runtime that already exists.

The corollary that shapes every requirement: **a cast is the player's, or it is not a cast.** The one path that gates, charges and feeds is the player's; a script's `Cast`, the console's `magic.cast` and a perk's activate-choice cast stay what they are, a spell started without a caster's cost and without skill use. The engine will not feed a school from any other path.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Readied spell** | The one spell the player has chosen to cast next; it persists until another is chosen or it leaves the spell list. |
| **Cast action** | The input edge that casts the readied spell. |
| **Range** | Where an effect lands, per effect: Self, Touch or Target. Oblivion's `EFIT` carries it as `Type` (0, 1, 2; xEdit `wbEffectTypeEnum`), and the repository's decode calls it the range (`ranges`, `oblivion_effect_chain`). |
| **Effect cost** | One effect's magicka cost before the caster's skill adjusts it: `B × M × D × A` with `B = MGEF base cost ÷ 10`, `M = max(1, magnitude)^1.28`, `D = max(1, duration)`, `A = max(1, area × 0.15)`, times 1.5 for a Target effect (UESP *Spell Making*, "Spell Cost"). |
| **Base cost** | A spell's magicka cost before skill: the sum of its effect costs, or the `SPIT` `Cost` when the `Manual Spell Cost` flag is set (xEdit TES4 `SPIT`; **Phase 0: verify** that the original honours the flag the same way). |
| **Actual cost** | Each effect's cost times `fMagicCasterSkillCostBase + fMagicCasterSkillCostMult × (1 − Skill ÷ 100)` (default `1.4 − 0.012 × Skill`), with the skill of the effect's own school, summed (UESP *Magic Overview*; *Spell Making*). |
| **Mastery** | The skill a spell requires: Novice, Apprentice, Journeyman, Expert or Master, the authored `SPIT` `Level` (xEdit TES4 `SPIT`), checked against the unfortified skill (UESP *Magic Overview*, "Skill levels and training"). |
| **Castable spell** | A spell of type Spell or Lesser Power in the player's `SpellList`. |
| **School** | Alteration, Conjuration, Destruction, Illusion, Mysticism or Restoration: MGEF `magic_school` 0..5 (xEdit `wbMagicSchoolEnum`; 6 is None), actor value `20 + school`. A script effect carries its own school in `SCIT` (xEdit TES4 `SCIT`, "Magic school"). |
| **Primary school** | A multi-effect spell's school: that of its single effect with the highest effect cost (UESP *Spell Making*; the rule is stated there for spells the player makes, so **Phase 0: verify** it for authored spells). |
| **Valid target** | A target the cast affected: Self always; for Touch and Target, a target an effect of the spell landed on (UESP: casting a fireball on a chest gives no experience, nor does an open spell on an NPC). |
| **Cast intent** | The result of the player's cast action, before any effect lands: refused (with a reason) or started. |

---

## 3. Core model

A cast has four stages, in order: **ready** (a spell is selected), **gate** (the player may cast it now), **resolve** (each effect finds its target by range), **settle** (cost paid, effects started, skill use queued). A refusal at the gate pays nothing and queues nothing, and says why.

The inventory below is the state at HEAD on 2026-11-19.

| # | Piece | Class | Today |
|---|---|---|---|
| 1 | Starting a spell's effects on a target | exists | `cast_spell(world, caster, target, spell)` (`crates/scripting/src/magic_effects.rs:250`) starts every effect that lands and raises the Story Manager `CAST` event; it spends nothing and takes no range |
| 2 | Who calls it | script, console, perk | three callers outside tests: the console's `magic.cast <spell> [target\|.]` (`byroredux/src/commands/gameplay.rs:273`), ObScript `Cast` (`crates/scripting/src/obscript_effects/presentation.rs:37`) and a perk's activate-choice *Apply Spell* (`crates/scripting/src/perk_entry_points/activate.rs:195`, caster the activator); no input path reaches it |
| 3 | The spell list | exists | `SpellList(Vec<u32>)` (`crates/scripting/src/magic.rs:158`) is the actor's known spells; nothing reads it as a choice |
| 4 | Ready state | missing | no readied-spell component; `ReadyWeapon` (`combat.rs`, `InputAction::ReadyWeapon`, #6942) is the precedent for a toggled ready state |
| 5 | Input | missing | `InputAction` (`crates/sdk/src/event.rs:671`) has no cast, ready-spell or cycle variant |
| 6 | Magicka and Fatigue cost | missing | `magic_effects.rs` does not touch Magicka or Fatigue |
| 7 | Skill and cost data per spell | decoded, not carried | MGEF `base_cost`, `magic_school` and `projectile_speed` (`crates/plugin/src/esm/records/misc/magic.rs`, `MgefRecord`) and Oblivion's `EFIT` range are decoded, but `CastEffect` and `CanonicalSpell` (`magic_effects.rs:61`, `magic.rs`) carry none of them, nor the cost inputs the equation needs (magnitude, duration, area) as a computed cost |
| 8 | A projectile a spell could ride | exists for weapons | `ActiveProjectiles` (`byroredux/src/systems/projectiles.rs`) flies a straight line at a `PROJ` record's speed and queues a hit on a live actor; it is driven by ammunition (`Weapon.Fire`, `player_shot`) |
| 9 | Feeding a skill | exists | `queue_skill_use` (`byroredux/src/skill_use_emit.rs`); `FED_SKILLS = [13, 14, 16, 17, 18, 26, 27, 31]` (`crates/scripting/src/oblivion_skill_use.rs:437`) holds no magic school (20..25) and `levelup` lists them unfed |
| 10 | The authored spell header | partly decoded | `decode_spit` (`magic.rs:80`) reads Oblivion's type, `Cost` (@4) and flags (@12) and drops `Level` (@8), the authored mastery; `SpelRecord.cost` is documented as unused on Oblivion, which xEdit's `Manual Spell Cost` flag contradicts. A script effect's school (`SCIT`) is not read for a cast either |

---

## 4. Goals and non-goals

### 4.1 Goals

- **PSC-G-001 — A mage can cast.** On Oblivion, the player readies a known spell and casts it with an input, and the spell's effects land by range.
- **PSC-G-002 — The six schools level by casting.** A cast with a valid target feeds the spell's primary school by the `SKIL` use value, and nothing else feeds a school.
- **PSC-G-003 — The cost is the original's.** The Magicka a cast spends follows the effect-cost equation and the skill adjustment, and a cast the player cannot afford or is too unskilled for is refused with the reason.
- **PSC-G-004 — One path.** The player's cast, a script's `Cast`, the console's `magic.cast` and a perk's activate choice reach the same effect-start code; the player's path alone gates, charges and feeds.

### 4.2 Non-goals

- **PSC-NG-001.** Cast animation, casting sound and the cast, travel and impact visuals. The engine has no spell animation or effect-visual system; the cast action's feedback is the log and the console report until one exists.
- **PSC-NG-002.** Scrolls and powers. A scroll costs no Magicka and a power gives no skill use (UESP *Magic Overview*), so neither belongs to the feed this PRD exists for; they get a PRD when their menus and item path do.
- **PSC-NG-003.** Spell making and the spell-making altar, and enchanting. They create spells; this PRD casts them.
- **PSC-NG-004.** The magic menu and its hotkeys. They are the Game UI PRD's; the readied spell is chosen through a command and a cycle action until the menu exists.
- **PSC-NG-005.** Non-player casters. The AI casting spells at the player is the AI packages PRD's; the shared `cast_spell` path (PSC-G-004) is all this PRD asks of it.
- **PSC-NG-006.** Spell effectiveness reduced by worn armor. It is real (UESP *Magic Overview*, "Using Armor": wearing armor lowers the magnitude, rounded down) and the formula is sourced only from a tips section, so it is a **Phase 0: verify** item and a §10 decision, not a Wave 1 requirement.
- **PSC-NG-007.** Whether a cast is a sound the detection tick hears. No source says so for Oblivion, so it is a **Phase 0: verify** item (§9); the sneak design's attack-noise hook is where a verified answer would land, in a later revision.

---

## 5. Requirements by area

### 5.1 Ready and choose (`PSC-RDY`)

- **PSC-RDY-001.** The player MUST have a readied spell, a form id of a spell in their `SpellList` or none. A readied spell that leaves the list (removed, or a load that no longer has it) MUST clear. Today: no such state exists.
- **PSC-RDY-002.** `spell.ready <spell form id>` MUST ready a spell the player knows and report its name, its primary school and its actual cost; it MUST refuse a spell not in the list and say so.
- **PSC-RDY-003.** An `InputAction` pair (`NextSpell`, `PreviousSpell`) SHOULD cycle the readied spell through the player's castable spells in list order and report the new one. Today: no cycle action exists.
- **PSC-RDY-004.** The readied spell MUST survive a save and a load, and a loaded save that names a spell no longer present MUST clear it without an error (PSC-RDY-001). A save-format bump is a §10 decision (Q5).

### 5.2 The cast action (`PSC-CST`)

- **PSC-CST-001.** An `InputAction::CastSpell` MUST exist, with a default binding the owner decides (Q2), and its press edge MUST be the only input that casts. Today: no such variant exists.
- **PSC-CST-002.** A cast MUST pass the gate in this order and refuse at the first failure, naming it: the player can act and is in character mode (`player_can_act`, the check `combat_input_system` already uses); a spell is readied and is still in the list; the spell is castable (§2: a Power, a Disease or an Ability is not castable here); the player's unfortified skill in the spell's primary school meets the spell's mastery (PSC-CST-003); the player's current Magicka covers the actual cost (PSC-CST-004). A refusal MUST spend nothing and queue no skill use. The order is the engine's; **Phase 0: verify** what the original does when Magicka is short (refuse, or cast anyway), which UESP does not say, and keep the refusal until it is settled.
- **PSC-CST-003.** The mastery a spell requires MUST be its authored `SPIT` `Level`, which the decode MUST carry into the spell catalog (Today it is dropped, §3 row 10), checked against the unfortified skill. The level-to-skill mapping is Novice 0, Apprentice 25, Journeyman 50, Expert 75, Master 100 per UESP *Spell Making*, "Spell Cost", whose cost thresholds (below 26 none, 26 to 62 skill 25, 63 to 149 skill 50, 150 to 399 skill 75, 400 and up skill 100) it matches for custom spells; **Phase 0: verify** the mapping for authored spells against the executable.
- **PSC-CST-004.** The actual cost MUST be the sum over the spell's effects of the effect cost (§2) times `fMagicCasterSkillCostBase + fMagicCasterSkillCostMult × (1 − Skill ÷ 100)`, each effect against its own school's skill (UESP *Magic Overview*; *Spell Making*: "the skill adjustment is applied to each component effect separately"); a spell with the `Manual Spell Cost` flag uses its `SPIT` `Cost` as the base cost instead. The two constants MUST be read from `GameSettings` (`crates/scripting/src/obscript_state.rs`), not hard-coded. The skill MUST first take the Luck term (UESP *Spell Making*: Luck modifies the skill, and the result is held between 0 and 100); **Phase 0: verify** that the cast applies it and not only the displayed cost.
- **PSC-CST-005.** A cast that passes the gate MUST apply the actual cost as damage to the player's Magicka and 1 as damage to their Fatigue, through `ActorValues::apply_damage` (`crates/core/src/ecs/components/actor_values.rs:264`), the layer the HUD's current value reads (UESP *Magic Overview*: "Casting any spell costs one point of Fatigue"), then call the shared effect start (PSC-CST-007). A spell cannot fail once the gate passes (UESP *Magic Overview*, "Comparison to Morrowind").
- **PSC-CST-006.** A cast MUST NOT repeat before the casting interval has elapsed. The original's speed is fixed (UESP *Magic Overview*) and its value is **Phase 0: verify**; until it is measured the interval MUST be `MELEE_COOLDOWN_SECONDS` (0.45 s, `byroredux/src/combat.rs:42`), `cast.status` MUST label it provisional, and a refused repeat MUST report `cooldown` with the seconds left.
- **PSC-CST-007.** The player's cast MUST call `cast_spell` with the player as the caster and the resolved target (PSC-TGT-001..003), and MUST NOT duplicate its effect-start or `CAST` event code. The console's `magic.cast`, ObScript `Cast` and a perk's activate-choice cast MUST keep calling it unchanged and MUST neither charge nor feed (PSC-G-004). Today: those three callers exist (§3 row 2).
- **PSC-CST-008.** The Oblivion cast rules (the effect-cost equation, the skill adjustment and its Luck term, the mastery mapping, the one Fatigue point) MUST live in the character ruleset (`crates/core/src/character`, beside `skill_use.rs`) as functions of the rules profile, and the cast code MUST ask them rather than hold the constants (CLAUDE.md invariant 1: no per-game branch past the translation boundary).
- **PSC-CST-009.** One system MUST own the spend: it reads the readied spell and the cast edge, writes the Magicka and Fatigue damage and queues the skill use, and it MUST declare its component and resource access to the scheduler in the way `skill_use_system` does (CLAUDE.md invariants 3 and 4; [skill-use §4](../engine/skill-use.md#4-the-emitter-contract-osl-xp-007)). It MUST NOT lock the player's `ActorValues` from the input path.

### 5.3 Where a cast lands (`PSC-TGT`)

- **PSC-TGT-001.** An effect of range Self MUST land on the player. Today the range is decoded for Oblivion (`oblivion_effect_chain`, `crates/plugin/src/esm/records/misc/magic.rs`) and dropped before the runtime; the runtime's `CastEffect` MUST carry it.
- **PSC-TGT-002.** An effect of range Touch MUST land on the live actor the player's reach finds along the camera's forward, within `MELEE_REACH_BU` (180 BU, `byroredux/src/combat.rs:28`) until Phase 0 measures the original's; `cast.status` MUST label the reach provisional. What a Touch spell does to a door, a chest or a corpse, and to a Touch spell with the `Touch Spell Explodes w/ no Target` flag (xEdit TES4 `SPIT` flags, bit 7) when nothing is hit, are **Phase 0: verify**; until settled, such a cast lands on no one and feeds nothing.
- **PSC-TGT-003.** An effect of range Target MUST fly from the player along the camera's forward at the MGEF's projectile speed and land on the first live actor it meets, or on nothing at the end of its range, reusing the flying-projectile path the player's gun already uses (`ActiveProjectiles`, PSC-G-004's single path). Today only ammunition launches one. Gravity, the projectile model and the impact explosion are the projectile system's declared gaps and stay so (`systems/projectiles.rs` module docs).
- **PSC-TGT-004.** A multi-range spell MUST resolve each effect by its own range (UESP *Spell Making*: effects of one spell may differ in range), and a spell none of whose effects found a target MUST still pay its cost (PSC-CST-005) and report `no target`.
- **PSC-TGT-005.** An effect with an area greater than 0 SHOULD be reported as `area ignored` until area casting exists; it MUST NOT silently land on one actor as though the area were 0. **Phase 0: verify** the original's area rule.

### 5.4 Feeding the school (`PSC-USE`)

- **PSC-USE-001.** A cast that started at least one effect on a valid target MUST queue exactly one skill use for the spell's primary school, through `queue_skill_use`, with the school's use value from [skill-use §1](../engine/skill-use.md) at the usage index the other emitters read, and the same multiplier they use (the table and the index are that design's; Phase 0 reconciles its "Alteration 4.0 = value 2" note against the executable census before the emitter is written). Today: none of the six is fed.
- **PSC-USE-002.** A Self range MUST count as a valid target. A Touch or Target cast MUST count only when an effect landed on the actor it hit (PSC-TGT-002, -003, and `CastEffect::lands`); a cast that landed on nothing, or on a target no effect could affect, MUST feed nothing (UESP *Magic Overview*, "Skill levels and training").
- **PSC-USE-003.** The primary school MUST be the school of the effect with the highest effect cost (§2) among the spell's effects, and the cast feeds only it (UESP *Spell Making*). An effect's school is its MGEF's `magic_school`, or its `SCIT` school for a script effect (§2); a spell none of whose effects carries a school (MGEF school 6, None, and no `SCIT`) MUST feed nothing and report `no school`.
- **PSC-USE-004.** The six codes (20 to 25) MUST join `FED_SKILLS` in the same commit that adds the emitter, and not before (the OSL-USE-006 rule), so `levelup` stops listing them unfed only when a cast feeds them.
- **PSC-USE-005.** A power, a scroll, an ability, ObScript `Cast`, the console's `magic.cast` and a perk's activate-choice cast (`activate.rs:195`, which can run with the player as caster) MUST feed nothing (UESP *Magic Overview*: casting a power "does not contribute any experience points"; scrolls give none).

### 5.5 Observability (`PSC-DX`)

- **PSC-DX-001.** `cast.status` MUST report the readied spell, the last cast intent (`started` or the refusal reason), its actual cost, whether it fed a school and which, the cast interval and the touch reach in force (PSC-CST-006, PSC-TGT-002), each labelled provisional until Phase 0 measures it.
- **PSC-DX-002.** `spell.cast` MUST run the same gate and settle as the key (PSC-CST-001..007) with the player as caster, so a smoke script drives the real path; `magic.cast` MUST stay the uncharged, unfed script path.
- **PSC-DX-003.** Every refusal MUST log one line naming the spell and the failed gate, the same wording `cast.status` reports.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | The cast plays an animation whose timing sets the casting speed (UESP *Magic Overview*: blocking while casting shortens it) | A fixed interval, no animation (PSC-CST-006, PSC-NG-001) | none | A player cannot cast faster by blocking → PSC-CST-006 names the interval and `cast.status` reports it |
| 2 | A Target spell is a projectile with its own travel and impact | The weapon projectile path, a straight line, with its declared gaps (PSC-TGT-003) | none | Arcing and exploding spells miss their original behaviour → the gaps are stated in the projectile system and PSC-TGT-005 reports an ignored area |
| 3 | Worn armor lowers spell effectiveness (UESP *Magic Overview*) | Not applied in Wave 1 (PSC-NG-006) | none | Armored mages are stronger than the original's → Q3 decides when it lands |
| 4 | The magic menu readies a spell | A command and a cycle action (PSC-RDY-002, -003) | none | Not discoverable in play → the menu is the Game UI PRD's, PSC-NG-004 |
| 5 | Whether casting is heard by actors nearby is unsourced | Casting makes no noise (PSC-NG-007) | none | A stealth mage is quieter than the original may be → a Phase 0 verify item; if the answer is yes, a later revision adds the noise through the sneak design |
| 6 | The Touch reach and casting interval are the original's | Provisional engine constants (PSC-TGT-002, PSC-CST-006) | none | Touch spells reach and repeat differently from the original → both are labelled provisional in `cast.status` until Phase 0 measures them |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | Wave 1. The six magic skills level only by casting and the engine feeds none of them (§3 row 9). |
| Fallout 3 | no | No player spellcasting in the game's rules; **Phase 0: verify** against the GECK that no player-castable magic type exists. |
| Fallout: New Vegas | no | As Fallout 3. |
| Skyrim (LE / SE / AE) | decided in Q1 | Its cast action is a different design from Oblivion's: **Phase 0: verify**, from the Creation Kit wiki and UESP *Skyrim:Magic Overview*, that spells are cast per hand with concentration and dual casting, and, at HEAD, how the engine feeds Skyrim's magic skills today (the per-skill progress the Skyrim path saves), before a Wave 2 PRD. |
| Fallout 4 | no | **Phase 0: verify**, from the Fallout 4 CK wiki (`/mnt/data/src/reference/falloutck-uesp-wiki/`), that no player-castable spell type exists. |
| Fallout 76 | no | Online, out of the engine's single-player scope. |
| Starfield | no | **Phase 0: verify** from the Starfield data (xEdit SF1 definitions) that the player's magic is powers only, which PSC-NG-002 leaves out. |

### 7.2 Acceptance

Data: `Oblivion.esm`, read from the directory `crates/plugin/src/esm/test_paths.rs` resolves. No game file is checked in. A scenario passes when:

1. **Unit.** The gate refuses in the PSC-CST-002 order, each with its reason, and spends nothing (`cast_refuses_each_gate_in_order`); the actual cost at skill 5 and skill 100 equals the effect-cost equation times `1.4 − 0.012 × Skill` for two named `Oblivion.esm` spells, one Self and one Target, and for one `Manual Spell Cost` spell (`cast_cost_follows_the_equation`); a spell's required mastery is its `SPIT` `Level` (`cast_mastery_is_the_authored_level`); a multi-effect spell's primary school is its highest-cost effect's and a script effect's is its `SCIT` school (`primary_school_is_the_highest_cost_effect`). Phase 0's census names the three spells by EditorID in the design, and the tests take them from there.
2. **Unit, the feed.** A Self cast feeds its school once; a Touch cast on an unaffectable target feeds nothing; a script's `Cast`, the console's `magic.cast` and a power feed nothing (`player_cast_feeds_only_a_valid_target`, `other_casters_feed_nothing`: the console, ObScript `Cast`, a perk's activate choice and a power).
3. **Live, new script `oblivion-spellcasting.sh`** (indexed in [docs/smoke-tests/README.md](../smoke-tests/README.md)): Oblivion in the cell and with the target actor and Destruction spell that Phase 0's census names (the script header records them, as `oblivion-level-up.sh` records its cell), `spell.ready` on the spell, `spell.cast` at the actor, and `levelup` shows Destruction's banked experience rise by the use value of skill-use §1 while `levelup` no longer lists the school as unfed. The negative leg (the same cast on a chest raises nothing) runs once PSC-TGT-002's object rule is settled; until then it asserts the provisional rule (nothing lands, nothing feeds).
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

**Phase 0 — Decode and verify (no engine code).** Decode the original executable's own spell-cast site, the one the skill-use work left open (`0x5fca20` is the item-use path, not this); write the "Player cast" section of `docs/engine/scripting.md` and the cast-rules section of the Oblivion ruleset design (PSC-CST-008); name the two spells, the cell and the target actor of §7.2 by EditorID; resolve §10. Close the **Phase 0: verify** items:

| Item | Requirement |
|---|---|
| The executable's spell-cast site and what it feeds | PSC-USE-001, -002 |
| The `SPIT` `Level` to skill mapping on authored spells | PSC-CST-003 |
| `Manual Spell Cost` honoured as in §2; Area and magnitude rounding in the effect cost | PSC-CST-004 |
| Refusal, or a cast anyway, on short Magicka | PSC-CST-002 |
| Primary school rule on authored spells; script-effect school | PSC-USE-003 |
| The skill-use value index ("Alteration 4.0 = value 2") | PSC-USE-001 |
| Whether a cast is heard by the detection tick | PSC-NG-007 |
| Luck term applies to the cast, not only the display | PSC-CST-004 |
| Casting interval | PSC-CST-006 |
| Touch reach; what Touch does to a door, a chest, a corpse, and a `Touch Spell Explodes w/ no Target` spell with no target | PSC-TGT-002 |
| Area rule | PSC-TGT-005 |
| Armor effectiveness formula (Q3) | PSC-NG-006 |
| No player-castable magic type on Fallout 3, New Vegas, Fallout 4; Starfield powers only; Skyrim's hand model and skill feed | §7.1 |

Exit: each item closed with a source or filed as an issue, and the Oblivion cast site recorded in the design.

**Phase 1 — Ready, gate, cost, Self.** PSC-RDY, PSC-CST, PSC-TGT-001, PSC-DX. Carry range, effect cost inputs, the `SPIT` `Level`, `Cost` and `Manual Spell Cost` flag, and the school (MGEF or `SCIT`) into `CastEffect` / `CanonicalSpell`. Exit: scenario 1 passes and a Self cast spends Magicka and Fatigue on `Oblivion.esm`.

**Phase 2 — Touch and Target.** PSC-TGT-002 to -005. Exit: the Touch and Target legs of scenario 3 land on a live actor and a missed Target cast pays its cost.

**Phase 3 — Feed the schools.** PSC-USE. Exit: scenarios 2, 3 and 4 pass; the six codes are in `FED_SKILLS`; the ROADMAP matrix row shows the metrics of §8.

---

## 10. Decisions required

1. **Q1 — Which games' casting is in this PRD.** Recommended: Oblivion only (Wave 1). Skyrim's per-hand casting and Fallout's absence of it are separate designs; folding them in doubles the Phase 0 verify work for a feed that Skyrim does not need. Alternative: Oblivion and Skyrim together, which makes this a two-dialect PRD and moves Skyrim's hand model into Wave 1.
2. **Q2 — The default key.** Recommended: the owner names the `CastSpell` key from the keys Phase 0 lists as unbound, with `NextSpell` / `PreviousSpell` on neighbouring keys. Alternative: the `CastSpell` key only, with `spell.ready` as the sole way to choose, which leaves a mage unable to switch spells without the console until the menu lands. (The `spell.ready` command and the cycle action are not in question: PSC-RDY-002 and -003 settle them.)
3. **Q3 — Spell effectiveness from armor.** Recommended: out of Wave 1; a later requirement once Phase 0 sources the armor-skill formula. Alternative: in Wave 1, which delays the cast action behind an unsourced formula.
4. **Q4 — How a Target spell flies.** Recommended: the weapon projectile path (`ActiveProjectiles`) at the MGEF's speed, a straight line. Alternative: an instant ray cast first, with no visible flight, and the projectile as a later phase; the skill use is identical either way.
5. **Q5 — The saved ready spell.** Recommended: save it, with the save format bump that needs (the `FORMAT_MAJOR` rule, README T2). Alternative: do not save it, so a loaded game starts with nothing readied, no bump, and a mage re-readies after every load.

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
| 2 | 2026-11-19 | Review round 1 folded in: cast callers (console, ObScript `Cast`, perk activate choice), spell cost equation and `SPIT` `Level` / `Manual Spell Cost`, script-effect school, ownership of the Oblivion cast rules and scheduler access (PSC-CST-008, -009), measurable interval and reach, §7.1 citations, scenario data, phase dependencies, term fixes (range, mastery, castable spell), detection noise (PSC-NG-007), Q2 and Q4 wording | claude: revise |
