# Oblivion skill-use leveling — Product Requirements

**Status**: SHIPPED (2026-11-23). Revision 20 re-sources the executable-read rules (#7331). Revision 19 records the owner's #7310 decisions (`AdvancePCSkill` clears the banked experience; the armor-hit roll is a declared engine choice). Revision 18 ships it: every phase exit is met, the metrics are in the ROADMAP Oblivion row, and every tracking issue is closed; the history is §12.

**Tracking**: #7047 (parent). Phases: #7049 (0), #7050 (1), #7051 (2), #7052 (3). Menu request: #7048, built in #7086. Follow-ups, all closed: #7044 (with Phase 3), #7053 (the player cast action, six schools), #7054 (the block guard, Block), #7055 (the bow shot path, Marksman), #7056 (fall damage, Acrobatics' fall).

**Scope**: the player of Oblivion levels the way the original's player does: using a skill banks experience, enough experience raises the skill, ten raises of major skills make a level pending, a night's sleep applies it, the player raises three attributes by the bonuses their skill use earned, and Health grows by a tenth of Endurance. Everything the player does that the original counts as skill use feeds it, where the engine has the system that action belongs to. It covers the player only, on Oblivion only (every other title has a leveling model already; §7.1). Outside it: character creation (class, birthsign, race choice), the original's trainers and skill books, creature and NPC level scaling, and the menu surfaces owned by the Game UI Integration PRD (§4.2).

**Related**: this document states the *promise*; the designs below own the *how*. Every mechanism promised here has an owner in this table, or Phase 0 writes the missing design.

| Mechanism | Owning design |
|---|---|
| The leveling vocabulary, `LevelingModel`, the per-game ruleset and the derived-stat rows | [CHARAL](../engine/charal.md) |
| Oblivion's formulas, skill roster and attribute governance | [CHARAL — Oblivion Ruleset](../engine/charal-oblivion-ruleset.md) |
| The player's actor values, their layers and the engine-supplied `AVIF` records the skills live in | [Oblivion Actor Values — PRD](oblivion-actor-values.md) |
| What persists, and how a save of a new column stays loadable | [Save/load round trip](../engine/save-load-roundtrip.md) |
| ObScript commands and the console | [Scripting](../engine/scripting.md) |
| The skill-use hooks: the contract between the combat, magic, movement and detection systems and the experience entry point | [Skill use](../engine/skill-use.md) (written in Phase 0, #7049) |
| Undetected sneaking, the condition the Sneak skill's action needs | [Sneak and Detection — PRD](sneak-and-detection.md) |
| The level-up and sleep/rest menu surfaces | [Game UI Integration — PRD](game-ui-integration.md#59-menuxml-menus-beyond-the-hud-gui-xml), GUI-XML-004…007 (#7048) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119 meanings, as in the related designs. Requirement IDs use the `OSL-` prefix.

---

## 1. Product thesis

Oblivion's progression is its identity: the player is never handed experience points. They become better at what they do, the world scales to the level that accumulates behind it, and the cost of a careless level is a character weaker than the world around them. UESP's *Oblivion:Leveling* states the model: a character qualifies for a level "after improving any combination of major Skills by a total of 10 points", must "rest on a bed" for it to take effect, then raises three attributes, and Health "increases by 10% of your Endurance" ([UESP: Oblivion Leveling](https://en.uesp.net/wiki/Oblivion:Leveling)). The *Increasing Skills* page states how a skill rises: by experience that actions grant, at a cost that grows with the skill and shrinks for specialization and major skills ([UESP: Oblivion Increasing Skills](https://en.uesp.net/wiki/Oblivion:Increasing_Skills)).

When this PRD was drafted (2026-11-17), none of this happened. The player of Oblivion started at level 1 and stayed there: `LevelingModel::OBLIVION` was a skill-use model with no experience curve, nothing banked a skill's use, and the two helpers that hold Oblivion's level-up arithmetic had no caller (revision 16's §3). Every consumer of the level — `GetLevel` conditions, the level-scaled actors a cell spawns, and the level requirement every Daedric quest carries (UESP *Leveling*) — read a level nothing raised. The follow-up #7044 showed the same gap from the actor-value side: there was no per-level Health gain to keep apart from the refreshed base, because there was no level.

It is for players who want Oblivion's character to grow the way Oblivion's does, and for content: leveled lists, level-gated quests and level-offset creatures all assume a player who levels.

The corollary that shapes every requirement: **the level is earned by skill use alone, through the original's own rules.** The engine adds no character experience points for Oblivion, no level a command can give for free beyond the commands the original ships (`AdvancePCSkill`, `AdvancePCLevel`), and no per-game branch past the ruleset: the settings are the load order's, and the rules are `LevelingModel::SkillUse` data.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Skill use** | One counted action: a weapon hit, a spell cast, a hit taken, a lock tumbler fixed. Each skill's actions and the experience one action grants are UESP's table ([Increasing Skills](https://en.uesp.net/wiki/Oblivion:Increasing_Skills#Acquiring_Experience_Points)) |
| **Experience** | The points a skill has banked toward its next rank. Not visible in the original's UI; the console `tdt` + `sdt 10` display shows it (same page) |
| **Skill class** | Which scaling a skill takes: *base* (neither), *specialization* (one of the seven skills of the class's Combat, Magic or Stealth specialization), *major* (one of the class's seven), or both |
| **Rank-up** | The skill value rising by one because banked experience reached the cost of the next |
| **Major increase** | A rank-up of a major skill; ten of them make a level pending |
| **Attribute tally** | Per attribute, the rank-ups of the skills it governs since the last level-up, major and minor ("how many skill points you have gained in skills governed by this attribute (major *and* minor)", UESP *Leveling*) |
| **Pending level** | A level the tenth major increase made available and a sleep has not applied; it carries the attribute tallies frozen at that moment, and several can be pending at once |
| **Level-up** | Applying a pending level: the level rises by one, three attributes rise by their tally bonuses, Health gains a tenth of Endurance, the tallies reset |
| **Unfed skill** | An Oblivion skill whose original action has no engine system to count it; `levelup` names it and §8 counts it |
| **Stored Health term** | The part of the player's Health that accumulates level by level and is not a formula of the attributes (§5.5) |
| **Ruleset** | CHARAL's per-game data, `CharacterRuleset`, already the home of Oblivion's derived stats ([CHARAL](../engine/charal.md)) |

---

## 3. Core model

A skill and its levels move through four states, each reported by the console `levelup` (§5.7):

| State | Meaning | Reported by |
|---|---|---|
| **Banked** | The skill holds experience short of the next rank's cost | `levelup` (per skill: experience and cost) |
| **Raised** | A rank-up happened; the skill value is one higher, the surplus experience rolls over | `levelup`, the actor value itself |
| **Pending** | Ten major increases are banked: a pending level, with its frozen tallies | `levelup` (the pending levels) |
| **Applied** | A sleep ended with a pending level and the player chose their attributes | `levelup`, `GetLevel`, the saved `CharacterLevel` |

The inventory below is the state at HEAD on 2026-11-22, after Phases 0 to 3 and the follow-ups #7053 to #7056 and #7086. The state before the work, on 2026-11-18, is revision 16's §3. Each row's proof names the symbol that shows it.

| # | Piece | At HEAD | Proof |
|---|---|---|---|
| 1 | Oblivion's leveling model | `LevelingModel::OBLIVION` is `SkillUse` with its rank costs (`SkillUseCosts`); `with_gmst` overlays the five `fSkillUse*` settings (OSL-XP-006) | `crates/core/src/character/leveling.rs` (`OBLIVION`, `with_gmst`) |
| 2 | Attribute bonus table | `oblivion_attribute_bonus` prices each pick from the level's frozen tally | `crates/core/src/character/tes.rs`; `pick_attribute` in `crates/scripting/src/oblivion_skill_use.rs` |
| 3 | Per-level Health gain | `apply_oblivion_level_up_health` adds the gain to the Health actor value's `stored` layer, which `undamaged()` includes (Q1, #7044) | `crates/core/src/character/tes.rs`; `crates/core/src/ecs/components/actor_values.rs` |
| 4 | Skill roster and governance | `SkillSet::OBLIVION` carries the 21 skills and their governing attributes; the engine-supplied `AVIF` records give each skill an actor value | `crates/core/src/character/skill.rs`; [Oblivion Actor Values — PRD](oblivion-actor-values.md) |
| 5 | The player's class | `player_class` reads the `Player` record's class (`SkillUseClasses`) at each use | `crates/scripting/src/oblivion_skill_use.rs` |
| 6 | Skill experience | Emitters call `queue_skill_use`, which appends to `PendingSkillUse`; `skill_use_system` drains it through `bank_skill_use`. The banked experience is `SkillProgress`, the rank-ups, tallies and pending levels `SkillUseLevel` | `crates/scripting/src/oblivion_skill_use.rs`; `byroredux/src/combat.rs`; `crates/core/src/character/components.rs` |
| 7 | Who feeds skills | `on_landed_hit`, `stance_use_system`, `on_jump` and `on_fall` (`byroredux/src/skill_use_emit.rs`) and the player cast's `feed_school` (`crates/scripting/src/player_cast.rs`): the 16 skills of `FED_SKILLS`; [skill-use.md](../engine/skill-use.md) §3 names the five unfed and why | `crates/scripting/src/oblivion_skill_use.rs` (`FED_SKILLS`) |
| 8 | Sleep | `sleep_system` reports a completed bed sleep, `SetPCSleepHours` and jail rest as `SleepCompleted`; an ordinary bed opens the rest session, which offers the oldest pending level to the level-up menu (#7086) | `byroredux/src/systems/script_requests.rs`; `byroredux/src/oblivion_rest.rs` |
| 9 | Level writers | `apply_pending_level` and `apply_level_menu_picks` take a level; `advance_level` (`AdvancePCLevel`) makes one pending; `SetLevel` writes the level and no stat; `levelup` reports Oblivion's class, skills, tallies and pending levels and takes `levelup attr` | `crates/scripting/src/oblivion_skill_use.rs`; `byroredux/src/commands/gameplay.rs` |
| 10 | Level readers | `GetLevel` reads `CharacterLevel`; actor spawn reads the player's level through `player_level_for_spawn`; a container's contents resolve against it | `crates/scripting/src/condition/eval/actor.rs` (`get_level`); `byroredux/src/npc_spawn/stamps.rs`; `byroredux/src/cell_loader/references/attach.rs` |
| 11 | Persistence | `SkillUseLevel` with its pending levels, `SkillProgress` and the Health `stored` layer are saved (`FORMAT_MAJOR` 141 at HEAD) | `byroredux/src/save_io/registry.rs`; `crates/save/src/snapshot.rs` |
| 12 | ObScript commands | `AdvancePCSkill` and `ModPCSkill` (`raise_skill_ranks`), `AdvancePCLevel` (`advance_level`) and Oblivion's `SetPCSleepHours` have handlers | `crates/scripting/src/oblivion_skill_use.rs`; `crates/scripting/src/obscript_more/state.rs` |

---

## 4. Goals and non-goals

### 4.1 Goals

- **OSL-G-001 — A skill rises by use.** Using an Oblivion skill banks experience at the original's rate and raises the skill at the original's cost, for every skill whose action the engine has a system for (§5.2).
- **OSL-G-002 — A level is earned and applied.** Ten major increases and a sleep make the player level 2, with three attributes raised by the bonuses their skill use earned.
- **OSL-G-003 — Health grows with the level and survives the refresh.** The per-level Health gain persists across `player_derived_stats_system`, a save and a load, and an added and removed Fortify Health ability (#7044).
- **OSL-G-004 — Nothing is faked.** Every skill whose original action has no engine system is reported unfed, never approximated (§5.2, §8).

### 4.2 Non-goals

- **OSL-NG-001.** Character creation: the class, specialization, birthsign and race choices (the Oblivion Actor Values PRD's Q3 takes the player's values from the `Player` `NPC_` record). A chargen flow needs its own PRD; §5.6 fixes the interim class.
- **OSL-NG-002.** Trainers and skill books: they grant experience "equal to the amount necessary for a full skill level" ([UESP: Increasing Skills](https://en.uesp.net/wiki/Oblivion:Increasing_Skills#Acquiring_Experience_Points)) and depend on dialogue and inventory flows this PRD does not touch. The experience entry point (OSL-XP-001) is what they will call.
- **OSL-NG-003.** Level scaling of NPCs and creatures, and the world's re-leveling on a cell change: owned by the Oblivion Actor Values PRD's population work (#7022, #7040). This PRD only changes the level they read.
- **OSL-NG-004.** The visual level-up and sleep/rest menus are owned by the [Game UI Integration PRD](game-ui-integration.md#59-menuxml-menus-beyond-the-hud-gui-xml). #7086 binds the leveling rules to that contract. The console surface remains available (§5.7, Q3).
- **OSL-NG-005.** Oblivion Remastered's formulas. The target is the 2006 game, whose Health rule is the one in the local dump of [UESP: Oblivion Health](https://en.uesp.net/wiki/Oblivion:Health) (`/mnt/data/src/reference/uesp-wiki/Oblivion/Health.wiki`: base `Endurance × 2`, a tenth of Endurance per level, rounded down; `crates/core/src/character/tes.rs:209–212` already scopes the code to it). The live page's second formula is Oblivion Remastered's, under its own heading (V15); every citation of that page in this PRD is of the local dump, not of the live page.
- **OSL-NG-006.** Shivering Isles and the other expansions' level rules (the Felldew addiction that lowers base attributes, the Oghma Infinium's free skill boosts): they are content scripts that use the commands of §5.7.

---

## 5. Requirements by area

### 5.1 Skill experience (`OSL-XP`)

- **OSL-XP-001.** One entry point MUST bank `uses` of an action against an Oblivion skill and return the ranks it reached, in order; skill use, `AdvancePCSkill`-style script calls and later trainers MUST all go through it.
- **OSL-XP-002.** The experience a skill needs to rise from value `L` MUST be `(fSkillUseFactor × L) ^ fSkillUseExp`, multiplied by `fSkillUseSpecMult` for a specialization skill and by `fSkillUseMajorMult` for a major skill or `fSkillUseMinorMult` otherwise (the major multiplier replaces the minor one: UESP *Increasing Skills* gives Base to Major as `FSkillUseMajorMult` / `FSkillUseMinor`), with the settings read from the load order's `GMST` records and these defaults when absent: Minor 1.25, Major 0.75, Spec 0.75, Factor 1 and Exp 1 (executable-derived and declared, §9), of which `Oblivion.esm` authors Factor 0.35 and Exp 1.5 ([CS: FSkillUseExp](https://cs.uesp.net/wiki/FSkillUseExp), [UESP: Increasing Skills](https://en.uesp.net/wiki/Oblivion:Increasing_Skills#Experience_Points_Needed)). The setting is `fSkillUseMinorMult`; `fSkillUseMinor` is UESP's name for it (V1, answered in #7049).
- **OSL-XP-003.** Experience beyond the cost MUST roll over to the next rank, and a skill MUST NOT exceed 100 by use ("Any extra experience points will roll over", UESP *Increasing Skills*; the cap is the skill maximum). A skill whose current value, Fortify included, is 100 or more banks nothing (V2, executable-derived and declared; reading the current value is the owner's decision of 2026-11-22, #7210).
- **OSL-XP-004.** A skill's class (base, specialization, major, both) MUST come from the player's class record: `CLAS` specialization and seven major skills.
- **OSL-XP-005.** The experience a banked skill holds MUST persist across a save and a load (§5.8).
- **OSL-XP-006.** The rules MUST be `LevelingModel::SkillUse` data overlaid with the load order's settings, as `SkillXp` overlays Skyrim's (`with_gmst`), with no `if Oblivion` branch in the system that applies them.

- **OSL-XP-007.** Skill-use emitters in the combat, magic, movement and detection systems MUST append to one pending resource that a single system drains into the entry point, and MUST NOT lock the player's `ActorValues` themselves, with the drain's scheduler access declared (CLAUDE.md invariants 3 and 4; precedent `PendingSkillAdvances`, `byroredux/src/boot/schedule/late/scripted_combat.rs:28`).

### 5.2 Skill use (`OSL-USE`)

- **OSL-USE-001.** One use of an action the engine feeds MUST grant the experience of that action's row in UESP's table ([Increasing Skills](https://en.uesp.net/wiki/Oblivion:Increasing_Skills#Acquiring_Experience_Points)); the table is held as data, and a test iterates every fed row of every skill the ruleset names. UESP's numbers are the two use values of each skill's `SKIL` record, which `Oblivion.esm` authors (V3).
- **OSL-USE-002.** A weapon hit MUST feed the skill of the weapon (Blade, Blunt, Hand to Hand, Marksman) only when it lands on a living target ("weapon blows must land on a creature or NPC, not an inanimate object", UESP). A landed melee blow feeds Blade, Blunt or Hand to Hand, and the player's own arrow landing feeds Marksman (`on_landed_hit`, `byroredux/src/skill_use_emit.rs`; #7055, [Player archery PRD](player-archery.md)).
- **OSL-USE-003.** A hit the player takes MUST feed Heavy Armor or Light Armor by what the armor piece struck is, and a blocked hit MUST feed Block. One piece per hit takes the experience, Light or Heavy Armor by that piece ([skill-use.md](../engine/skill-use.md) §2 row 9, executable-derived and declared). A hit that meets the player's guard feeds Block (#7054).
- **OSL-USE-004.** A spell the player casts with a valid target MUST feed the spell's school (Alteration, Conjuration, Destruction, Illusion, Mysticism, Restoration); a spell cast on self counts as a valid target ("spells must be cast upon yourself, a creature, or an NPC", UESP). The player's cast feeds its primary school (`feed_school`, `crates/scripting/src/player_cast.rs`; [Player spellcasting PRD](player-spellcasting.md), #7053); the console's `magic.cast` and a script's `Cast` feed nothing (OSL-G-004).
- **OSL-USE-005.** The time-based skills (Athletics while running or swimming, Sneak while moving undetected in sneak mode) MUST bank experience per second of the stance, with the undetected condition read from the detection level ([Sneak and Detection PRD](sneak-and-detection.md)). Acrobatics ("each jump or dodge", UESP *Increasing Skills*) feeds per jump and per damaging fall (#7056); the pickpocket action waits on a pickpocket system (OSL-USE-006). V4's census ([skill-use.md](../engine/skill-use.md) §2, §3) names each trigger and whether it is fed.
- **OSL-USE-006.** A skill whose action belongs to a system the engine does not have (Security's lock minigame, Speechcraft's persuasion minigame, Mercantile's barter) MUST be reported unfed by `levelup` and counted in §8, never fed by a stand-in action (OSL-G-004). The fed set is `FED_SKILLS` (`crates/scripting/src/oblivion_skill_use.rs`); `levelup` lists the rest as unfed, and §8's metrics count both.
- **OSL-USE-007.** Alchemy and Armorer MAY be fed once the engine has potion-making and repair actions; until then they follow OSL-USE-006.

### 5.3 Rank-up and the level count (`OSL-LVL`)

- **OSL-LVL-001.** A rank-up MUST raise the skill's base actor value by 1, and MUST NOT go through the permanent-modifier layer, which carries abilities and gear (`magic::apply_modifiers`). The original raises the base skill, not a separate stored value (V5, executable-derived and declared).
- **OSL-LVL-002.** A rank-up of a major skill MUST count toward the level, and a rank-up of any skill MUST add to the tally of the attribute that governs it ("Even though minor skills do not count towards your level up count, they still contribute to attribute bonuses", UESP *Increasing Skills*).
- **OSL-LVL-003.** When the major increases since the last level-up reach `major_skill_ups_per_level`, a level MUST become pending, and the attribute tallies at that moment MUST be the ones the level-up offers: "Your attribute bonuses are set when you gain your 10th major skill point" and rank-ups after it start the next level's tallies (UESP *Leveling*, Details). The rank-up that makes the tenth major increase is in the frozen tally (V13): UESP *Leveling*'s Example 2 reaches +5 only if it counts, though its text says "before gaining the 10th increase in a major skill"; the executable-derived answer agrees (declared).
- **OSL-LVL-004.** A level MUST NOT apply except when the player sleeps to the end of a sleep without an interruption: waiting does not apply it ("Waiting will not trigger level-up", UESP *Sleeping and Eating*). Which entry points count as sleeping in the engine is §5.4's.
- **OSL-LVL-005.** Several levels MAY be pending at once after consecutive sleeps ("It is possible to sleep several times in succession with multiple level-ups", UESP *Leveling*); each MUST use the tallies saved with it.
- **OSL-LVL-006.** The level MUST NOT rise when every attribute is at its maximum of 100: the original answers "Perfection cannot be improved upon" instead (UESP *Leveling*, Notes).

### 5.4 Sleep and the level-up trigger (`OSL-SLP`)

- **OSL-SLP-001.** A completed sleep MUST be one event the level-up system reads, produced by both the script path (`SetPCSleepHours`, `sleep_system`) and a confirmed bed sleep. The bed flow follows GUI-XML-006. The scripted and interactive producers both use `finish_sleep` (#7086).
- **OSL-SLP-002.** A sleep interrupted by an enemy encounter MUST NOT apply the level ([UESP: Sleeping and Eating](https://en.uesp.net/wiki/Oblivion:Sleeping_and_Eating)). The interactive runner rechecks combat before advancing the clock, and the level-up consumer ignores interrupted events ([Skill Use § Levels](../engine/skill-use.md#5-levels-7052), V6).

### 5.5 Attributes and Health at level-up (`OSL-ATT`, `OSL-HP`)

- **OSL-ATT-001.** A level-up MUST offer three attributes, each with the bonus `oblivion_attribute_bonus` returns for its tally, and apply the three the player chose by raising their base values, to at most 100 ("It is never possible to gain more than a +5 attribute bonus", UESP *Leveling*; the cap is the attribute maximum). Attribute tallies that are not spent MUST NOT roll over.
- **OSL-ATT-002.** The raised attributes MUST flow to every derived stat through the existing refresh: after a level-up, Health, Magicka and Fatigue read the values their formulas give at the new attributes (`player_derived_stats_system`), pinned by a test that raises Endurance, Intelligence and Strength.
- **OSL-HP-001.** A level-up MUST add `floor(0.1 × Endurance)` to the player's stored Health term, rounded down: "you gain health points equal to a tenth of your endurance, rounded down", with the example of Endurance 98 giving +9 ([UESP: Oblivion Health](https://en.uesp.net/wiki/Oblivion:Health)). The original truncates the stored sum, which for an integer term equals UESP's rounded-down gain (V7). The Endurance read is the value after this level's attribute picks (V8, executable-derived and declared).
- **OSL-HP-002.** The stored Health term MUST be added to the derived `2 × Endurance` when the maximum is read, so it survives `player_derived_stats_system`, and MUST be independent of the permanent-modifier layer, so that adding and removing a Fortify Health ability leaves it intact (#7044; the layer's shape is Q1).
- **OSL-HP-003.** A level-up MUST NOT change the stored terms of Magicka or Fatigue: the original's level-up writes only stored Health, and its load path zeroes the `Player` record's three stored pools (V9, executable-derived and declared).

### 5.6 The player's class (`OSL-CLS`)

- **OSL-CLS-001.** Until a character-creation flow exists, the player's class MUST be the class of the `Player` `NPC_` record, so its specialization and seven major skills select the skill classes of OSL-XP-004; `levelup` MUST name the class it read. The vanilla `Player` record's class is `CharactergenClass` (`CLAS` 000230E6): Combat, with Armorer, Athletics, Blade, Block, Blunt, Hand to Hand and Heavy Armor major (V10, `Oblivion.esm`).
- **OSL-CLS-002.** The player's class MUST be read at each skill use, not at boot only: a test edits the class record's major skills between two uses of one skill and sees the second cost change.
- **OSL-CLS-003.** A mod or script that sets the player's class MAY do so through the same record data.

### 5.7 Surfaces (`OSL-API`)

- **OSL-API-001.** ObScript `AdvancePCSkill <skill> <amount>` MUST advance the skill "as if the player earned the skill by use or training" ([CS: AdvancePCSkill](https://cs.uesp.net/wiki/AdvancePCSkill)): through OSL-XP-001, so the rank-ups count toward the level and the attribute tally. `Amount` is skill ranks: the one vanilla caller, `ModPCSkill Blade 2`, tells the player "Your Blade and Block skills have both been raised by 2 points" (V14). No vanilla script calls `AdvancePCSkill`; one script calls `ModPCSkill` twice (V11). Like `ModPCSkill`, it MUST set the skill's banked experience to 0.0: the CS page says it "is subject to the same issue as ModPCSkill", and Q5 chose the patched behaviour (owner decision 2026-11-23, #7310).
- **OSL-API-006.** ObScript `ModPCSkill <skill> <amount>` MUST raise the skill by `amount` ranks, count the raise toward the level "just like normal 'earned' skill increases", and set the skill's banked experience to 0.0 (Q5) ([CS: ModPCSkill](https://cs.uesp.net/wiki/ModPCSkill), whose Issues section documents two behaviours: the usage count falls by the old usage limit, and, from Beta Patch 1.1, the usage is set to 0.0). `amount` is ranks (V14).
- **OSL-API-002.** ObScript `AdvancePCLevel` MUST make a level pending for the player's next sleep or open the level-up surface, as the CS states ("raises the player's level by bringing up the 'level up' menu"); `SetLevel` on the player keeps changing the level and no stat ([CS: SetLevel](https://cs.uesp.net/wiki/SetLevel)).
- **OSL-API-003.** The console `levelup` MUST report, for Oblivion, the class and its major skills, per skill the experience banked and its cost, the major increases since the last level-up, each attribute's tally and bonus, and the pending levels; each field reports the state that exists in the phase that builds it (§9).
- **OSL-API-004.** Applying a level MUST push one line naming the new level to the notification queue (`notifications::push`; the original shows its level-up message, UESP *Leveling*, Messages), with the text and its sources that [skill-use.md](../engine/skill-use.md) §5 records (V12; the default text of `sMeditate` is executable-derived and declared).
- **OSL-API-005.** The console `levelup attr <attribute>` MUST spend one of a pending level's three picks (Q3).

### 5.8 Persistence (`OSL-SAVE`)

- **OSL-SAVE-001.** Everything a level needs between a rank-up and its application MUST be saved: the experience per skill, the major-increase count, the per-attribute tallies, the pending levels and their frozen tallies, and the stored Health term. Each new column MUST be a component with a declared storage (CLAUDE.md invariant 2). A new column or a changed shape MUST bump `FORMAT_MAJOR` and the schema fingerprint in the same commit (`the_bump_ledger_is_contiguous_and_current`, `the_schema_fingerprint_is_pinned`).
- **OSL-SAVE-002.** A save written before a shape change of this feature MUST be refused cleanly by the `FORMAT_MAJOR` bump, never loaded with silently invented state: the container rejects a major it does not support, and a new field is required rather than `serde(default)` (SAVE-D2-01, `crates/save/src/snapshot.rs`; #1714). The stored Health layer is the first such change (#7044, v136); a column added later with no shape change to an existing type needs no bump but still defaults to "no progress".
- **OSL-SAVE-003.** Loading a save made mid-level MUST resume it: the banked experience, the tallies and the stored term read back identical, and a re-captured snapshot of the loaded state decodes equal to the one loaded.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | What counts as skill use, and where, is the executable's (the call sites are [skill-use.md](../engine/skill-use.md) §2's census, executable-derived and declared) | Skill use is fed by the engine's systems as the matching action exists (OSL-USE-001 to 007); a skill with no system is unfed | none | A player of a build with few emitters levels slower than the original: reported by §8's coverage metric, never papered over (OSL-USE-006) |
| 2 | The level-up menu raises the three attributes | An ordinary bed sleep opens the level-up menu (#7086); a scripted or jail sleep applies the level with the console's `levelup attr` picks (Q3) | none | A script-driven sleep takes its picks from the console, not the menu; the notification of OSL-API-004 says a level is waiting |
| 3 | The class is chosen by the player in character creation | The class of the `Player` record, until a chargen PRD (OSL-CLS-001) | none | A vanilla player never picks a custom class, so which skills are major differs from the original's first minutes; OSL-CLS-001 names the class `levelup` read |
| 4 | Health rises by a tenth of Endurance at each level-up ([UESP: Oblivion Health](https://en.uesp.net/wiki/Oblivion:Health)); how the executable stores it is V7 to V9's answer (executable-derived and declared) | The stored term is a saved layer the maximum reads (OSL-HP-002, Q1) | the per-stat "retain" mechanism #7041 removed | The layer's shape is a save-format change (OSL-SAVE-001) |
| 5 | "The world will not 'level up' with the player until he goes through a loading door, or changes cells" ([CS: SetLevel](https://cs.uesp.net/wiki/SetLevel)) | Whatever the existing consumers do on a cell load (OSL-NG-003) | none | Content that expects the world to follow the level instantly sees it a cell later; owned by #7022 / #7040 |
| 6 | A completed sleep applies a pending level and waiting does not ([UESP: Sleeping and Eating](https://en.uesp.net/wiki/Oblivion:Sleeping_and_Eating)); no documented source says whether a script's `SetPCSleepHours` counts as one | A completed script-driven sleep applies a pending level (OSL-SLP-001) | none | A quest-driven sleep might level a player the original would not; declared, and V6's executable-derived answer is that a completed sleep applies it |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | The only title whose model is `SkillUse`. One wave |
| Fallout 3 | n/a | `XpCurve`: levels come from experience points ([CHARAL](../engine/charal.md)); built |
| Fallout: New Vegas | n/a | Same model as Fallout 3 |
| Skyrim (LE / SE / AE) | n/a | `SkillXp`: skill rank-ups feed character experience through the path this PRD reuses for its entry point (`crates/scripting/src/skills.rs`); built |
| Fallout 4 | n/a | `XpCurve` with a perk per level; built |
| Fallout 76 | n/a | No playable flow starts a character; no leveling model is in scope |
| Starfield | n/a | Its own model ([CHARAL — Starfield Ruleset](../engine/charal-starfield-ruleset.md)), not skill use |

### 7.2 Acceptance

Game data comes from the local Oblivion install only (`BYROREDUX_OBLIVION_DATA`); nothing is checked in.

A scenario passes when:

1. **The cost table (`cargo test`, no data).** `cargo test -p byroredux-core` pins the experience cost of the base, specialization, major and both classes at skill values 5, 25, 50, 75 and 99 against UESP's table, including the page's worked example (a base skill at 5 needs 2.89).
2. **The settings (ignored real-data test).** With `Oblivion.esm`, the five `fSkillUse*` settings resolve to their defaults and the cost at skill 5 is 2.89.
3. **Banking and rank-up (headless, `docs/smoke-tests/oblivion-level-up.sh`).** In Anvil's Count's Arms, `ModPCSkill Blade 2` raises Blade by two ranks (V14), counts them toward the level and tallies Strength; `AdvancePCSkill Sneak 1` tallies Agility and, Sneak being no major skill of the class, not the level; `levelup` reports the banked experience.
4. **Level-up (same script).** Ten major increases make `levelup` report a pending level; `SetPCSleepHours 8` applies it after `levelup attr` picks three attributes; `GetLevel` reads 2, the picked attributes rose by their bonuses, and Health reads `2 × Endurance + floor(0.1 × Endurance)`.
5. **Health survives (same script and a unit test).** After `ModAV Endurance 5`, after adding and removing a Fortify Health ability, and after a save and a live load, the stored term is unchanged (OSL-HP-002, OSL-SAVE-003).
6. **A pre-feature save is refused (`cargo test`).** `rejects_major_version_skew` (`crates/save`) covers a snapshot of the previous major; a snapshot captured with a new column left out of the registry loads with that column at its defaults (OSL-SAVE-002).
7. **Waiting does not level (`cargo test`).** A unit test feeds the level-up system a wait event and a completed-sleep event; only the sleep applies a pending level (OSL-LVL-004).

---

## 8. Metrics

The numbers live in [ROADMAP](../../ROADMAP.md)'s per-game compatibility matrix, with the Oblivion row; the numbers sit in that row's existing cell (fed skills, and fed actions by school).

| Metric | Definition | Granularity |
|---|---|---|
| Fed skills | Oblivion skills with at least one action the engine feeds ÷ 21 | per phase |
| Fed actions | Actions in UESP's skill table the engine feeds ÷ UESP's skill table, its actions counted in Phase 0 (V4) | per skill school (combat, magic, stealth) |
| Verify items closed | **Phase 0: verify** items settled ÷ the items of §9 | per phase |

---

## 9. Delivery sequence

**Phase 0 — Decode, census, decisions (no engine code).** Close V1–V15 from the executable (the level-up routine near `0x66c3e0` and its callers, the skill-use routine and its settings, the sleep trigger) and from the load order (the `GMST` census, the vanilla scripts' calls of the level commands, the `Player` record's class), correct the ruleset document's "Leveling — LOCKED" paragraph (it names `fSkillUseCurve` 1.95, which is Skyrim's setting, and counts only major skill increases toward the attribute bonus where UESP counts all governed skills), write `docs/engine/skill-use.md` (the emitter contract of OSL-XP-007 and the action census of V4), file the level-up and sleep/rest menus against the Game UI Integration PRD, correct the whole `ModPCSkill` / `GetAV` / `SetAV` / `ModAV` row of `docs/engine/scripting.md` (or file its stale actor-value half against the Oblivion Actor Values PRD), and record the decisions (Q1 to Q5 are decided). Exit: every verify item closed or filed as an issue, the ruleset and scripting documents corrected, `skill-use.md` merged, the menu request filed, and every §10 decision recorded.

**Phase 1 — Experience and rank-up.** OSL-XP-001 to OSL-XP-007, OSL-LVL-001, OSL-LVL-002, OSL-CLS-001 to OSL-CLS-003, OSL-API-001, OSL-API-006, OSL-API-003 for the state that exists in this phase, and the saved state of OSL-SAVE-001 for experience. Depends on the CHARAL ruleset and the Oblivion actor values already built. Exit: acceptance scenarios 1, 2 and 3 pass.

**Phase 2 — Skill use.** OSL-USE-001 to OSL-USE-007 for the combat, magic and stealth actions whose systems exist (V4's census names them). Depends on `skill-use.md` and on the Sneak and Detection PRD's undetected level for Sneak. Exit: the fed-skills and fed-actions metrics are published, and a live `levelup` in a fight, a cast and a sneak shows the matching skills banking.

**Phase 3 — Levels.** OSL-LVL-003 to OSL-LVL-006, OSL-SLP-001, OSL-SLP-002, OSL-ATT-001, OSL-ATT-002, OSL-HP-001 to OSL-HP-003, OSL-API-002, OSL-API-004, OSL-API-005 and the rest of OSL-SAVE. The commands depend on the ObScript runtime. Exit: acceptance scenarios 4 to 7 pass, and #7044 closes with them. The remaining player-facing bindings depend on [GUI-XML-004…007](game-ui-integration.md#59-menuxml-menus-beyond-the-hud-gui-xml), with acceptance scenarios 11 and 12 in that PRD.

The verify items Phase 0 closes:

| # | Item | What settles it | Answer (#7049) |
|---|---|---|---|
| V1 | Whether the minor multiplier is the `fSkillUseMinorMult` setting the CS names or `fSkillUseMinor` the UESP formula names | The setting's name in `Oblivion.esm`'s `GMST` records and the executable's string table | `fSkillUseMinorMult`; the major multiplier replaces the minor one (UESP *Increasing Skills*) |
| V2 | Whether a skill at 100 keeps banking experience | The skill-use routine in `Oblivion.exe` | A skill at 100 or more banks nothing; the engine reads the current value, Fortify included (#7210) — declared |
| V3 | Whether UESP's experience per action are the executable's skill-use increments | The skill-use table in `Oblivion.exe` against `GetSkillUseIncrement` ([CS: GetSkillUseIncrement](https://cs.uesp.net/wiki/GetSkillUseIncrement)) read by script | The `SKIL` records' two use values, authored by `Oblivion.esm` |
| V4 | The census of actions: which armor piece takes a hit's experience, which jump, fall and dodge events count (UESP: Acrobatics "each jump or dodge"), whether a pickpocket counts | The skill-use call sites in `Oblivion.exe` | [skill-use.md](../engine/skill-use.md) §2 — declared |
| V5 | Whether a rank-up writes the base skill or a separate stored value | The rank-up routine in `Oblivion.exe` | The base skill — declared |
| V6 | What ends a sleep as interrupted, whether the engine can interrupt one, and whether the original's `SetPCSleepHours` applies a level | UESP's *Sleeping and Eating*, the executable's sleep routine and the command's handler | A completed sleep applies a pending level, waiting does not (UESP *Sleeping and Eating*); the script case — declared |
| V7 | Which value the executable truncates, the gain or the stored sum (UESP: the gain, rounded down) | The level-up routine near `0x66c3e0` (the #7041 decode is the starting point) | The stored sum, equal to UESP's rounded-down gain for an integer term |
| V8 | Whether the Health gain reads Endurance before or after the level's attribute bonus | The same routine's order of writes | After the picks — declared |
| V9 | Whether the level-up writes the stored terms of Magicka or Fatigue, and whether the load path zeroes the `Player` record's stored pools (`0x5240b2`) | The same routine's writes to actor values 9 and 10, and the load path | Only stored Health; the load path zeroes the three stored pools — declared |
| V10 | The vanilla `Player` record's class and its seven major skills | The `NPC_` and `CLAS` records of `Oblivion.esm` | `CharactergenClass` (`CLAS` 000230E6), OSL-CLS-001 |
| V11 | How many vanilla scripts call `AdvancePCSkill`, `ModPCSkill`, `AdvancePCLevel` and `SetLevel` on the player | `obscript_tools` over the three masters | `ModPCSkill` twice in one script; `AdvancePCSkill`, `AdvancePCLevel` and `SetLevel` on the player never |
| V12 | Where the level-up message's text lives: a load-order string or the executable | The `GMST` and string census of `Oblivion.esm`, the executable's string table | [skill-use.md](../engine/skill-use.md) §5; `sMeditate`'s default text — declared |
| V13 | Whether the rank-up that makes the tenth major increase counts toward its governing attribute's tally (UESP *Leveling* says "before gaining the 10th increase" but its Example 2 implies it counts) | The rank-up routine's order of writes in `Oblivion.exe`: the tally update against the level-count check | It counts (UESP *Leveling* Example 2) — declared |
| V14 | Whether the `Amount` of `AdvancePCSkill` and `ModPCSkill` is skill ranks or experience units (the CS says "increases the player's skill by Amount") | The two commands' handlers in `Oblivion.exe`, and the arguments vanilla callers pass (V11) | Ranks; the one vanilla caller's message says "raised by 2 points" |
| V15 | Whether the live UESP *Oblivion:Health* page documents a 2024 formula beside the 2006 one the local dump holds | A diff of the live page against `/mnt/data/src/reference/uesp-wiki/Oblivion/Health.wiki`; the 2006 rule stands either way | Oblivion Remastered's, under its own heading; the 2006 rule stands |

**Provenance.** Phase 0 answered these items from `Oblivion.exe` (#7049) before the owner ruled out decoding the executables for rules (2026-11-20). Documented: V1 (`Oblivion.esm`'s `GMST` names; UESP *Increasing Skills* for the major multiplier), V3, V10, V11 and V14 (`Oblivion.esm`'s records and its one caller's message), V15 (the live page), and the outcomes of V6 and V7 (UESP *Sleeping and Eating*, *Health*). #7331 re-sourced more of them: OSL-XP-002's defaults (the CS `fSkillUse*` pages), V2's cap (UESP *Increasing Skills*, "the maximum value of 100"; what value the gate reads is the owner's decision, #7210), V5 and V9 (UESP *Leveling*: the skill rises, and Magicka and Fatigue derive from the attributes), V12's hint (UESP *First Time Players*; the wording is the engine's, since `Oblivion.esm` authors no `sMeditate`) and V13 (UESP *Leveling*'s Example 2). Still **declared** as engine choices: V4's call sites (UESP's action table is the census's source), V8, the rank-up boundary and surplus, the cost of a skill at 0, the stance timers' shape and order, and the armor-hit roll (#7310); [skill-use.md](../engine/skill-use.md) §1 lists each.

---

## 10. Decisions required

1. **Q1 — Where the stored Health term lives.** Recommended: a dedicated saved layer on the player's Health actor value that `undamaged()` adds, so the HUD, regeneration and restores keep reading one maximum, and abilities and gear keep their own layer. Alternative: the permanent-modifier layer: no shape change, but it also carries abilities and gear, so removing one cannot tell its share from the gains (#7044's concern). Alternative: a component the Health formula row reads as an input: every consumer of the maximum then needs to know it.
   **Decided** (2026-11-18): as recommended: a dedicated saved layer on the player's Health actor value that `undamaged()` adds (OSL-HP-002, OSL-SAVE-001).
2. **Q2 — The first skill-use wave.** Recommended: feed the combat skills, the magic schools and Sneak, Athletics and Acrobatics, the actions whose systems exist, and leave Security, Speechcraft, Mercantile, Alchemy and Armorer unfed and reported (OSL-USE-006). Alternative: feed the missing skills from simple stand-in actions so every skill rises; the player levels faster than the original in those skills and the table's numbers lose their meaning.
   **Decided** (2026-11-18): as recommended: only the skills whose system exists are fed; the rest are reported unfed (OSL-USE-006).
3. **Q3 — The level-up surface before the native page.** Recommended: the console command `levelup attr <attribute>`, mirroring the Skyrim `levelup pool` picks, plus the level-up notification. Alternative: pick the three attributes by a fixed rule (the highest tallies): a level then needs no input, but the player loses the choice that defines the original's level-up. Alternative: wait for the native page, which no design owns yet (OSL-NG-004): nothing then plays end to end.
   **Decided** (2026-11-18): as recommended: the console command `levelup attr <attribute>` plus the level-up notification (OSL-API-005, OSL-API-004).
4. **Q4 — The player's class until chargen.** Recommended: the `Player` `NPC_` record's class (OSL-CLS-001), which follows the [Oblivion Actor Values PRD](oblivion-actor-values.md)'s decided Q3 (the player's values come from that record). Alternative: a console command that sets the class for a session: more testable, but a second source of truth next to the record. Alternative: wait for a character-creation PRD: nothing in this PRD can then be played end to end.
   **Decided** (2026-11-18): as recommended: the class of the `Player` `NPC_` record (OSL-CLS-001).
5. **Q5 — What `ModPCSkill` does to the skill's banked experience.** Recommended: the Beta Patch 1.1 behaviour the CS page documents, the skill rises and its banked experience is set to 0.0: no negative experience, and the behaviour of the patched game this engine targets. Alternative: the unpatched behaviour, where the experience falls by the old usage limit: it can leave negative experience, which no other path produces, and it wastes the experience earned so far. Alternative: leave the banked experience untouched, the effect of `SetActorValue` the page suggests as a workaround: the kindest to the player, but it matches neither shipped behaviour, so a script written against either sees a different result (OSL-API-006).
   **Decided** (2026-11-18): as recommended: the Beta Patch 1.1 behaviour, the skill rises and its banked experience is set to 0.0 (OSL-API-006).

---

## 11. Document map

| Need | Go to |
|---|---|
| The leveling vocabulary and the per-game rulesets | [CHARAL](../engine/charal.md) |
| Oblivion's formulas, the skill roster, the derived stats | [CHARAL — Oblivion Ruleset](../engine/charal-oblivion-ruleset.md) |
| The actor values the skills and attributes live in | [Oblivion Actor Values — PRD](oblivion-actor-values.md) |
| The save contract the new state joins | [Save/load round trip](../engine/save-load-roundtrip.md) |
| The commands and the console | [Scripting](../engine/scripting.md) |
| The level-up and sleep menus | [Game UI Integration — PRD](game-ui-integration.md#59-menuxml-menus-beyond-the-hud-gui-xml), GUI-XML-004…007 (#7048, #7086) |
| The skill-use hooks and the action census | [Skill use](../engine/skill-use.md) |
| Undetected sneaking, the Sneak skill's action | [Sneak and Detection PRD](sneak-and-detection.md) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-17 | First draft, at the owner's request, from #7044's blocked status | — |
| 2 | 2026-11-17 | Review round 1 folded in: container vs actor level readers, menu and emitter owners, phase dependencies, acceptance fixtures, uncited original-engine claims, undefined terms, lock and storage invariants, links not copies | claude: revise |
| 3 | 2026-11-18 | Owner decisions Q1 to Q4 recorded, each as recommended: a dedicated saved Health layer, only skills with a system fed, the console `levelup attr` surface, the `Player` record's class | — |
| 4 | 2026-11-18 | Review round 2 folded in: tenth-increase tally (V13), `AdvancePCSkill` amount unit (V14), Health-page revision (V15), `ModPCSkill` behaviour (OSL-API-006, Q5), observable notification and class-read requirements (OSL-API-004, OSL-CLS-002, OSL-CLS-003), Acrobatics and Sneak census, Related row for undetected sneaking, "pending" vocabulary, stale `scripting.md` row, `eval.rs` proof, `levelup` description, API ID order | claude: revise |
| 5 | 2026-11-18 | OSL-SAVE-002 and scenario 6 corrected: the repository's format rule refuses older majors instead of defaulting them; the stored Health layer (Q1) is built ahead of Phase 3 (#7044, v136) | — |
| 6 | 2026-11-18 | Owner decision Q5 recorded, as recommended: `ModPCSkill` raises the skill and sets its banked experience to 0.0 (the Beta Patch 1.1 behaviour); OSL-API-006 states it | — |
| 7 | 2026-11-18 | Owner's approval recorded: all five §10 decisions decided, review rounds 1 and 2 folded in, gate clean; implementation issues may now be filed (README §9) | owner: approved |
| 8 | 2026-11-18 | APPROVED → ACTIVE: parent #7047, phases #7049–#7052, menu request #7048; the Phase 0 verify items are on #7049 | — |
| 9 | 2026-11-18 | Phase 0 (#7049): the executable decode answers V1 (the setting is `fSkillUseMinorMult`; the major multiplier replaces the minor one), V2 (a skill at 100 banks nothing), V3 (the amounts are the `SKIL` records' use values), V5 (a rank-up writes the base skill), V6 (a completed sleep applies a level; `bInstantLevelUp` opens the menu at once), V7 to V9 (the stored Health sum is truncated, Endurance is read after the attribute picks, only stored Health is written, the load path zeroes the `Player` record's stored pools), V10 to V12 and V14 (the `Player` class, the vanilla callers, the message settings, `Amount` is ranks), V13 (the tenth major increase counts toward its tally) and V15 (the live UESP page's second formula is Oblivion Remastered's). OSL-XP-002's defaults are corrected: Factor 1 and Exp 1 are the executable's, 0.35 and 1.5 are `Oblivion.esm`'s. V4's census is `docs/engine/skill-use.md`, with four items left to Phase 2 | — |
| 10 | 2026-11-18 | Phase 1 (#7050) built: the rank cost, banking and class (`character::skill_use`), `SkillUseLevel` (saved, `FORMAT_MAJOR` 137), the entry point and the drain (`oblivion_skill_use`), `AdvancePCSkill` / `ModPCSkill` (`0x10d4` / `0x110f`, decoded from the executable's command table), and `levelup`'s class, tally and per-skill report; scenarios 1 to 3 pass (`oblivion-level-up.sh`) | — |
| 11 | 2026-11-18 | Phase 2a (#7051): the `SKIL` use values held as data and checked against `Oblivion.esm`; emitters at the one `HitEvent` site feed Blade, Blunt and Hand to Hand (a landed melee blow) and Light and Heavy Armor (a hit the player takes, the struck piece decoded from `0x5e5a00`); `levelup` reports 5 of 21 skills fed and the unfed list (Block waits on a target-side guard, Marksman on a bow shot path). Spells, Athletics, Sneak, Acrobatics and the §8 metrics are 2b | — |
| 12 | 2026-11-18 | Phase 2b (#7051): the stance emitters (Sneak and Athletics per second of the stance, Acrobatics per jump) and the §8 metrics published in the ROADMAP matrix: 8 of 21 skills fed, 9 of 26 UESP actions (combat 6 / 8, magic 0 / 8, stealth 3 / 10). The schools stay unfed: the engine has no player cast action (only `magic.cast` and a script's `Cast`) | — |
| 13 | 2026-11-18 | Phase 3 (#7052) built: pending levels with frozen tallies (`SkillUseLevel::pending`, `FORMAT_MAJOR` 138), `levelup attr`, the completed-sleep trigger (`SleepCompleted` from `sleep_system`), the level application with the stored Health term, `AdvancePCLevel`, and Oblivion's `SetPCSleepHours`; scenarios 4 to 7 pass. A bed outside a jail has no rest action and nothing interrupts a sleep, both recorded in `skill-use.md` §5 | — |
| 14 | 2026-11-19 | Exits re-checked at HEAD `4dd44bd84`: the seventeen named scenario tests pass (scenario 2 against the real `Oblivion.esm`: the cost at skill 5 is 2.89) and `oblivion-level-up.sh` passes (Blade 5 to 7, Sneak 5 to 6), but the PRD is not moved to SHIPPED: OSL-USE-004 is unmet (no player cast action, #7053), the Block and Marksman parts of OSL-USE-002 and -003 and the Acrobatics fall wait on #7054 to #7056, Phase 2's exit names a live cast that has not been run, and the level-up and sleep/rest menus (#7048) are open. Three stale "Today" lines (OSL-USE-002, -004, -006) corrected to HEAD and the four follow-up issues added to the Tracking line. Owner decision of this date: keep ACTIVE until the cast action and the menus land | — |
| 15 | 2026-11-20 | #7048: link the menu contract and acceptance checks in Game UI Integration, and record the scripted and interactive sleep boundaries for the Phase 3 bindings | — |
| 16 | 2026-11-20 | #7086: an ordinary bed sleep offers the oldest pending level and the interactive menu submits all three picks to the shared rules. Scripted sleep continues to use console picks | — |
| 17 | 2026-11-22 | Review round 3 folded in: OSL-USE-002, -003, -004 and -006 restated as built (16 of 21 skills fed; Block, Marksman, the six schools and Acrobatics' fall); §3 rewritten at HEAD by symbol, with revision 16's §3 the pre-build record, and the "Today" clauses dropped; §1's HEAD paragraph dated to the draft; Phase 0's V1–V15 answers written into their requirements, with an Answer column and a provenance note in §9; the executable-derived answers labelled declared, and OSL-XP-002's major multiplier re-sourced to UESP *Increasing Skills*; the Related row, §6 rows 1, 2, 4 and 6 and the §11 rows link skill-use.md and GUI-XML-004…007; §8's home corrected; scenario 3 states the commands the smoke script runs; the follow-ups relabelled closed; the status line trimmed | claude: revise |
| 18 | 2026-11-22 | ACTIVE → SHIPPED. Exits re-measured at `299403322`: the scenario tests pass (core `character::` 244, scripting `oblivion_skill_use` 18, the two real-data tests against `Oblivion.esm`, `rejects_major_version_skew`, and the binary's `skill_use_emit` 19, `oblivion_rest` 2 and `save_io` 120), and so does `oblivion-level-up.sh` (Blade 5 to 7, Sneak 5 to 6). Revision 14's hold is met: the player cast action (#7053) and the menus (#7048, #7086) have landed, and #7054 to #7056 are closed. Metrics re-checked: 16 of 21 skills fed, 18 of 26 UESP actions, 15 of 15 verify items closed (7 declared). #7047 is closed | — |
| 19 | 2026-11-23 | Owner decisions on #7310 (two rules that rested only on the executable, after the 2026-11-20 ruling): `AdvancePCSkill` sets the banked experience to 0.0 like `ModPCSkill` (OSL-API-001; the executable's surplus carry is dropped), and the armor-hit roll keeps its order, shield remainder and seven attempts as a declared engine choice beside the CS-documented weights (`docs/engine/skill-use.md` §3). Status unchanged | — |
| 20 | 2026-11-23 | #7331: the rules Phase 0 read from the executable re-cited to the CS `fSkillUse*` pages and UESP *Increasing Skills*, *Leveling* and *First Time Players*; the rest declared engine choices (Provenance paragraph, [skill-use.md](../engine/skill-use.md) §1). No behaviour change. Status unchanged | — |
