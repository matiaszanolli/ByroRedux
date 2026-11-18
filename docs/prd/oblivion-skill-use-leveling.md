# Oblivion skill-use leveling — Product Requirements

**Status**: PROPOSED (2026-11-18). Revision 3 records the owner's decisions on Q1 to Q4, all as recommended. Revision 2 folded in review round 1: the container-versus-actor level readers, the owners of the menus and the skill-use hooks, phase dependencies, acceptance fixtures, uncited original-engine claims, undefined terms, the lock and storage invariants and links in place of copies. Statements about how Oblivion behaves cite UESP and the Construction Set wiki; a claim that could not be verified is marked **Phase 0: verify** instead of being stated as fact.

**Scope**: the player of Oblivion levels the way the original's player does: using a skill banks experience, enough experience raises the skill, ten raises of major skills make a level available, a night's sleep applies it, the player raises three attributes by the bonuses their skill use earned, and Health grows by a tenth of Endurance. Everything the player does that the original counts as skill use feeds it, where the engine has the system that action belongs to. It covers the player only, on Oblivion only (every other title has a leveling model already; §7.1). Outside it: character creation (class, birthsign, race choice), the original's trainers and skill books, creature and NPC level scaling, and the visual level-up and sleep/rest menus, which no design owns yet (§4.2).

**Related**: this document states the *promise*; the designs below own the *how*. Every mechanism promised here has an owner in this table, or Phase 0 writes the missing design.

| Mechanism | Owning design |
|---|---|
| The leveling vocabulary, `LevelingModel`, the per-game ruleset and the derived-stat rows | [CHARAL](../engine/charal.md) |
| Oblivion's formulas, skill roster and attribute governance | [CHARAL — Oblivion Ruleset](../engine/charal-oblivion-ruleset.md) |
| The player's actor values, their layers and the engine-supplied `AVIF` records the skills live in | [Oblivion Actor Values — PRD](oblivion-actor-values.md) |
| What persists, and how a save of a new column stays loadable | [Save/load round trip](../engine/save-load-roundtrip.md) |
| ObScript commands and the console | [Scripting](../engine/scripting.md) |
| The skill-use hooks: the contract between the combat, magic, movement and detection systems and the experience entry point | None today: Phase 0 writes `docs/engine/skill-use.md` |
| The level-up and sleep/rest menus as native pages | None today: [Game UI Integration — PRD](game-ui-integration.md) names Oblivion's class, birthsign and race menus and `ShowSleepWaitMenu` for Fallout 3 and New Vegas only; Phase 0 files both menus against it |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119 meanings, as in the related designs. Requirement IDs use the `OSL-` prefix.

---

## 1. Product thesis

Oblivion's progression is its identity: the player is never handed experience points. They become better at what they do, the world scales to the level that accumulates behind it, and the cost of a careless level is a character weaker than the world around them. UESP's *Oblivion:Leveling* states the model: a character qualifies for a level "after improving any combination of major Skills by a total of 10 points", must "rest on a bed" for it to take effect, then raises three attributes, and Health "increases by 10% of your Endurance" ([UESP: Oblivion Leveling](https://en.uesp.net/wiki/Oblivion:Leveling)). The *Increasing Skills* page states how a skill rises: by experience that actions grant, at a cost that grows with the skill and shrinks for specialization and major skills ([UESP: Oblivion Increasing Skills](https://en.uesp.net/wiki/Oblivion:Increasing_Skills)).

At HEAD none of this happens. The player of Oblivion starts at level 1 and stays there: `LevelingModel::OBLIVION` is a skill-use model with no experience curve, nothing banks a skill's use, and the two helpers that hold Oblivion's level-up arithmetic have no caller (§3). Every consumer of the level — `GetLevel` conditions, the level-scaled actors a cell spawns, and the level requirement every Daedric quest carries (UESP *Leveling*) — reads a level nothing raises. The follow-up #7044 shows the same gap from the actor-value side: there is no per-level Health gain to keep apart from the refreshed base, because there is no level.

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
| **Major increase** | A rank-up of a major skill; ten of them make a level available |
| **Attribute tally** | Per attribute, the rank-ups of the skills it governs since the last level-up, major and minor ("how many skill points you have gained in skills governed by this attribute (major *and* minor)", UESP *Leveling*) |
| **Pending level** | A level the tenth major increase made available and a sleep has not applied; it carries the attribute tallies frozen at that moment, and several can be pending at once |
| **Level-up** | Applying the available level: the level rises by one, three attributes rise by their tally bonuses, Health gains a tenth of Endurance, the tallies reset |
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

The inventory below is the state at HEAD on 2026-11-17. The Today column is hand-written, and each row's proof is the file that shows it. The §7.2 scenarios and the console `levelup` replace it as each row is built; every phase exit re-measures the rows it touches.

| # | Piece | Today | Proof |
|---|---|---|---|
| 1 | Oblivion's leveling model | `LevelingModel::OBLIVION` is `SkillUse { major_skill_ups_per_level: 10, level_cap: 0 }`; `xp_to_next` is 0 and `roll_levels` reaches no level for it | `crates/core/src/character/leveling.rs:196`, `:250`, `:264` |
| 2 | Attribute bonus table | `oblivion_attribute_bonus(governed_skill_ups)` returns the bonus table of UESP *Leveling*; no caller outside its tests | `crates/core/src/character/tes.rs:186` |
| 3 | Per-level Health gain | `oblivion_health_gain_per_level(endurance)` returns `0.1 × endurance`; no caller outside its tests and doc comments | `crates/core/src/character/tes.rs:214` |
| 4 | Skill roster and governance | `SkillSet::OBLIVION` carries the 21 skills and their governing attributes; the engine-supplied `AVIF` records give each skill an actor value | `crates/core/src/character/skill.rs:307`; [Oblivion Actor Values — PRD](oblivion-actor-values.md) |
| 5 | Class data | `CLAS` parses the specialization and the seven major skills for Oblivion | `crates/plugin/src/esm/records/actor/class.rs:41`, `:54` |
| 6 | Skill experience machinery | `advance_skill` banks usage experience against a skill's `AVSK` tuple (`SkillScaling`) and `SkillProgress` holds it (saved). The engine-supplied Oblivion `AVIF` records are built with no `AVSK` (`crates/plugin/src/esm/records/oblivion_actor_values.rs`), so every Oblivion skill reads "not a skill" | `crates/scripting/src/skills.rs:90`, `:98–104` |
| 7 | Who writes skill advances | Papyrus `Game.AdvanceSkill` and the Skyrim smithing forge; the system doc says another ruleset's player "has no skill-use leveling, so the call changes nothing there" | `byroredux/src/combat.rs:1524`, `byroredux/src/crafting.rs:345` |
| 8 | Sleep | `sleep_system` runs the game clock forward by the hours `SetPCSleepHours` queued; no code makes the player sleep in a bed by activation, and activating a bed only ends a jail sentence | `byroredux/src/systems/script_requests.rs:285`; `byroredux/src/crime.rs:332` |
| 9 | Level writers | ObScript `SetLevel` writes `CharacterLevel.level` and no stat; the console `levelup` is Skyrim-shaped (skill points, perks, pool picks) | `crates/scripting/src/obscript_more/tail.rs:423`; `byroredux/src/commands/gameplay.rs:615` |
| 10 | Level readers | `GetLevel` reads `CharacterLevel`; actor spawn reads the player's level through `player_level_for_spawn`, falling back to the `Player` record's authored level; a container's contents resolve against it (one while no player exists) | `crates/scripting/src/condition.rs:47`; `byroredux/src/npc_spawn/stamps.rs:100`; `byroredux/src/cell_loader/references/attach.rs:238` |
| 11 | Persistence | `CharacterLevel` and `LevelUpPoints` are replacing columns; `SkillProgress` is a saved resource | `byroredux/src/save_io/registry.rs:306`, `:305`; `crates/scripting/src/skills.rs:83` |
| 12 | ObScript commands | `AdvancePCSkill`, `ModPCSkill` and `AdvancePCLevel` have no handler; `SetLevel` and `ModPCMiscStat` have | `docs/engine/scripting.md:2666` (it lists `ModPCSkill` as unserved, waiting on an Oblivion ruleset that now exists: Phase 0 updates that row) |

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
- **OSL-NG-004.** The visual level-up and sleep/rest menus. No design owns them today: the [Game UI Integration PRD](game-ui-integration.md) names neither for Oblivion, and Phase 0 files them against it. Until they exist the console is the surface (§5.7, Q3).
- **OSL-NG-005.** Oblivion Remastered's formulas. The Health page documents a 2024 formula that differs from the 2006 engine ([UESP: Oblivion Health](https://en.uesp.net/wiki/Oblivion:Health)); the target is the 2006 game (`crates/core/src/character/tes.rs:209–212`).
- **OSL-NG-006.** Shivering Isles and the other expansions' level rules (the Felldew addiction that lowers base attributes, the Oghma Infinium's free skill boosts): they are content scripts that use the commands of §5.7.

---

## 5. Requirements by area

### 5.1 Skill experience (`OSL-XP`)

- **OSL-XP-001.** One entry point MUST bank `uses` of an action against an Oblivion skill and return the ranks it reached, in order; skill use, `AdvancePCSkill`-style script calls and later trainers MUST all go through it. Today the Skyrim entry point (`advance_skill`) resolves the skill's `AVSK` tuple and finds none on Oblivion (§3 row 6).
- **OSL-XP-002.** The experience a skill needs to rise from value `L` MUST be `fSkillUseMinor × (fSkillUseFactor × L) ^ fSkillUseExp`, multiplied by `fSkillUseSpecMult` for a specialization skill and by `fSkillUseMajorMult / fSkillUseMinor` for a major skill, with the settings read from the load order's `GMST` records and the defaults 1.25, 0.35, 1.5, 0.75 and 0.75 when absent ([CS: FSkillUseExp](https://cs.uesp.net/wiki/FSkillUseExp), [UESP: Increasing Skills](https://en.uesp.net/wiki/Oblivion:Increasing_Skills#Experience_Points_Needed)). The pairing of `fSkillUseMinor` with `fSkillUseMinorMult` (the CS page names the latter, UESP the former) is **Phase 0: verify** (V1).
- **OSL-XP-003.** Experience beyond the cost MUST roll over to the next rank, and a skill MUST NOT exceed 100 by use ("Any extra experience points will roll over", UESP *Increasing Skills*; the cap is the skill maximum). Whether a skill at 100 keeps banking is **Phase 0: verify** (V2).
- **OSL-XP-004.** A skill's class (base, specialization, major, both) MUST come from the player's class record: `CLAS` specialization and seven major skills. Today the records parse (§3 row 5) and nothing reads them for the player.
- **OSL-XP-005.** The experience a banked skill holds MUST persist across a save and a load (§5.8). Today `SkillProgress` persists for the Skyrim path (§3 row 11).
- **OSL-XP-006.** The rules MUST be `LevelingModel::SkillUse` data overlaid with the load order's settings, as `SkillXp` overlays Skyrim's (`with_gmst`), with no `if Oblivion` branch in the system that applies them. Today `with_gmst` has an arm for the Fallout and Skyrim models and none for `SkillUse` (`crates/core/src/character/leveling.rs:97`).

- **OSL-XP-007.** Skill-use emitters in the combat, magic, movement and detection systems MUST append to one pending resource that a single system drains into the entry point, and MUST NOT lock the player's `ActorValues` themselves, with the drain's scheduler access declared (CLAUDE.md invariants 3 and 4; precedent `PendingSkillAdvances`, `byroredux/src/boot/schedule/late/scripted_combat.rs:28`).

### 5.2 Skill use (`OSL-USE`)

- **OSL-USE-001.** One use of an action the engine feeds MUST grant the experience of that action's row in UESP's table ([Increasing Skills](https://en.uesp.net/wiki/Oblivion:Increasing_Skills#Acquiring_Experience_Points)); the table is held as data, and a test iterates every fed row of every skill the ruleset names. Whether the table's numbers are the executable's own increments, the values `GetSkillUseIncrement` reads ([CS: GetSkillUseIncrement](https://cs.uesp.net/wiki/GetSkillUseIncrement)), is **Phase 0: verify** (V3).
- **OSL-USE-002.** A weapon hit MUST feed the skill of the weapon (Blade, Blunt, Hand to Hand, Marksman) only when it lands on a living target ("weapon blows must land on a creature or NPC, not an inanimate object", UESP). Today a melee or ranged hit changes no skill.
- **OSL-USE-003.** A hit the player takes MUST feed Heavy Armor or Light Armor by what the armor piece struck is, and a blocked hit MUST feed Block. Which piece, and what a mixed outfit does, is **Phase 0: verify** (V4).
- **OSL-USE-004.** A spell the player casts with a valid target MUST feed the spell's school (Alteration, Conjuration, Destruction, Illusion, Mysticism, Restoration); a spell cast on self counts as a valid target ("spells must be cast upon yourself, a creature, or an NPC", UESP). Today the caster runtime applies effects and feeds no skill.
- **OSL-USE-005.** The time-based skills (Athletics while running or swimming, Sneak while moving undetected in sneak mode) MUST bank experience per second of the stance, with the undetected condition read from the detection level ([Sneak and Detection PRD](sneak-and-detection.md)). The pickpocket and jump/fall actions follow V4's census.
- **OSL-USE-006.** A skill whose action belongs to a system the engine does not have (Security's lock minigame, Speechcraft's persuasion minigame, Mercantile's barter) MUST be reported unfed by `levelup` and counted in §8, never fed by a stand-in action (OSL-G-004). Today none of the 21 skills is fed.
- **OSL-USE-007.** Alchemy and Armorer MAY be fed once the engine has potion-making and repair actions; until then they follow OSL-USE-006.

### 5.3 Rank-up and the level count (`OSL-LVL`)

- **OSL-LVL-001.** A rank-up MUST raise the skill's base actor value by 1, and MUST NOT go through the permanent-modifier layer, which carries abilities and gear (`magic::apply_modifiers`). Whether the original raises the skill's base or a separate stored value is **Phase 0: verify** (V5).
- **OSL-LVL-002.** A rank-up of a major skill MUST count toward the level, and a rank-up of any skill MUST add to the tally of the attribute that governs it ("Even though minor skills do not count towards your level up count, they still contribute to attribute bonuses", UESP *Increasing Skills*).
- **OSL-LVL-003.** When the major increases since the last level-up reach `major_skill_ups_per_level`, the level MUST become available, and the attribute tallies at that moment MUST be the ones the level-up offers: "Your attribute bonuses are set when you gain your 10th major skill point" and rank-ups after it start the next level's tallies (UESP *Leveling*, Details).
- **OSL-LVL-004.** A level MUST NOT apply except when the player sleeps to the end of a sleep without an interruption: waiting does not apply it ("Waiting will not trigger level-up", UESP *Sleeping and Eating*). Which entry points count as sleeping in the engine is §5.4's.
- **OSL-LVL-005.** Several levels MAY be available at once after consecutive sleeps ("It is possible to sleep several times in succession with multiple level-ups", UESP *Leveling*); each MUST use the tallies saved with it.
- **OSL-LVL-006.** The level MUST NOT rise when every attribute is at its maximum of 100: the original answers "Perfection cannot be improved upon" instead (UESP *Leveling*, Notes).

### 5.4 Sleep and the level-up trigger (`OSL-SLP`)

- **OSL-SLP-001.** A completed sleep MUST be one event the level-up system reads, produced by both the script path (`SetPCSleepHours`, today `sleep_system`) and the activation of a bed. Today only the first exists (§3 row 8); no design owns the bed's sleep/rest menu (§4.2), so Phase 0 records the event's contract with it. Whether the original's `SetPCSleepHours` applies a level at all is **Phase 0: verify** (V6).
- **OSL-SLP-002.** A sleep interrupted by an enemy encounter MUST NOT apply the level ("without being interrupted by a random enemy encounter", UESP *Sleeping and Eating*); whether the engine can interrupt a sleep today is **Phase 0: verify** (V6).

### 5.5 Attributes and Health at level-up (`OSL-ATT`, `OSL-HP`)

- **OSL-ATT-001.** A level-up MUST offer three attributes, each with the bonus `oblivion_attribute_bonus` returns for its tally, and apply the three the player chose by raising their base values, to at most 100 ("It is never possible to gain more than a +5 attribute bonus", UESP *Leveling*; the cap is the attribute maximum). Attribute tallies that are not spent MUST NOT roll over.
- **OSL-ATT-002.** The raised attributes MUST flow to every derived stat through the existing refresh: after a level-up, Health, Magicka and Fatigue read the values their formulas give at the new attributes (`player_derived_stats_system`), pinned by a test that raises Endurance, Intelligence and Strength.
- **OSL-HP-001.** A level-up MUST add `floor(0.1 × Endurance)` to the player's stored Health term, rounded down: "you gain health points equal to a tenth of your endurance, rounded down", with the example of Endurance 98 giving +9 ([UESP: Oblivion Health](https://en.uesp.net/wiki/Oblivion:Health)). Which value the executable truncates, the gain or the stored sum, is **Phase 0: verify** (V7). "At the time you level" does not say whether the Endurance read is the one before or after this level's attribute bonus: **Phase 0: verify** (V8).
- **OSL-HP-002.** The stored Health term MUST be added to the derived `2 × Endurance` when the maximum is read, so it survives `player_derived_stats_system`, and MUST be independent of the permanent-modifier layer, so that adding and removing a Fortify Health ability leaves it intact (#7044; the layer's shape is Q1).
- **OSL-HP-003.** A level-up MUST NOT change the stored terms of Magicka or Fatigue unless Phase 0 finds the original writing them (V9). Which stored terms the original's level-up writes, and whether its load path zeroes the `Player` record's stored pools, is **Phase 0: verify** (V9).

### 5.6 The player's class (`OSL-CLS`)

- **OSL-CLS-001.** Until a character-creation flow exists, the player's class MUST be the class of the `Player` `NPC_` record, so its specialization and seven major skills select the skill classes of OSL-XP-004; `levelup` MUST name the class it read. Which major skills the vanilla `Player` record's class gives is **Phase 0: verify** (V10).
- **OSL-CLS-002.** A mod or script that sets the player's class MAY do so through the same record data; the engine reads it at skill use, not at boot only.

### 5.7 Surfaces (`OSL-API`)

- **OSL-API-001.** ObScript `AdvancePCSkill <skill> <amount>` MUST bank the amount through OSL-XP-001 "as if the player earned the skill by use or training" ([CS: AdvancePCSkill](https://cs.uesp.net/wiki/AdvancePCSkill)), and `ModPCSkill <skill> <amount>` MUST do what its page states, including the usage-count issue the page documents ([CS: ModPCSkill](https://cs.uesp.net/wiki/ModPCSkill)). How many vanilla scripts call either is **Phase 0: verify** (V11).
- **OSL-API-002.** ObScript `AdvancePCLevel` MUST make a level available for the player's next sleep or open the level-up surface, as the CS states ("raises the player's level by bringing up the 'level up' menu"); `SetLevel` on the player keeps changing the level and no stat ([CS: SetLevel](https://cs.uesp.net/wiki/SetLevel)).
- **OSL-API-003.** The console `levelup` MUST report, for Oblivion, the class and its major skills, per skill the experience banked and its cost, the major increases since the last level-up, each attribute's tally and bonus, and the pending levels; each field reports the state that exists in the phase that builds it (§9). Today `levelup` reports Skyrim's skill points, perks and pool picks (`byroredux/src/commands/gameplay.rs:615`).
- **OSL-API-005.** The console `levelup attr <attribute>` MUST spend one of a pending level's three picks (Q3).
- **OSL-API-004.** Applying a level MUST notify the player through the notification surface (the original shows its level-up message, UESP *Leveling*, Messages), and where the message's text lives, a load-order string or the executable, is **Phase 0: verify** (V12).

### 5.8 Persistence (`OSL-SAVE`)

- **OSL-SAVE-001.** Everything a level needs between a rank-up and its application MUST be saved: the experience per skill, the major-increase count, the per-attribute tallies, the pending levels and their frozen tallies, and the stored Health term. Each new column MUST be a component with a declared storage (CLAUDE.md invariant 2). A new column or a changed shape MUST bump `FORMAT_MAJOR` and the schema fingerprint in the same commit (`the_bump_ledger_is_contiguous_and_current`, `the_schema_fingerprint_is_pinned`).
- **OSL-SAVE-002.** A save written before this feature MUST load: the new state defaults to "no progress" and the stored Health term to 0, as every earlier column does (`crates/save/src/snapshot.rs`, the version ledger).
- **OSL-SAVE-003.** Loading a save made mid-level MUST resume it: the banked experience, the tallies and the stored term read back identical, and a re-captured snapshot of the loaded state decodes equal to the one loaded.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | What counts as skill use, and where, is the executable's (the call sites are **Phase 0: verify**, V4) | Skill use is fed by the engine's systems as the matching action exists (OSL-USE-001 to 007); a skill with no system is unfed | none | A player of a build with few emitters levels slower than the original: reported by §8's coverage metric, never papered over (OSL-USE-006) |
| 2 | The level-up menu raises the three attributes | The console `levelup attr` until a native page exists (Q3) | none | A player without the console cannot spend a level; the notification of OSL-API-004 says a level is waiting, and the menu has no owner yet (OSL-NG-004) |
| 3 | The class is chosen by the player in character creation | The class of the `Player` record, until a chargen PRD (OSL-CLS-001) | none | A vanilla player never picks a custom class, so which skills are major differs from the original's first minutes; OSL-CLS-001 names the class `levelup` read |
| 4 | Health rises by a tenth of Endurance at each level-up ([UESP: Oblivion Health](https://en.uesp.net/wiki/Oblivion:Health)); how the executable stores it is **Phase 0: verify** (V7 to V9) | The stored term is a saved layer the maximum reads (OSL-HP-002, Q1) | the per-stat "retain" mechanism #7041 removed | The layer's shape is a save-format change (OSL-SAVE-001) |
| 5 | "The world will not 'level up' with the player until he goes through a loading door, or changes cells" ([CS: SetLevel](https://cs.uesp.net/wiki/SetLevel)) | Whatever the existing consumers do on a cell load (OSL-NG-003) | none | Content that expects the world to follow the level instantly sees it a cell later; owned by #7022 / #7040 |
| 6 | Whether `SetPCSleepHours` applies a level in the original is unverified (V6) | A completed script-driven sleep applies a pending level (OSL-SLP-001) | none | A quest-driven sleep might level a player the original would not: V6 settles it before Phase 3 |

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
3. **Banking and rank-up (headless, `docs/smoke-tests/oblivion-level-up.sh`).** In Anvil's Count's Arms, `AdvancePCSkill Blade <amount>` raises Blade by the ranks the amount pays for, `levelup` reports the surplus experience, and the attribute tally of Strength follows.
4. **Level-up (same script).** Ten major increases make `levelup` report a pending level; `SetPCSleepHours 8` applies it after `levelup attr` picks three attributes; `GetLevel` reads 2, the picked attributes rose by their bonuses, and Health reads `2 × Endurance + floor(0.1 × Endurance)`.
5. **Health survives (same script and a unit test).** After `ModAV Endurance 5`, after adding and removing a Fortify Health ability, and after a save and a live load, the stored term is unchanged (OSL-HP-002, OSL-SAVE-003).
6. **A pre-feature save loads (`cargo test`).** A snapshot captured with the new columns left out of the registry, as before the feature, loads with the new state at its defaults (OSL-SAVE-002).
7. **Waiting does not level (`cargo test`).** A unit test feeds the level-up system a wait event and a completed-sleep event; only the sleep applies a pending level (OSL-LVL-004).

---

## 8. Metrics

The numbers live in [ROADMAP](../../ROADMAP.md)'s per-game compatibility matrix, with the Oblivion row; the matrix gains a *skill-use leveling* column when Phase 2 exits.

| Metric | Definition | Granularity |
|---|---|---|
| Fed skills | Oblivion skills with at least one action the engine feeds ÷ 21 | per phase |
| Fed actions | Actions in UESP's skill table the engine feeds ÷ UESP's skill table, its actions counted in Phase 0 (V4) | per skill school (combat, magic, stealth) |
| Verify items closed | **Phase 0: verify** items settled ÷ the items of §9 | per phase |

---

## 9. Delivery sequence

**Phase 0 — Decode, census, decisions (no engine code).** Close V1–V12 from the executable (the level-up routine near `0x66c3e0` and its callers, the skill-use routine and its settings, the sleep trigger) and from the load order (the `GMST` census, the vanilla scripts' calls of the level commands, the `Player` record's class), correct the ruleset document's "Leveling — LOCKED" paragraph (it names `fSkillUseCurve` 1.95, which is Skyrim's setting, and counts only major skill increases toward the attribute bonus where UESP counts all governed skills), write `docs/engine/skill-use.md` (the emitter contract of OSL-XP-007 and the action census of V4), file the level-up and sleep/rest menus against the Game UI Integration PRD, update the `ModPCSkill` row of `docs/engine/scripting.md`, and decide Q1–Q4. Exit: every verify item closed or filed as an issue, the ruleset and scripting documents corrected, `skill-use.md` merged, the menu request filed, and every §10 decision recorded.

**Phase 1 — Experience and rank-up.** OSL-XP-001 to OSL-XP-007, OSL-LVL-001, OSL-LVL-002, OSL-CLS-001, OSL-CLS-002, OSL-API-001, OSL-API-003 for the state that exists in this phase, and the saved state of OSL-SAVE-001 for experience. Depends on the CHARAL ruleset and the Oblivion actor values already built. Exit: acceptance scenarios 1, 2 and 3 pass.

**Phase 2 — Skill use.** OSL-USE-001 to OSL-USE-007 for the combat, magic and stealth actions whose systems exist (V4's census names them). Depends on `skill-use.md` and on the Sneak and Detection PRD's undetected level for Sneak. Exit: the fed-skills and fed-actions metrics are published, and a live `levelup` in a fight, a cast and a sneak shows the matching skills banking.

**Phase 3 — Levels.** OSL-LVL-003 to OSL-LVL-006, OSL-SLP-001, OSL-SLP-002, OSL-ATT-001, OSL-ATT-002, OSL-HP-001 to OSL-HP-003, OSL-API-002, OSL-API-004, OSL-API-005 and the rest of OSL-SAVE. Depends on the ObScript runtime for the commands, and on the menu request of Phase 0 for the native page (not for the console surface). Exit: acceptance scenarios 4 to 7 pass, and #7044 closes with them.

The verify items Phase 0 closes:

| # | Item | What settles it |
|---|---|---|
| V1 | Whether the minor multiplier is the `fSkillUseMinorMult` setting the CS names or `fSkillUseMinor` the UESP formula names | The setting's name in `Oblivion.esm`'s `GMST` records and the executable's string table |
| V2 | Whether a skill at 100 keeps banking experience | The skill-use routine in `Oblivion.exe` |
| V3 | Whether UESP's experience per action are the executable's skill-use increments | The skill-use table in `Oblivion.exe` against `GetSkillUseIncrement` ([CS: GetSkillUseIncrement](https://cs.uesp.net/wiki/GetSkillUseIncrement)) read by script |
| V4 | The census of actions: which armor piece takes a hit's experience, which jump and fall events count, whether a pickpocket counts | The skill-use call sites in `Oblivion.exe` |
| V5 | Whether a rank-up writes the base skill or a separate stored value | The rank-up routine in `Oblivion.exe` |
| V6 | What ends a sleep as interrupted, whether the engine can interrupt one, and whether the original's `SetPCSleepHours` applies a level | UESP's *Sleeping and Eating*, the executable's sleep routine and the command's handler |
| V7 | Which value the executable truncates, the gain or the stored sum (UESP: the gain, rounded down) | The level-up routine near `0x66c3e0` (the #7041 decode is the starting point) |
| V8 | Whether the Health gain reads Endurance before or after the level's attribute bonus | The same routine's order of writes |
| V9 | Whether the level-up writes the stored terms of Magicka or Fatigue, and whether the load path zeroes the `Player` record's stored pools (`0x5240b2`) | The same routine's writes to actor values 9 and 10, and the load path |
| V10 | The vanilla `Player` record's class and its seven major skills | The `NPC_` and `CLAS` records of `Oblivion.esm` |
| V11 | How many vanilla scripts call `AdvancePCSkill`, `ModPCSkill`, `AdvancePCLevel` and `SetLevel` on the player | `obscript_tools` over the three masters |
| V12 | Where the level-up message's text lives: a load-order string or the executable | The `GMST` and string census of `Oblivion.esm`, the executable's string table |

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

---

## 11. Document map

| Need | Go to |
|---|---|
| The leveling vocabulary and the per-game rulesets | [CHARAL](../engine/charal.md) |
| Oblivion's formulas, the skill roster, the derived stats | [CHARAL — Oblivion Ruleset](../engine/charal-oblivion-ruleset.md) |
| The actor values the skills and attributes live in | [Oblivion Actor Values — PRD](oblivion-actor-values.md) |
| The save contract the new state joins | [Save/load round trip](../engine/save-load-roundtrip.md) |
| The commands and the console | [Scripting](../engine/scripting.md) |
| The level-up and sleep menus | None yet; Phase 0 files them against [Game UI Integration — PRD](game-ui-integration.md) |
| The skill-use hooks | `docs/engine/skill-use.md`, written in Phase 0 |
| Undetected sneaking, the Sneak skill's action | [Sneak and Detection PRD](sneak-and-detection.md) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-17 | First draft, at the owner's request, from #7044's blocked status | — |
| 2 | 2026-11-17 | Review round 1 folded in: container vs actor level readers, menu and emitter owners, phase dependencies, acceptance fixtures, uncited original-engine claims, undefined terms, lock and storage invariants, links not copies | claude: revise |
| 3 | 2026-11-18 | Owner decisions Q1 to Q4 recorded, each as recommended: a dedicated saved Health layer, only skills with a system fed, the console `levelup attr` surface, the `Player` record's class | — |
