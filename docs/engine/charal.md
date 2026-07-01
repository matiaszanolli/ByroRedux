# CHARAL — Character Abstraction Layer

**CHARAL** (Character Abstraction Layer; pronounced "CARE-al") is the canonical
translation tier for **character progression** — the attributes, skills, perks,
level, experience, and derived resources that define an actor's capability and
how it grows. It is the sibling of [`nifal.md`](nifal.md), [`exal.md`](exal.md),
[`physal.md`](physal.md), and [`watal.md`](watal.md): where NIFAL translates
per-game **NIF geometry/material** data, EXAL per-game **ESM environment** data,
PHYSAL per-game **Havok physics** data, and WATAL per-game **water authoring**,
CHARAL translates each game's per-game character **ruleset** — the stats it
tracks, how those stats are derived from authoring, and how they level — into one
canonical character state the gameplay runtime reads identically for every game.

"Abstraction" is the brand; the mechanism is **canonical translation** (per-game
`Imported*` + per-game rules → one resolved, game-agnostic character state). The
verbs stay `translate` / `derive` / `canonical` / `resolve`; **CHARAL** names the
layer as a whole.

**Status**: PROPOSED (design, 2026-06-29). The Fallout numeric substrate
(`ActorValues`, #1663) and the FNV/FO3 SPECIAL→skill auto-calc population (#1663,
commit `fad3890b`) are the reference realisation — see §3. Rollout per §8,
**starting with FO4**.

**Goal**: every supported game — the Fallout family (FO3 / FNV / FO4 / FO76), the
TES family (Morrowind / Oblivion / Skyrim), and Starfield — translates its native,
per-game character authoring **and rules** into one canonical character state:
`ActorValues` (every numeric property) + `CharacterLevel` + `Perks` +
`Background`. The gameplay runtime (conditions, combat, dialogue, skill checks,
perk entry points, leveling) consumes that state **identically for every game** —
no per-game branches downstream, no `Option` "resolve-it-later" fallbacks.

This is the same doctrine NIFAL formalises
(`feedback_format_translation` — "never per-game branches downstream; translate at
the parser boundary"; the `format_abstraction` GameVariant pattern), now applied
to the character/progression pipeline.

---

## 0. What makes CHARAL different from its siblings

NIFAL, EXAL, PHYSAL, and WATAL translate **static authored data** — a vertex
buffer, a WATR record, a Havok constraint. Decode the bytes, fold the per-game
quirks, done. CHARAL is the first layer whose per-game seam is a **ruleset**, not
a wire format:

- A character's stats are not merely *read* from a record — they are **derived**
  through per-game formulas (an auto-calc NPC's skills come from its class SPECIAL
  via the GECK derived-skill model, not from stored bytes).
- A character is not static — it **levels**, and the leveling model is per-game
  (Fallout spends XP on perks/skill points; TES raises level through skill use;
  Starfield is a mix).

So CHARAL translates two per-game things its siblings don't: the **derivation
rules** (authoring → canonical stats) and the **progression rules** (how the
canonical stats evolve). Both live at the boundary; the runtime stays
game-agnostic.

CHARAL is **single-sink** (unlike the double-ended PHYSAL/WATAL): the only
consumer is the gameplay runtime, which reads canonical `ActorValues` / `Level` /
`Perks`. There is no second consumer to resolve against — the richness is all on
the **source** side (rules, not just data).

---

## 1. What's universal vs per-game — the three families

The load-bearing observation (per the proposal): every game's character system is
*its own* leveling/stats/perks model, **but the similarity is large and falls into
three families.**

| Family | Attributes | Skills | Level driver | Perks | Derived pools |
|---|---|---|---|---|---|
| **Fallout — FO3 / FNV** | 7 SPECIAL | 13, governed by a SPECIAL | XP → level | per-level pick | Health / AP / CarryWeight = f(SPECIAL, level) |
| **Fallout — FO4 / FO76** | 7 SPECIAL | **none** (perks replace skills) | XP → level | SPECIAL-rank-gated chart | Health / AP / CarryWeight = f(SPECIAL, level) |
| **TES — Morrowind / Oblivion** | 8 attributes | ~21–27, governed by an attribute | **skill-use → level** | none | Health / Magicka / Fatigue = f(attributes) |
| **TES — Skyrim** | **none** | 18, level via skill-XP | **skill-XP → level** | per-skill perk trees | Health / Magicka / Stamina (+10 pick/level) |
| **Starfield** | none | skills in 5 categories | XP → level → skill point | skill ranks (+ challenges) | derived pools |

*(TES / Starfield skill counts and governance are placeholders for the
user-provided data — §5 defines the intake shape.)*

### Universal (the canonical core)

- **All character state is numeric** → it all lands in **`ActorValues`** (built).
  SPECIAL, skills, resistances, Health / AP / Magicka / Stamina, every derived
  value — one component, one composition (`base + permanent + temporary − damage`),
  keyed by AVIF FormID. No per-game numeric type.
- **Every character has a level + a progress metric** → **`CharacterLevel`** (even
  TES, where the level is *driven* by skill use, still *has* a level and an
  accumulator).
- **Most have perks** → **`Perks`** (the [[perk_system]] modifier-pipeline owner;
  [[perk_entry_points]]).
- **Every character has provenance** — race + class/background (+ birthsign /
  traits) that seeded its base stats → **`Background`**.

### Per-game (the only thing CHARAL translates) — the **ruleset**

Which AVs exist, how base stats derive from authoring, the derived-stat formulas,
and the leveling model. That is the entire per-game surface, and §3 shows most of
it is **AUTHORED in the game's own data**, not engine-hardcoded.

---

## 2. The tier model

```
   ESM records ────parse───▶  Imported*  ────derive()────▶  Canonical  ────read───▶  Gameplay runtime
  (NPC_ / CLAS / RACE /       (raw class,    (per-game ruleset    (ActorValues +     (conditions, combat,
   AVIF / GMST / PERK)         SPECIAL,       folds in: auto-calc,  CharacterLevel +   dialogue, skill checks,
                               skills, level) derived stats,        Perks +            perk entry points,
                                              level scaling)        Background)        leveling)
```

| Tier | What it is | Where it lives | Rule |
|---|---|---|---|
| **Raw / `Imported*`** | A faithful decode of per-game character authoring: NPC SPECIAL / class / level / skills / perks (`NPC_`), class base attributes (`CLAS`), race bonuses (`RACE`), the AV set (`AVIF`), formula constants (`GMST`), perk gates (`PERK`). **Allowed to be messy.** | `crates/plugin/src/esm/records/` (`actor`, `class`, `avif`, …) | Decode only; never the source of truth. |
| **`derive()` boundary** | The per-game **ruleset** applied: auto-calc (SPECIAL→skills), derived-stat formulas, level scaling, perk-gate resolution. Exactly **one** site per concern. | `crates/plugin/.../actor_value_derive.rs` (today: FNV/FO3) → a per-game ruleset module set | One producer; no duplicate derivation. |
| **Canonical** | The game-agnostic components the runtime reasons about: `ActorValues` (built) + `CharacterLevel` + `Perks` + `Background`. | `crates/core/src/ecs/components/` | The single source of truth. |
| **Runtime** | Gameplay reads canonical state identically. `GetActorValue` already reads `ActorValues` (`crates/scripting/src/condition.rs`). | conditions / combat / dialogue / leveling | No per-game branches. |

### The canonical-type rule (inherited from NIFAL)

> **Where an ECS component already serves the game-agnostic, engine-facing role,
> that component IS the canonical type. Introduce a new canonical type only where
> none exists.**

`ActorValues` IS the canonical numeric type — CHARAL adds *populators* and
*derivers*, never a parallel numeric struct. The genuinely new types
(`CharacterLevel`, `Perks`, `Background`) fill roles no component fills yet.

---

## 3. The ruleset is mostly AUTHORED (AVIF + GMST + CLAS), not hardcoded

The same AUTHORED / SENTINEL split WATAL uses for water applies to character
rules — and the load-bearing finding is that **most of a game's character ruleset
is parsed from its own data**, not engine-hardcoded:

| Ruleset element | Source | Status |
|---|---|---|
| Which AVs exist (SPECIAL, skills, resources) | **AUTHORED** — `AVIF` records → `EsmIndex::actor_values` | parsed (#1663) |
| Base SPECIAL of an auto-calc NPC | **AUTHORED** — `CLAS` base attributes (+ `RACE` bonus) | `CLAS` parsed; `RACE` bonus pending |
| Derived-skill constants (`fAVDSkillBase`, `…PrimaryBonusMult`, `…LuckBonusMult`) | **AUTHORED** — `GMST` game settings | values known; read as **hardcoded constants today**, should read from parsed `GMST` (§8.4) |
| XP / level curve (`iXPBase`, `iXPLevelUpBase`, …) | **AUTHORED** — `GMST` | pending |
| Perk gates (required SPECIAL / level / rank) | **AUTHORED** — `PERK` conditions | pending |
| **Skill → governing attribute** map | **ENGINE-SUPPLIED** — not in any single record; per-game engine knowledge | canonical `SkillSet` rosters shipped (OBLIVION / SKYRIM / FALLOUT_FO3_FNV); FNV/FO3 population consumes it |
| **Procedural leveling strategy** (OB attribute-multiplier from skill-ups; Skyrim skill-XP curve) | **ENGINE-SUPPLIED** — irreducibly procedural | shipped: `oblivion_attribute_bonus` (+1…+5), `skyrim_skill_xp_to_next` / `_between` (`fSkillUseCurve` 1.95); Morrowind out of scope |

So CHARAL's "ruleset" = **(a) AUTHORED**, parsed from `GMST`/`AVIF`/`CLAS`/`RACE`/
`PERK`, **+ (b) ENGINE-SUPPLIED**, the governing-attribute maps and leveling
strategies the parsed data does not carry. The user-provided "data for each skill"
populates (b)'s declarative half; (a) comes from the ESM. **No guessing**
([[feedback_no_guessing]]): the AUTHORED half is read, never assumed; the
ENGINE-SUPPLIED half is sourced from the user's public data or cited research.

---

## 4. Canonical character model (proposed components)

### 4.1 `ActorValues` — **BUILT** (the numeric substrate)

All numeric character state: SPECIAL, skills, resistances, derived pools — every
AVIF-keyed value, layered `base + permanent + temporary − damage`
(`crates/core/src/ecs/components/actor_values.rs`, #1663). CHARAL **reuses** this
as-is; it adds population and derivation around it, not a new numeric type.

### 4.2 `CharacterLevel` — **NEW**

```rust
pub struct CharacterLevel { level: u16, xp: f32 /* progress toward next */ }
```

Universal. Fallout / Starfield: `xp` is experience points. TES: `xp` is the
skill-XP accumulator (Skyrim) or major-skill-up count (MW/OB). The per-game
leveling strategy (§5 `LevelingModel`) advances it.

### 4.3 `Perks` — **NEW** (the [[perk_system]] owner)

```rust
pub struct Perks { entries: Vec<(u32 /* PERK FormID */, u8 /* rank */)> }
```

The component the perk entry-point modifier pipeline ([[perk_entry_points]])
iterates. Fallout 3+ / Skyrim / Starfield.

### 4.4 `Background` — **NEW** (provenance + leveling inputs)

```rust
pub struct Background { race: u32, class: Option<u32> /*, birthsign / traits */ }
```

Carries what **population** consumed (race / class) so **runtime leveling** can
reuse it — MW/OB class governs the per-level attribute multipliers; FNV class tag
skills drive per-level skill growth. Without this, leveling would have to re-find
the class record at every level-up.

### 4.5 `FactionReputation` — **NEW, BUILT** (reputation-family storage)

```rust
pub struct FactionReputation { entries: Vec<FactionStanding> }  // FactionStanding { faction, fame: u16, infamy: u16 }
```

The storage half of the **reputation family** ([`character::reputation`]): per-faction
Fame/Infamy, both **monotonic** (FNV reputation never drops — `add_fame`/`add_infamy`
saturate; `reset` zeroes for the scripted NCR/Legion/disguise exceptions).
`standing(faction, &thresholds)` bridges the stored pair to the `ReputationStanding`
4×4 classifier. **Karma needs no component** — it is already an `ActorValues` entry, so
the reputation family's two instances split cleanly: Karma rides the AV substrate,
faction Reputation gets this dedicated component (FO4 companion *affinity* will be a
third, per-companion variant). Player-scoped in practice; a component so it rides the
ECS/save machinery like the rest of CHARAL.

---

## 5. The per-game ruleset (the data the user provides)

One `CharacterRuleset` Resource per loaded game, assembled at load from **AUTHORED**
(parsed §3) + **ENGINE-SUPPLIED** tables:

```rust
pub struct CharacterRuleset {
    attributes: AttributeSet,           // 7 SPECIAL | 8 TES attrs | none  (shipped)
    skills:     SkillSet,               // roster + governing-attr map  (shipped; Oblivion 21)
    derived:    Vec<DerivedStatFormula>,// Health / AP / CarryWeight / Magicka / … = f(attrs, level)
    skill_calc: SkillDerivation,        // base / attr-mult / luck-mult  (from GMST)
    leveling:   LevelingModel,          // XpCurve { … } | SkillUse { … } | SkillXp { … }
}
```

`AttributeSet` (shipped — `crates/core/src/character/attribute.rs`) is the per-game
**roster** over a canonical [`Attribute`] union (the lineage-wide set: Strength /
Endurance / Intelligence / Agility / Luck shared, Perception+Charisma Fallout-only,
Willpower+Speed+Personality TES-only). Membership is ENGINE-SUPPLIED per family
(`AttributeSet::FALLOUT` / `TES_CLASSIC` / `SKYRIM` / `STARFIELD` const rosters);
each attribute's AVIF FormID stays AUTHORED, produced on demand by
`AttributeSet::resolve(editor_id → form_id)` so the canonical identity travels with
the number and the consumer never branches on game.

`SkillSet` (shipped — `crates/core/src/character/skill.rs`) is the parallel skill
roster: `SkillDef { editor_id, governing: Option<Attribute> }` × the game's skills.
Skills are EditorID-keyed (large, game-specific — no union enum), but the **governing
attribute is canonical** ([`Attribute`]), so the skill→attribute map reads
game-agnostically. `SkillSet::OBLIVION` ships (21 skills, governing map sourced from
the Elder Scrolls Wiki — Luck governs none); `SkillSet::NONE` covers FO4/FO76.
`resolve()` pairs each skill's AUTHORED AVIF id with its governor's id, degrading an
unresolved governor to `None` rather than dropping the skill. Shipped rosters:
`SkillSet::OBLIVION` (21 governed), `SkillSet::SKYRIM` (18 ungoverned),
`SkillSet::FALLOUT_FO3_FNV` (15 = FO3 ∪ FNV, SPECIAL-governed) and `SkillSet::NONE`
(FO4/FO76). The Fallout set is the **single source** of the auto-calc governing map —
the FNV/FO3 population path (`actor_value_derive.rs`) consumes it (mapping each governor
to its class-attribute index via the shared `SPECIAL` order) instead of a local table.
Morrowind's 27 skills are out of scope (not in the compat list).

TES derived pools (shipped — `crates/core/src/character/tes.rs`):
`oblivion_health_formula` = 2×Endurance, `oblivion_magicka_formula` = 2×Intelligence,
`oblivion_fatigue_formulas` = Strength+Willpower+Agility+Endurance (sourced; Health/Magicka
from the ES Wiki, Fatigue from UESP). All player-scoped. Fatigue's four-attribute sum
exceeds the two-input `DerivedStatFormula`, so it ships as **four affine rows** summed by
`derived_value` — the resolved shape of the multi-row generalisation (`push_derived` doc).

`LevelingModel` is now an **enum** with all three shapes: `XpCurve { xp_a, xp_b, level_cap,
reward }` (Fallout — FO3/FO4/FNV consts), `SkillUse { major_skill_ups_per_level, level_cap }`
(Oblivion — `OBLIVION` = 10 major-skill-ups/level), and `SkillXp { xp_base, xp_mult,
xp_per_skill_rank, pool_pick_gain, level_cap }` (Skyrim — `SKYRIM` = 25·L+75 XP, 1 XP/skill
rank, +10 pool pick + perk/level; UESP-sourced). Skyrim helpers: `xp_from_skill_rank`,
`pool_pick_gain`. **`skyrim_ruleset(resolve)`** assembles TES V: empty `AttributeSet::SKYRIM`
(no attributes) + the 18 ungoverned `SkillSet::SKYRIM` skills + `LevelingModel::SKYRIM`, with
an **empty derived table** (Health/Magicka/Stamina aren't attribute-derived — they start at
`SKYRIM_POOL_BASE` 100 and grow only by the level pick). Archery/Speech use their CK internal
AV names (`Marksman`/`Speechcraft`); resolution is verified at load (resolve-or-skip).
**`oblivion_ruleset(resolve)`
now assembles the full TES ruleset end-to-end** — `AttributeSet::TES_CLASSIC` +
`SkillSet::OBLIVION` + `LevelingModel::OBLIVION` + the three derived pools, resolve-or-skip
like the Fallout builders. The level-up leveling-efficiency mechanics are shipped too:
`oblivion_attribute_bonus(governed_skill_ups)` → +1/+2/+3/+4/+5 by the UESP tier table
(0 / 1–4 / 5–7 / 8–9 / 10+), capped, no roll-over; and
`oblivion_health_gain_per_level(endurance)` = 10 % of Endurance accrued (and stored) each
level (path-dependent, so a per-level event, not a stateless formula). **Classic Oblivion
(2006 Gamebryo) only** — the live UESP *Oblivion:Health* page now documents the 2024 UE5
*Remastered* formula, which is out of scope. **Oblivion is now CHARAL-complete** end-to-end.

The user-provided per-game **data tables**, by family — each slots directly into
the struct above; **the canonical runtime never changes**:

- **Fallout** — the 7 SPECIAL (have it), the skill list + each skill's governing
  SPECIAL (FO3/FNV; **empty** for FO4/FO76), the derived-stat formulas, and the
  perk chart (perk × required SPECIAL × required level × ranks).
- **TES** — the 8-attribute list (MW/OB) or none (Skyrim), the skill list + each
  skill's governing attribute, the derived pools (Health/Magicka/Fatigue or
  Stamina), and the leveling model (major-skill-up count, or skill-XP curve).
- **Starfield** — the skill categories + skills, backgrounds → starting skills,
  the XP/level curve.

**FO4 (the starting point)** needs, concretely: the **derived-stat formulas**
(Health / AP / Carry Weight / Melee Damage / XP-multiplier as functions of SPECIAL
+ level), the **perk chart** (70-cell SPECIAL×rank grid with level gates), and the
**XP/level curve**. The SPECIAL set itself is shared with FNV (AV codes 5–11).

---

## 6. Derived statistics — computed, not stored

**Decision** (resolving the prior open question): derived stats (Health, AP, Carry
Weight, Melee Damage, Magicka, Stamina, …) are **computed on demand** from base AVs
via the ruleset's `DerivedStatFormula`, **not** materialised into `ActorValues` at
spawn. Rationale:

- It is what Bethesda does (derived AVs are read-only, flagged `Derived` in the AV
  system — see [[actor_value_system]]).
- It keeps `ActorValues` to *authored* bases, so an attribute change can't leave a
  stale derived value behind.
- The formula is per-game DATA, so the seam stays at the ruleset, not in a spawn
  path.

A `derived_value(av, &ruleset, &avs, level)` helper evaluates the formula when a
reader asks for a derived AV by FormID; the formula table is the single source.
Each formula needs a **citable source** per game (no guessing) — supplied by the
user-provided tables or cited research (§9).

---

## 7. What stays out of scope

- **No new gameplay systems.** CHARAL produces canonical character *state*; it does
  not implement combat, dialogue, or the perk *effects* — those consume the state.
  ([[perk_entry_points]] is the perk-effect design; CHARAL just owns the `Perks`
  component it reads.)
- **No player chargen yet.** There is still no stat-bearing player-actor entity
  (`scene.rs`'s `player_entity` is an `AnimationPlayer`) — see
  [[actor_value_population]]. CHARAL designs *where* player stats land (the same
  canonical components) but player creation is a separate slice.
- **No Vulkan / render changes.** Like every sibling layer, CHARAL is pure ECS +
  parse; nothing touches the render pass or pipeline.

### 7.1 Companion / non-player progression (source: fandom *Companion*)

A companion is just an NPC, so its **stats land in the same canonical `ActorValues`** —
but the page confirms its *progression strategy* differs from the player's XP curve, and
that difference is exactly a `LevelingModel` variant, not a new component:

- **FO3 / FNV / FO4 — *scale-to-leader*.** Companion stats scale off the **player's**
  level (FO3 capped, *Broken Steel* lifts to 30; FO4 uncapped), not their own XP. This
  is a distinct leveling strategy — `LevelReward`/`LevelingModel` gains a `ScaleToLeader`
  arm whose "level" input is the player's, with the actual per-level numbers coming from
  the NPC's level-list / template records, not a hardcoded curve.
- **FO4 *affinity* is ANOTHER reputation-family instance.** Per-companion approval moves
  up/down with player actions and at **max** unlocks a permanent companion perk. That is
  the same `{ AV + band classifier → effect }` shape as Karma — but **scoped to one
  relationship** (one affinity AV per companion) rather than world-wide. Reinforces the
  reputation family ([[charal-fnv-fo3-ruleset]] Karma section): Karma = global 1-axis,
  FNV Reputation = per-faction 2-axis (Fame/Infamy), FO4 affinity = per-companion 1-axis.
  The perk-at-threshold reward is scripting/quest data, as always.
- **FO76 has no traditional companions** (C.A.M.P. allies) — out of scope for now.
- **FO1 / FO2 companion mechanics** (no leveling / fixed "stage" model-swap, the
  200-byte/5-record truncation bug) are **out of scope** — those are the isometric
  pre-Gamebryo engine, not a ByroRedux target. Recorded only so the taxonomy is complete.

The takeaway for CHARAL: companions need **no new canonical type** — they reuse
`ActorValues` + (eventually) an affinity reputation AV; only the *leveling strategy* enum
grows a `ScaleToLeader` arm.

---

## 8. Rollout order

1. ~~Fallout numeric substrate — `ActorValues`~~ — shipped (#1663).
2. ~~FNV/FO3 population — SPECIAL→skills auto-calc~~ — shipped (#1663, `fad3890b`).
   The reference realisation (§3 AUTHORED model, `derive_npc_actor_values`).
3. **FO4 population (STARTING POINT)** — SPECIAL from its storage (the open
   research item §9) + the derived-stat formulas + perk-gate population. No skills.
4. **Canonical model** — add `CharacterLevel` / `Perks` / `Background`; introduce
   the `CharacterRuleset` Resource; generalise `derive_npc_actor_values` from one
   FNV function into the per-game ruleset module set.
5. **Derived-stat deriver** — the computed-derived helper (§6) + the per-game
   formula tables.
6. **GMST sourcing** — replace the hardcoded `fAVDSkill*` constants with values
   read from parsed `GMST` records (§3), closing the last AUTHORED gap.
7. **TES family** — attribute/skill ruleset; the skill-use leveling strategy
   (MW/OB) and skill-XP strategy (Skyrim).
8. **Starfield** — skill-category ruleset; background → starting skills.

Each phase ships independently behind `cargo test` (pure derivation + ruleset unit
tests; no Vulkan, no game data required for the unit layer — real-data validation
follows the smoke-test pattern).

---

## 9. Open research items (no-guessing — [[feedback_no_guessing]])

- **FO4 NPC SPECIAL storage.** Whether FO4 NPCs store SPECIAL as `PRPS`
  `(AVIF FormID, value)` property pairs, inherit from `RACE`/template, or both —
  needed before FO4 population (item 3). *(Research was in flight when CHARAL was
  proposed; resume before implementing FO4.)*
- **Per-game derived-stat formulas.** Health / AP / Carry Weight / Melee Damage /
  Magicka / Stamina as functions of attributes + level — one citable formula per
  game (FO4 first).
- **TES / Starfield skill → governing-attribute maps + leveling curves.** The
  user-provided public data (§5).
- **FNV per-level tag-skill growth.** Still undocumented in any citable source
  (deferred at #1663 — see [[actor_value_population]]); pin against the engine
  before claiming tag-skill correctness.

---

## 10. Tooling (proposed)

- `char.dump <entity>` debug-server command — print the resolved canonical
  `ActorValues` + `CharacterLevel` + `Perks` + `Background` for an actor (the
  character analog of `tex.missing` / `water.dump`).
- Per-game **derive harness** — feed a representative NPC + that game's
  `CharacterRuleset`, assert the canonical stats (the FNV harness in
  `actor_value_derive.rs::tests` is the seed).
