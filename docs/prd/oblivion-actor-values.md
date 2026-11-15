# Oblivion Actor Values — Product Requirements

**Status**: ACTIVE (2026-11-15). Revision 5 files the tracking issues; revision 4 records the owner's approval; revision 3 the decisions on Q1–Q3; revision 2 folded in review round 1. Statements
about how Oblivion behaves cite external sources (UESP, the Construction Set
wiki dump under `/mnt/data/src/reference/`, xEdit's `wbDefinitionsTES4.pas`);
a claim that could not be verified is marked **Phase 0: verify** instead of
being stated as fact.

**Tracking**: #7007 (parent). Phases: #7008 (0), #7009 (1), #7010 (2), #7011 (3).

**Scope**: every Oblivion actor (NPC, creature and the player) carries its
actor values (attributes, derived pools, skills and the extra values UESP
lists) in the engine's one canonical store, `ActorValues`, read from its
record or computed by Oblivion's own auto-calc rules. The consumers that
already exist for the other games then read them on Oblivion: CHARAL's built
Oblivion ruleset, detection (M65), sneak attacks, magic effects, conditions,
ObScript `GetAV` / `SetAV`, the HUD bars and pool regeneration. Disposition
toward the player, which Oblivion's attack rule compares with Aggression, is
in scope as its starting value and that comparison only (Q2). Outside it: character creation menus,
leveling the player (CHARAL's existing leveling model owns it), Morrowind,
and the 2024 *Remastered* formulas.

**Related**: this document states the *promise*; the designs below own the
*how*. Every mechanism promised here has an owner in this table, or Phase 0
writes the missing design.

| Mechanism | Owning design |
|---|---|
| The per-game ruleset, derived stats, leveling, and population of canonical character state | [CHARAL](../engine/charal.md) |
| Oblivion's formulas (derived pools, regen, damage, movement, disposition shape) | [CHARAL — Oblivion ruleset](../engine/charal-oblivion-ruleset.md) |
| Detection and sneak-attack consumers | [Sneak and Detection](../engine/sneak-detection.md) |
| Engine-supplied actor-value records and the Oblivion stat models | Phase 0 writes a section of [CHARAL](../engine/charal.md) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `OAV-` prefix.

---

## 1. Product thesis

Oblivion is the one in-scope game whose actors have no stats in the engine.
Its master authors no `AVIF` records: actor values are hardcoded indices in
the executable ("The following tables provide a list of all Actor Values
recognized by Oblivion, as well as their corresponding numerical indices",
UESP *Oblivion Mod:Actor Value Indices*), while every engine consumer keys
actor values by `AVIF` FormID. So CHARAL's Oblivion ruleset is built and
unwired, the HUD bars sit full, `GetAV` declines, the detection formula's
skill terms read 0 and every sneak attack takes the Novice ladder (#6995).

The engine closes that gap at the record boundary, not in the consumers: the
load order gains the actor-value records Oblivion never authored, with
Oblivion's own names and indices, and Oblivion's records fill them. After
that, nothing downstream knows it is running Oblivion.

**Corollary**: the engine supplies only what Oblivion's executable hardcodes
(the index table) and computes only what Oblivion's own documented rules
compute (auto-calc). It never invents a stat an actor's record and those
rules do not give.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Actor value** | One of Oblivion's 72 indexed values, 0 Strength to 71 ResistWaterDamage (UESP *Oblivion Mod:Actor Value Indices*). The **standard** values are 0–32 (attributes, Health, Magicka, Fatigue, Encumbrance, the 21 skills); the **extra** values are 33–71 (Aggression, Chameleon, Blindness, resistances …). |
| **Engine-supplied `AVIF`** | An `AVIF` record the engine adds to an Oblivion load order for one index, with the UESP internal name as its EditorID. Not in any plugin. |
| **Stat model** | How one record kind's values are produced: CHARAL's `NpcStatModel` (`Stored`, `ClassAutoCalc`, `CreatureData`, …). |
| **Auto-calc** | An NPC whose attributes and skills the game computes: "The level and class of the NPC is used to determine the final values" (CS *Stats Tab - NPC*). xEdit TES4 names `ACBS` bit 4 "Auto-calc stats" and bit 7 "PC Level Offset", and the CS page ties auto-calc to the latter ("automatically calculated if the PC Level Offset is checked"); which flag selects it is a **Phase 0: verify** item (OAV-POP-003). |
| **Disposition** | How much one actor likes another, the value Oblivion's attack rule compares with Aggression (UESP *Oblivion:Disposition*). |

---

## 3. Core model

### 3.1 Keys

The engine supplies one `AVIF` record per index 0–71, EditorID the UESP
internal name (`Strength`, `HandToHand`, `NightEyeBonus`, …), under an
engine-owned plugin identity in the content-addressed Form ID space (ROADMAP
keep list, *Plugin identity*; decision Q1), never a raw reserved `u32` range.
The index → FormID table is a row of `EsmIndex::resolve_actor_value`, beside
FO3 / FNV's and Skyrim's, so `ActorValueRef::Index` from a magic effect, a
CTDA operand or an ObScript argument reaches the same record a name does.
Fame and Infamy (38, 39) are reserved, "*not* recognized as actor values by
most functions" (UESP); which functions do accept them is a **Phase 0:
verify** item (OAV-RES-002).

### 3.2 Where each actor's values come from

| Record kind | Model | Source |
|---|---|---|
| `NPC_` without auto-calc | Stored | `DATA`: 21 skills (u8), Health (u16), 8 attributes (u8); `ACBS`: base spell points, Fatigue (xEdit TES4 `NPC_`) |
| `NPC_` with auto-calc | Oblivion auto-calc | Race and gender base attributes and skill bonuses (`RACE` `ATTR`, `DATA`), class favored attributes, specialization and major skills (`CLAS` `DATA`), the level; the rules of §3.4 |
| `CREA` | Creature data | `DATA`: Combat / Magic / Stealth skill, Health, attack damage, 8 attributes; `ACBS` spell points, Fatigue. For a PC-level-offset creature, Health, spell points and Fatigue are "multiplied by the level of the creature" (CS *Stats Tab - Creatures*) |
| The player | Q3 | The `Player` `NPC_` record, or character creation |
| `AIDT` (all actors) | Extra values 33–36 | Aggression, Confidence, Energy, Responsibility, 0–100 (xEdit TES4 `AIDT`; already decoded, #6995) |

### 3.3 Consumers, at HEAD on 2026-11-15

| # | Consumer | Today on Oblivion |
|---|---|---|
| 1 | CHARAL ruleset (derived pools, leveling) | `CharacterRulesProfile::OBLIVION` carries `ruleset: RulesetBuilder::None` and `npc_stats` / `creature_stats: NpcStatModel::None` (`crates/core/src/character/profile.rs`) |
| 2 | Actor-value index resolution | `EsmIndex::resolve_actor_value` maps FO3 / FNV and Skyrim indices; Oblivion returns `None` (`crates/plugin/src/esm/records/index.rs`) |
| 3 | `NPC_ DATA` stats | Parsed on FO3 / FNV only; no Oblivion arm (`crates/plugin/src/esm/records/actor/npc.rs`) |
| 4 | Conditions `GetActorValue` | `condition_actor_value_form` resolves FO3 / FNV and Skyrim indices; Oblivion falls to `None`, so the condition fails closed (`crates/scripting/src/actor_value_names.rs`) |
| 5 | ObScript `GetAV` / `SetAV` / `ModAV` | `obscript_actor_values::actor_value_form` returns `None` unless FO3 / FNV |
| 6 | Detection (M65) | Sneak, Chameleon, Blindness and Night Eye read 0 (`byroredux/src/detection/tick.rs::actor_value`); the skill term is 0 |
| 7 | Sneak attacks (M65) | The attacker's Sneak reads 0, so every sneak attack takes the Novice ladder (`stealth::oblivion::sneak_attack_mult`) |
| 8 | HUD bars | Health / Magicka / Fatigue stay full unless pinned: "Oblivion resolves no ids today" (`byroredux/src/hud.rs::bar_fractions`) |
| 9 | Pool regeneration | `PoolRegenConfig` is built only in `npc_spawn/stamps.rs::build_vats_model` (the FO3 / FNV Action Points lane); no Oblivion path builds one |
| 10 | Hostility (the CS aggression rule) | `Aggression::Rated` starts no combat: no disposition is kept (`byroredux/src/systems/faction_hostility.rs::attacks`, #6995) |

This table is hand-written; the Phase 2 census (§7.2 scenario 3) replaces
rows 1, 3 and 6–8 with measured numbers, and ROADMAP's Oblivion row carries
them from then on.

### 3.4 Auto-calc rules

UESP *Oblivion:NPCs*, "NPC Statistics". Level-1 values are "identical to
those for a player character created using the same race, gender, and
class" (same page); their exact construction is a **Phase 0: verify** item
(OAV-POP-003).

| Value | Rule |
|---|---|
| Skill, per level | +0.1 minor, +0.6 specialization, +1.0 major, +1.5 major and specialization; rounded to a whole number |
| Attribute, per level | +0.6 / +1.4 / +2.2 / +3 for 0 / 1 / 2 / 3 governed major skills |
| Personality | Fixed (UESP and CS agree) |
| Luck | Fixed per UESP; the CS names only Personality (**Phase 0: verify**) |
| Health | `fLowLevelNPCBaseHealthMult`, `iLowLevelNPCMaxLevel`, `fNPCAttributeHealthMult`, `iNPCBasePerLevelHealthMult` formula (defaults 0.4, 4, 0.5, 4), set at spawn |
| Magicka | 2.5 × Intelligence |
| Fatigue | Endurance + Strength + Agility + Willpower |
| Encumbrance | 5 × Strength |

### 3.5 Reporting

The install log names the engine-supplied `AVIF` count and the stat model of
each record kind. An actor's value is read live with
`obscript.cmd <ref> GetAV` (the existing debug command). Coverage is the
census's output (§7.2 scenario 3).

---

## 4. Goals and non-goals

### 4.1 Goals

- **OAV-G-001 — Every index resolves.** Every index UESP lists resolves by
  name and by index on an Oblivion load order, with Fame and Infamy (38, 39)
  as Phase 0 settles.
- **OAV-G-002 — Every actor has its values.** Every spawned Oblivion NPC,
  creature and the player carries the 33 standard values.
- **OAV-G-003 — The consumers read them.** Every consumer in §3.3 reads
  Oblivion values; the count still on a stand-in (0, full, `None`) is zero.

### 4.2 Non-goals

- **OAV-NG-001.** Character creation (race, class, birthsign menus): the
  engine has no Oblivion new-game route, and the player's values come from
  its record (Q3).
- **OAV-NG-002.** Persuasion, bribery and the Speechcraft minigame: they
  change disposition but are dialogue features (UESP *Oblivion:Disposition*,
  "Increasing Disposition").
- **OAV-NG-003.** The 2024 *Remastered* formulas: the engine targets classic
  Oblivion ([CHARAL](../engine/charal.md)).
- **OAV-NG-004.** Morrowind, which is not one of the seven titles.

---

## 5. Requirements by area

### 5.1 Resolution (`OAV-RES`)

- **OAV-RES-001.** An Oblivion load order MUST carry one engine-supplied
  `AVIF` per recognized index, EditorID the UESP internal name. Today it
  carries none (§3.3 row 2).
- **OAV-RES-002.** `EsmIndex::resolve_actor_value(Index(i))` MUST return the
  engine-supplied record for each `i` UESP lists, and `None` above 71.
  **Phase 0: verify** which functions accept Fame and Infamy (38, 39; UESP
  *Oblivion Mod:Actor Value Indices*: "not recognized as actor values by most
  functions").
- **OAV-RES-003.** The engine-supplied records MUST carry an engine-owned
  plugin identity distinct from every loaded plugin's (Q1); a test MUST
  assert it.
- **OAV-RES-004.** `GetActorValue` / `GetPermanentActorValue` conditions MUST
  resolve an Oblivion operand. **Phase 0: verify** that Oblivion's CTDA
  stores the actor-value index (UESP *Oblivion Mod:Mod File Format*,
  CTDA).
- **OAV-RES-005.** ObScript `GetAV` / `SetAV` / `ModAV` / `ForceAV` MUST
  resolve Oblivion actor values by the name the CS uses (CS *GetActorValue*).

### 5.2 Population (`OAV-POP`)

- **OAV-POP-001.** The parser MUST decode Oblivion's `NPC_ DATA` (21 skills,
  Health, 8 attributes; xEdit TES4) and `CREA DATA` (Combat, Magic and
  Stealth skill, Health, attack damage, 8 attributes).
- **OAV-POP-002.** An `NPC_` without auto-calc MUST carry its `DATA` and
  `ACBS` values as authored.
- **OAV-POP-003.** An auto-calc `NPC_` MUST carry the values §3.4's rules
  give, computed at spawn from its level at that moment (UESP
  *Oblivion:NPCs*: "health is set at NPC's spawn"). **Phase 0: verify** which
  `ACBS` flag selects auto-calc at runtime (bit 4 or bit 7; CS *Stats Tab -
  NPC* and xEdit TES4 differ), whether Luck is fixed (UESP and CS differ), and
  the level-1 values: the base skill value, the major-skill and
  specialization bonuses and the favored-attribute bonus (UESP
  *Oblivion:Skills*, *Oblivion:Attributes*).
- **OAV-POP-004.** A `CREA` MUST carry its `DATA` values, with Health, spell
  points and Fatigue multiplied by its level where it is PC-level-offset (CS
  *Stats Tab - Creatures*). **Phase 0: verify** how the Combat / Magic /
  Stealth skills stand in for the 21 skills when a consumer asks for one
  (Sneak for detection, Blade for damage).
- **OAV-POP-005.** Every actor MUST carry extra values 33–36 from its `AIDT`.
  **Phase 0: verify** that `GetAV Aggression` on an actor answers its `AIDT`
  byte (CS *Aggression*: "SetActorValue Aggression value").
- **OAV-POP-006.** The player MUST carry the standard values of the `Player`
  `NPC_` record as the master authors them (Q3).
- **OAV-POP-007.** On an Oblivion index `build_character_ruleset` MUST return
  `Some`, and a `PoolRegenConfig` with Fatigue and Magicka lanes MUST be
  installed (§3.3 rows 1, 9).

### 5.3 Consumers (`OAV-USE`)

- **OAV-USE-001.** Detection's Sneak, Chameleon, Blindness and Night Eye
  inputs MUST read the actor's values on Oblivion (§3.3 row 6).
- **OAV-USE-002.** The sneak-attack mastery MUST follow the player's Sneak
  (§3.3 row 7).
- **OAV-USE-003.** The HUD bars MUST track the player's Health, Magicka and
  Fatigue (§3.3 row 8).
- **OAV-USE-004.** A constant-effect Fortify Sneak ability on an Oblivion
  actor MUST raise its `GetAV Sneak` by the effect's magnitude, through
  `canonical_spell`'s existing path.
- **OAV-USE-005.** An actor's starting disposition toward the player MUST
  follow UESP *Oblivion:Disposition* "Starting Disposition", and a rated actor
  MUST attack the player when its Aggression exceeds it (CS *Aggression*),
  replacing §3.3 row 10 (Q2).

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Actor values are executable-hardcoded indices; no record exists (UESP *Oblivion Mod:Actor Value Indices*) | Engine-supplied `AVIF` records (OAV-RES-001) | — | A FormID collision with a mod's form → OAV-RES-003 |
| 2 | Auto-calc stats are computed by the executable at load or spawn (CS *Stats Tab - NPC*: "Autocalc'd NPCs have this field grayed out, but it shows the calculated value") | Computed at spawn from the sourced formulas (OAV-POP-003) | — | A formula detail wrong at level 1 shifts every generic NPC → OAV-POP-003's verify item |
| 3 | Disposition has many inputs (personality, race, faction, fame, infamy, crimes, charm, persuasion) | Starting disposition only (OAV-USE-005, Q2) | — | Persuaded or charmed actors still attack → OAV-NG-002 records it |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | The only game without `AVIF` records |
| Fallout 3 | n/a | Authors `AVIF`; populated today |
| Fallout: New Vegas | n/a | Authors `AVIF`; populated today |
| Skyrim (LE / SE / AE) | n/a | Authors `AVIF`; populated today |
| Fallout 4 | n/a | Authors `AVIF` |
| Fallout 76 | n/a | Authors `AVIF` |
| Starfield | n/a | Authors `AVIF` |

### 7.2 Acceptance

A scenario passes when:

1. **Resolution.** A `cargo test` resolves every recognized index by name and
   by index on an Oblivion `EsmIndex`, and asserts 38, 39 and 72 do not
   resolve (OAV-RES-001, -002).
2. **Stored and auto-calc.** A `cargo test` stamps a stored and an auto-calc
   `NPC_` and asserts each standard value against UESP's worked numbers (a
   Knight's per-level gains; level-1 Health 12–20 for most races, UESP
   *Oblivion:NPCs*) (OAV-POP-002, -003).
3. **Census.** An ignored real-data test over `Oblivion.esm` reports, per
   record kind, the actors whose 33 standard values are all populated; the
   pass is 100 % of `NPC_` and `CREA` (OAV-G-002).
4. **Live.** `docs/smoke-tests/oblivion-actor-values.sh`, needing
   `BYROREDUX_OBLIVION_DATA` and exiting 77 without it, boots
   `AnvilTheCountsArms` and reads `GetAV Sneak` on a patron, a non-zero
   detection skill term in `stealth.status`, and a `hud.status` Health value
   that falls with damage (OAV-USE-001, -003).

---

## 8. Metrics

| Metric | Definition | Granularity |
|---|---|---|
| Index coverage | resolved recognized indices ÷ 70 | per phase |
| Population coverage | Oblivion actors with all 33 standard values ÷ actors censused | per record kind |
| Consumers on real values | §3.3 consumers reading Oblivion values ÷ 10 | per phase |

The numbers are published in ROADMAP's per-game compatibility matrix,
Oblivion row.

---

## 9. Delivery sequence

**Phase 0 — Survey, design and decisions (no engine code).** Close the
verify items in OAV-RES-002, -004, OAV-POP-003, -004 and -005, write the
CHARAL section that owns the engine-supplied records and the Oblivion stat
models, and take Q1–Q3. Parent: #6995. Exit: every verify item answered with
a source in the design, and §10 decided.

**Phase 1 — Resolution.** OAV-RES-001 to -005. Exit: §7.2 scenario 1 passes;
index coverage 70 / 70.

**Phase 2 — Population.** OAV-POP-001 to -007. Depends on CHARAL's built
Oblivion ruleset ([charal.md](../engine/charal.md) §5). Exit: §7.2 scenarios 2
and 3 pass; population coverage published.

**Phase 3 — Consumers.** OAV-USE-001 to -005.
Depends on M65's consumers ([sneak-detection.md](../engine/sneak-detection.md)
§4, §8).
Exit: §7.2 scenario 4 passes; consumers on real values 10 / 10.

---

## 10. Decisions required

1. **Q1 — How Oblivion actor values are keyed.** Recommended: engine-supplied
   `AVIF` records under an engine-owned plugin identity in the
   content-addressed Form ID space (ROADMAP keep list), so `ActorValues`,
   `ActorValueNames`, CHARAL's `resolve("Sneak")`, conditions, ObScript and
   magic keep one key and no consumer branches on the game (CLAUDE.md
   invariant 1; the translation-boundary rule). Alternative: key Oblivion's
   `ActorValues` by index directly; every consumer then needs an Oblivion
   path, which is the branching the layers exist to avoid.
   **Decided** (2026-11-15): engine-supplied `AVIF` records under an
   engine-owned plugin identity.
2. **Q2 — Disposition.** Recommended: in scope as Phase 3's last item, the
   starting disposition toward the player and the CS aggression comparison
   only, which is what #6995 needs for Oblivion to start combat.
   Alternative: its own PRD covering dialogue, barter, persuasion and crime
   reactions too; Oblivion actors keep starting no combat until it lands.
   **Decided** (2026-11-15): in scope, the starting disposition toward the
   player and the aggression comparison only.
3. **Q3 — The player's starting values.** Recommended: the `Player` `NPC_`
   record's stats, as Oblivion's master authors them, since the engine has no
   Oblivion character creation. Alternative: an Oblivion character-creation
   route (race, class, birthsign), a much larger feature with its own UI.
   **Decided** (2026-11-15): the `Player` `NPC_` record's stats.

---

## 11. Document map

| Need | Go to |
|---|---|
| The ruleset, stat models and population boundary | [CHARAL](../engine/charal.md) |
| Oblivion's formulas | [CHARAL — Oblivion ruleset](../engine/charal-oblivion-ruleset.md) |
| Detection and sneak-attack consumers | [Sneak and Detection](../engine/sneak-detection.md) |
| The follow-up this answers | #6995 |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-15 | First draft | — |
| 2 | 2026-11-15 | Review round 1 folded in: auto-calc flag and Luck verify, creature level scaling, Fame / Infamy verify, Remastered citation, regen proof, observable OAV-USE-004 / -POP-007, Q4 folded into OAV-POP-003, reporting, content-addressed identity for engine-supplied records, phase dependencies, live smoke named, auto-calc table | claude: revise |
| 3 | 2026-11-15 | Owner's decisions recorded: Q1 engine-supplied `AVIF` records under an engine-owned plugin identity, Q2 starting disposition and the aggression comparison in scope (OAV-USE-005 unconditional), Q3 the `Player` `NPC_` record (OAV-POP-006) | — |
| 4 | 2026-11-15 | PROPOSED → APPROVED | owner: approved |
| 5 | 2026-11-15 | APPROVED → ACTIVE: parent #7007, phases #7008–#7011; the Phase 0 verify items are on #7008 | — |
