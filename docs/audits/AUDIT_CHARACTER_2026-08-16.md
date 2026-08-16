# Character / CHARAL Audit — 2026-08-16

**Scope**: `/audit-character` — all 6 dimensions, all implemented families, `--depth deep`.
Run as part of the `comprehensive` audit-suite sweep.

**Repo state**: HEAD `85b77371`, branch `main`.
**Test baseline**: `cargo test -p byroredux-core character` → **98 passed, 0 failed**
(94 on 2026-08-15; +4 from the regression tests added by the fix commits below).

**Prior audit**: `docs/audits/AUDIT_CHARACTER_2026-08-15.md` (34 findings, filed as
#2932–#2962). Twelve were fixed in the intervening 24 h
(`819c4491`, `4f1eb7dd`, `4ab3bd41`); nineteen remain OPEN and are **not** re-filed here.
This report covers what changed, what regressed (nothing), and what the first audit
could not see because it verified the code against the capture documents but never
against the shipped game data.

| Dimension | Area | Findings |
|---|---|---|
| 1 | Ruleset Seam & CHARAL Doctrine | **1 MEDIUM** |
| 2 | Derived-Stat Formulas | **2 MEDIUM** |
| 3 | Leveling & Progression | 0 |
| 4 | Pools, Afflictions, Resistances & Reputation | 0 |
| 5 | Population Boundary | 0 new (1 major, owned by `/audit-esm` — see below) |
| 6 | Coverage, Documentation & Doctrine Drift | **1 MEDIUM** |
| **Total** | | **0 CRITICAL · 0 HIGH · 4 MEDIUM · 0 LOW** |

---

## Executive Summary

### The headline is the sibling audit's, and it changes this one

`/audit-esm`, running in the same sweep, found that **`AVIF` EditorIDs are `AV`-prefixed
on FO3, FNV and Skyrim SE** (`AVStrength`, `AVHealth`, `AVSmallGuns`) while
`EsmIndex::actor_value_form_id` is queried with the bare CHARAL roster strings. That is
filed as **ESM-2026-08-16-D7-01 / D7-02** and is deliberately **not re-filed here**. I
reproduced it independently with `crates/plugin/examples/dump_record_subs.rs` before
accepting it (see Dimension 5).

It matters to this audit because it invalidates the *reading* of the 2026-08-15 coverage
matrix. That matrix was built by reading the builders; it said five of seven games have a
complete ruleset and two reach an actor. Measured against vanilla data instead:

| Game | 2026-08-15 said | Actually resolves on vanilla data |
|---|---|---|
| FNV | ruleset wired ✓, 8 derived rows | wired, **0 rows** — every `resolve()` returns `None` |
| FO3 | 8 rows (unreachable) | **0 rows** even if reached |
| Skyrim SE | 2 rows, unwired | **0 rows** — `AVDamageResist` / `AVLightArmor` / `AVCarryWeight` / `AVStamina` exist, but under `AV`-prefixed EditorIDs |
| FO4 | 4 rows, wired | **3 rows** — vanilla `Fallout4.esm` authors no `MeleeDamage` `AVIF` at all (CHAR-D2-01) |

So the honest CHARAL coverage number today is **one game (FO4) with three live derived
rows**, not "five complete rulesets, two wired." Three of those four rows come from the
one game whose `AVIF` EditorIDs happen to be unprefixed.

### What is genuinely new, and CHARAL's own

Four findings, all MEDIUM, and they share one root: **CHARAL has never been resolved
against a real `AVIF` set.**

1. **CHAR-D6-01** is the mechanism. Every per-game builder test hands the builder a
   hand-written `full()` resolver whose EditorIDs are the roster's own strings. A fixture
   that encodes the hypothesis under test cannot falsify it — which is how 98 tests stay
   green over three builders that produce empty tables on disk.
2. **CHAR-D2-02** is the part that survives the ESM-side fix. `SkillSet::FALLOUT_FO3_FNV`
   spells two FNV skills with their **display** names (`"Guns"`, `"Survival"`). Vanilla FNV
   authors `AVSmallGuns` (FULL `Guns`) and `AVThrowing` (FULL `Survival`); there is no
   `AVGuns` and no `AVSurvival`. Adding an `AV` prefix at the parser boundary fixes twelve
   FNV skills and leaves Survival permanently absent while populating
   `AVBigGuns` — FULL `"Big Guns - OBSOLETE"` — as a phantom skill.
3. **CHAR-D2-01** is FO4's version of the same class: a `push_derived` row keyed on an
   output `AVIF` the game does not author. The capture document predicted it in words
   ("not a standalone resource AV") and the builder registered it anyway.
4. **CHAR-D1-01** is the other direction — a *consumer* that should read CHARAL and
   doesn't. `byroredux/src/combat.rs` landed today with `UNARMED_DAMAGE = 8.0` and
   `EquippedWeapon.damage`, ignoring the shipped Melee Damage / Unarmed Damage derived
   rows. `DerivedOutput::Multiplier`'s docstring names "the combat / XP system" as its
   reader; that system now exists, and the enum still has **zero positive readers
   workspace-wide**.

### The negative results, which are real

Everything the first audit verified about the *numbers* still holds. I re-checked and found
**no coefficient, bias, cross term, cap or rounding-mode drift** in any shipped table, and
**no regression** in any of the twelve fixes landed since. Specifically re-verified as still
correct: the `DerivedScope` guard in `pool_regen_tick_system` (#2932), the
`ActorGeneral && Absolute` gate in `GetActorValue` (#2933), the de-duplicated SPECIAL
roster (#2934), `derived_row_len` (#2935), `effective_actor_level`'s ACBS `calcMin`
routing (#2955), and the whole reputation/regen/affliction constant set (#2948–#2954).

Dimension 1's doctrine check is clean for the second time: no per-game branch in any
`CharacterRuleset` consumer, one construction sink, no bypass writing derived stats
straight into `ActorValues`. Dimensions 3 and 4 produced **zero** new findings.

### Verification honesty

- **Verified against capture documents**: FO4, FNV, FO3, Oblivion, Skyrim (coefficients —
  re-spot-check of the 2026-08-15 26-row table; unchanged).
- **Verified against real game data this session** (new this audit): the `AVIF` EditorID
  space of `FalloutNV.esm`, `Fallout3.esm`, `Fallout4.esm` and `Skyrim.esm`, using the
  repository's own parser via `crates/plugin/examples/dump_record_subs.rs`.
- **NOT verified**: FO76 and Starfield — still no builder, nothing to diff.
- **Not re-derived**: the leveling constants' circular sourcing (#2945) is unchanged and
  still not independently confirmed.

---

## Constant Verification Table (this session's additions)

Only rows newly checked against **game data** are listed; the 26-row coefficient table from
2026-08-15 was re-spot-checked and is unchanged (25 PASS, 1 UNSOURCED, 0 mismatches).

| # | Constant / lookup | Code value | Game-data value | Source | Verdict |
|---|---|---|---|---|---|
| 27 | FO4 Melee Damage output key | `resolve("MeleeDamage")` | no `AVIF` whose EDID contains `Melee` exists in `Fallout4.esm` | real-parser probe; `charal-fo4-ruleset.md` § Melee Damage ("not a standalone resource AV") | **FAIL** → CHAR-D2-01 |
| 28 | FO4 Health / AP / Carry Weight / SPECIAL keys | `"Health"`, `"ActionPoints"`, `"CarryWeight"`, `"Strength"`, `"Agility"`, `"Endurance"` | `Health` 0x2D4, `ActionPoints` 0x2D5, `CarryWeight` 0x2DC, `Strength` 0x2C2, `Agility` 0x2C7, `Endurance` 0x2C4 | real-parser probe on `Fallout4.esm` | **PASS** |
| 29 | FNV skill key `"Guns"` | `SkillDef::governed("Guns", Agility)` | no `AVGuns`; `AVSmallGuns` 0x4B9, FULL `"Guns"` | real-parser probe on `FalloutNV.esm` | **FAIL** → CHAR-D2-02 |
| 30 | FNV skill key `"Survival"` | `SkillDef::governed("Survival", Endurance)` | no `AVSurvival`; `AVThrowing` 0x4BC, FULL `"Survival"` | real-parser probe on `FalloutNV.esm` | **FAIL** → CHAR-D2-02 |
| 31 | FNV skill key `"BigGuns"` | `SkillDef::governed("BigGuns", Endurance)` | `AVBigGuns` 0x4B1, FULL `"Big Guns - OBSOLETE"` | real-parser probe on `FalloutNV.esm` | **FAIL** (resolves to a retired AV) → CHAR-D2-02 |
| 32 | Skyrim derived-row output/input keys | `"DamageResist"`, `"LightArmor"`, `"CarryWeight"`, `"Stamina"` | `AVDamageResist` 0x5CE, `AVLightArmor` 0x452, `AVCarryWeight` 0x3F0, `AVStamina` 0x3EA | real-parser probe on `Skyrim.esm` | **FAIL** — root cause is ESM-2026-08-16-D7-01, not re-filed |
| 33 | FO3/FNV SPECIAL + skill keys (all 20) | bare `Attribute::editor_id()` / `SkillDef::editor_id` | all `AV`-prefixed (`AVStrength` 0x3E8, `AVHealth` 0x450, …) | real-parser probe on both masters | **FAIL** — ESM-2026-08-16-D7-01, not re-filed |
| 34 | `UNARMED_DAMAGE` (combat slice) | `8.0` | no capture document states it; the FO3/FNV rule is `ceil((10 + Unarmed)/20)` | `charal-fnv-fo3-ruleset.md` § Unarmed Damage | **UNSOURCED** (acknowledged placeholder) → CHAR-D1-01 |

---

## Coverage Matrix (corrected against game data)

| Game | Capture doc | Builder exists | Ruleset **wired** | Derived rows *in code* | Derived rows **on vanilla data** | Leveling model | Regen wired | Affliction wired |
|---|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| **Oblivion** | `charal-oblivion-ruleset.md` | ✓ | ✗ (`None`) | 8 (5 stats) | n/a (pre-`AVIF` family; needs the legacy-index resolver the roster docs describe) | ✓ `OBLIVION` | ~ builder exists, no callers | ✗ |
| **FO3** | `charal-fnv-fo3-ruleset.md` | ✓ | ~ shadowed by FNV (#2941) | 8 | **0** | ~ `FO3` (unreachable) | ✗ | ✗ |
| **FNV** | `charal-fnv-fo3-ruleset.md` | ✓ | ✓ | 8 | **0** | ✓ `FNV` | ✗ | ✗ |
| **Skyrim SE** | `charal-skyrim-ruleset.md` | ✓ | ✗ (`None`) | 2 | **0** | ✓ `SKYRIM` | ✗ | ✗ |
| **FO4** | `charal-fo4-ruleset.md` | ✓ | ✓ | 4 | **3** (no `MeleeDamage` `AVIF`) | ✓ `FO4` | ✗ | ✗ |
| **FO76** | `charal-fo76-ruleset.md` | ✗ | ✗ | — | — | ✗ | ✗ | ✗ |
| **Starfield** | `charal-starfield-ruleset.md` | ✗ (blocked, noted) | ✗ | — | — | ✗ | ✗ | ✗ |

**Reading**: the "derived rows in code" column is what the 2026-08-15 matrix measured. The
column beside it is what an actor actually gets. The gap between the two columns is this
audit's whole contribution.

---

## Findings

### CHAR-2026-08-16-D1-01: The P2 melee slice is a CHARAL non-consumer — flat damage constants, and `DerivedOutput::Multiplier` still has no reader

- **Severity**: MEDIUM
- **Dimension**: Ruleset Seam & CHARAL Doctrine
- **Game**: all (FO4 + FO3NV are the wired ones; the slice itself gates on Skyrim)
- **Location**: `byroredux/src/combat.rs:30` (`UNARMED_DAMAGE`), `:269-273`
  (`attack_damage`), `:203-207` (the damage call); against
  `crates/core/src/character/derived.rs:99-103` (`DerivedOutput::Multiplier`),
  `crates/core/src/character/fallout.rs:45-47` + `:55-58` (FO3/FNV Melee + Unarmed Damage
  rows) and `:98-104` (FO4 Melee Damage multiplier)
- **Status**: NEW
- **Source**: `docs/engine/charal-fnv-fo3-ruleset.md` § Melee Damage — `STR × 0.5`,
  "an **additive** bonus"; § Unarmed Damage — `ceil((10 + Unarmed)/20)`.
  `docs/engine/charal-fo4-ruleset.md` § Melee Damage — LOCKED (multiplier, actor-general),
  `1 + Strength × 0.1`. `crates/core/src/character/derived.rs:99-103`: *"`eval` returns the
  multiplier; the combat / XP system multiplies."*
- **Description**: The first combat consumer in the engine landed on 2026-08-16
  (`eb5d76fe`) and does not read CHARAL. `attack_damage` is the whole damage model:

  ```rust
  fn attack_damage(world: &World, aggressor: EntityId) -> f32 {
      world
          .get::<EquippedWeapon>(aggressor)
          .map_or(UNARMED_DAMAGE, |weapon| weapon.damage.max(0.0))
  }
  ```

  Neither branch consults the aggressor's `ActorValues`, its `CharacterRuleset`, or any
  derived row. Both wired games ship a Melee Damage formula — FO3/FNV as an `Absolute`
  additive bonus, FO4 as a `Multiplier` — and FO3/FNV additionally ship an Unarmed Damage
  formula that is *exactly* the quantity the `UNARMED_DAMAGE` constant substitutes for.
  The constant's own comment admits it is a placeholder ("one explicit unarmed damage rule
  instead of inventing an item/equipment record"), which is honest, but `8.0` appears in no
  capture document for any game.

  The sharper half is `DerivedOutput::Multiplier`. #2933 (CLOSED) removed its one *wrong*
  reader by making `GetActorValue` skip multiplier rows, on the stated grounds that a
  multiplier belongs to the combat consumer. `grep DerivedOutput` outside
  `crates/core/src/character/` now returns exactly one site —
  `crates/scripting/src/condition.rs:463`, and only to exclude it. The enum arm the type
  system says is handled has **zero positive readers workspace-wide**, and the system it
  was waiting for has now shipped without connecting to it.
- **Evidence**: `grep -rn "DerivedOutput" --include="*.rs" .` outside
  `crates/core/src/character/` → `crates/scripting/src/condition.rs:426` (import) and
  `:463` (`&& formula.kind == DerivedOutput::Absolute`). No call to `derived_value` exists
  anywhere in `byroredux/src/combat.rs`; the file does not import
  `byroredux_core::character` at all.
- **Impact**: Every melee hit in the engine deals a flat authored-weapon or flat-8 number,
  independent of the attacker's Strength on every game. Nothing is *wrongly* computed — the
  formulas simply never run — so no test fails and no log line appears. Blast radius today
  is bounded because `EquippedWeapon.damage` is a reasonable stand-in, but it means the
  P2 combat gate certifies a damage path that will have to change shape (not just gain a
  constant) when CHARAL is connected. `crates/core/src/combat.rs`'s capture-sourced
  Oblivion damage helpers (#2962) also still have zero callers, so the engine now has two
  unconnected combat-math modules and one connected one that uses neither.
- **Related**: #2933 (CLOSED — removed the wrong reader, anticipated this one); #2962
  (OPEN — `crates/core/src/combat.rs` / `stealth.rs` unowned); CHAR-D2-01 (the FO4 Melee
  Damage row this would have consumed).
- **Suggested Fix**: Route `attack_damage` through the `CharacterRuleset` when one is
  present: add the `Absolute` Melee Damage row for FO3/FNV, apply the `Multiplier` row to
  the weapon base for FO4, and fall back to `EquippedWeapon.damage` only when no ruleset is
  loaded (which is Skyrim's case today, and is the honest reason the slice works). Keep
  `UNARMED_DAMAGE` as the no-ruleset fallback and say so in its docstring rather than
  presenting it as the unarmed rule.

---

### CHAR-2026-08-16-D2-01: `fallout4_ruleset` registers a Melee Damage row keyed on a `MeleeDamage` `AVIF` that vanilla `Fallout4.esm` does not author

- **Severity**: MEDIUM
- **Dimension**: Derived-Stat Formulas
- **Game**: fo4
- **Location**: `crates/core/src/character/fallout.rs:98-104` (the row),
  `:186-190` (`fo4_ruleset_evaluates_and_scopes`, which asserts `derived_row_len() == 4`)
- **Status**: NEW
- **Source**: `docs/engine/charal-fo4-ruleset.md` § "Melee Damage — LOCKED (multiplier,
  actor-general)": *"not an additive bonus, and **not a standalone resource AV**"*. The
  document says the quantity is not an actor value; the builder registers it as one anyway.
- **Description**: `fallout4_ruleset` does
  `if let (Some(out), Some(s)) = (resolve("MeleeDamage"), strength)` and pushes
  `affine(av(s), 0.1, 1.0).as_multiplier()`. Vanilla `Fallout4.esm` contains **no `AVIF`
  record whose EditorID contains `Melee`** — so `resolve("MeleeDamage")` is `None` and the
  row is silently dropped by the resolve-or-skip contract on every real load. FO4's actual
  derived table is three rows (Health, Action Points, Carry Weight), not the four its own
  test pins.

  This is not the `AV`-prefix defect: FO4's `AVIF` EditorIDs *are* unprefixed and the other
  three keys resolve exactly as written. The record simply does not exist, which is what the
  capture document already said in prose.
- **Evidence**: `crates/plugin/examples/dump_record_subs.rs`, release path, against
  `Fallout4.esm`:
  ```
  $ … dump_record_subs Fallout4.esm AVIF Melee     → (no records)
  $ … dump_record_subs Fallout4.esm AVIF Damage    → HC_OutgoingDamageMult, HC_IncomingDamageMult,
        LGND_LessFallDamage, PADamageMult, …, UnarmedDamage (000002DF), DamageResist (000002E3)
  $ … dump_record_subs Fallout4.esm AVIF Health    → == Health (000002D4) ==
  $ … dump_record_subs Fallout4.esm AVIF CarryWeight → == CarryWeight (000002DC) ==
  ```
  The selector is a substring match (it finds `CarryWeight` from `Carry`), so the empty
  `Melee` result is a genuine absence, not a lookup artifact.
- **Impact**: The FO4 Melee Damage multiplier — the *only* `Multiplier`-kind row on a wired
  game, and the row CHAR-D1-01's combat consumer would need — can never be produced. Since
  #2933 also made the one live reader skip multiplier rows, the FO4 Melee Damage model is
  presently unreachable from both ends. The mismatch is invisible because the builder test
  supplies a synthetic resolver that invents the record (CHAR-D6-01).
- **Related**: CHAR-D6-01 (the fixture that hides it); CHAR-D1-01 (the consumer that would
  read it); #2933 (CLOSED).
- **Suggested Fix**: Decide which the FO4 melee multiplier is. If it is genuinely not an
  actor value, it does not belong in the `derived` table keyed by output `AVIF` — give it a
  named accessor on `CharacterRuleset` (or fold it into the combat consumer) and delete the
  row. Either way, correct `fo4_ruleset_evaluates_and_scopes`'s row-count assertion to the
  three rows that actually register, and drop `"MeleeDamage"` from the test's `full()`
  resolver so the fixture stops inventing vanilla content.

---

### CHAR-2026-08-16-D2-02: `SkillSet::FALLOUT_FO3_FNV` spells two FNV skills with their *display* names, not their `AVIF` EditorIDs — and resolves a retired one

- **Severity**: MEDIUM
- **Dimension**: Derived-Stat Formulas / Population Boundary
- **Game**: fnv (fo3 unaffected)
- **Location**: `crates/core/src/character/skill.rs:151-176`
  (`SkillSet::FALLOUT_FO3_FNV`) — specifically `:163` `"Guns"`, `:171` `"Survival"`,
  `:174` `"BigGuns"`, and the roster docstring at `:151-157`
- **Status**: NEW
- **Source**: vanilla `FalloutNV.esm` `AVIF` set (64 records), read with the repository's
  own parser. `docs/engine/charal-fnv-fo3-ruleset.md` § NPC stat storage flags the FO3/FNV
  actor-value wiring as a follow-up; it does not state the EditorIDs, so this row was
  previously **UNSOURCED** rather than wrong-per-document.
- **Description**: The roster's docstring states the union rule as *"FO3 has SmallGuns /
  BigGuns, FNV replaces them with Guns / Survival"*, and encodes `"Guns"` and `"Survival"`
  as EditorIDs on the strength of it. New Vegas did rename those skills — **in the display
  name (`FULL`), not the record identity (`EDID`)**. FNV reuses FO3's `AVIF` records:

  | FNV `AVIF` | FormID | `EDID` | `FULL` |
  |---|---|---|---|
  | Guns | `0x000004B9` | `AVSmallGuns` | `Guns` |
  | Survival | `0x000004BC` | `AVThrowing` | `Survival` |
  | (retired) | `0x000004B1` | `AVBigGuns` | `Big Guns - OBSOLETE` |

  There is no `AVGuns` and no `AVSurvival` record in `FalloutNV.esm`. This is a **separate
  defect from ESM-2026-08-16-D7-01** and survives its suggested fix: after the parser learns
  to try the `AV`-prefixed spelling, `resolve("Guns")` → `AVGuns` still misses,
  `resolve("Survival")` → `AVSurvival` still misses, and `resolve("BigGuns")` →
  `AVBigGuns` **hits**, registering the explicitly obsolete Big Guns actor value as a live
  Endurance-governed skill on every FNV actor.
- **Evidence**: `crates/plugin/examples/dump_record_subs.rs` against `FalloutNV.esm`, `FULL`
  payloads decoded from the dumped bytes:
  ```
  == AVSmallGuns (000004B9) ==   FULL = Guns
  == AVThrowing  (000004BC) ==   FULL = Survival
  == AVBigGuns   (000004B1) ==   FULL = Big Guns - OBSOLETE
  == AVMeleeWeapons (000004B6) == FULL = Melee Weapons
  ```
  A search for `Surv` over the `AVIF` group returns nothing; a search for `Guns` returns
  only `AVSmallGuns` and `AVBigGuns`.
- **Impact**: Latent behind ESM-2026-08-16-D7-01 today (nothing resolves at all), live the
  moment that lands. Then, per FNV actor: Guns populates correctly by accident — the roster
  governs `SmallGuns` by Agility, which is also FNV Guns' governor, so the auto-calc value
  is right — while **Survival never populates** (its only `AVIF` is `AVThrowing`, absent
  from the roster), and a phantom Big Guns skill is written at
  `2 + 2·END + ceil(Luck/2)`. `GetActorValue` on FNV Survival then returns the absent-AV
  default `0.0` for every CTDA, and a retired actor value carries a plausible non-zero
  number that dialogue and package conditions can read.
- **Related**: ESM-2026-08-16-D7-01 (`AV` prefix; fix these together or the second one
  hides behind the first); CHAR-D6-01 (the fixture that hides it — `fnv_index_with_class`
  in `actor_value_derive.rs:263-301` builds its `AVIF` set from the roster's own strings,
  and its sibling test's comment "SmallGuns/BigGuns absent here" is the tell).
- **Suggested Fix**: Key the FO3/FNV roster on the **record** identity that both games
  share — `SmallGuns` / `BigGuns` / `Throwing` — and carry the per-game display name
  separately if one is needed for the UI. Drop `"Guns"` / `"Survival"` as EditorIDs, add
  `Throwing` governed by Endurance (FNV Survival's governor), and mark `BigGuns` FO3-only
  so FNV does not resolve the obsolete record. Pin it with the real-data test CHAR-D6-01
  asks for.

---

### CHAR-2026-08-16-D6-01: Every CHARAL builder test supplies a resolver built from the roster's own strings — the fixtures cannot falsify the roster

- **Severity**: MEDIUM
- **Dimension**: Coverage, Documentation & Doctrine Drift
- **Game**: all
- **Location**: `crates/core/src/character/fallout.rs:162-181` (`full`),
  `crates/core/src/character/skyrim.rs:168-176` (`full`),
  `crates/core/src/character/tes.rs` (the Oblivion builder tests' stand-in resolvers),
  `crates/plugin/src/esm/records/actor_value_derive.rs:263-301` (`fnv_index_with_class`)
- **Status**: NEW
- **Description**: Every test that exercises a `*_ruleset` builder or the population path
  hands it a hand-written `Fn(&str) -> Option<u32>` whose match arms are the *same string
  literals the roster and the builders use*:

  ```rust
  fn full(id: &str) -> Option<u32> {
      Some(match id {
          "Strength" => 0x05,  "Endurance" => 0x07,  …  "MeleeDamage" => 0x2D2,  …
      })
  }
  ```

  The resolver is the component under test — it is the single point where an
  engine-supplied roster meets authored data — and the fixture reimplements it from the
  roster. So the tests answer "does the builder push a row when its key resolves?" and can
  never answer "does its key resolve?". Three of the five shipped builders produce an empty
  derived table against vanilla data and one produces a short one, with **98 green tests**
  and no `#[ignore]`d real-data test anywhere in `crates/core/src/character/`.

  This is the same defect class `/audit-esm` recorded for `derive_npc_actor_values`
  (ESM-2026-08-16-D7-01's "Why 693 green tests coexist with this" section), one layer up
  and inside CHARAL's own scope.
- **Evidence**: `grep -rn "fn full(" crates/core/src/character/` returns the FO4/FO3/FNV
  and Skyrim stand-in resolvers; `grep -rn "ignore" crates/core/src/character/` returns
  nothing. `fallout.rs:174` maps `"MeleeDamage" => 0x2D2`, a record `Fallout4.esm` does not
  contain (CHAR-D2-01); `actor_value_derive.rs:270-289` builds an `AVIF` set spelled
  `"Barter"`, `"Guns"`, `"Science"` — none of which are FNV `EDID`s (CHAR-D2-02).
- **Impact**: This is the reason all three data-facing findings in this report went
  unnoticed by an audit that read every line of the same code the day before. It is a
  process finding, not a runtime one, but its blast radius is every future CHARAL
  constant: nothing in the current suite can catch a key that does not exist on disk.
- **Related**: CHAR-D2-01, CHAR-D2-02, ESM-2026-08-16-D7-01 (all three are instances).
- **Suggested Fix**: Add one opt-in real-data test per implemented family, gated the way
  the existing corpus baselines are (env-var path + `#[ignore]`), asserting that every
  EditorID the family's builder passes to `resolve` is `Some` against the vanilla master.
  It does not need to check values — existence is the whole gap. Keep the synthetic
  fixtures for the arithmetic; they are fine at that job.

---

## Cross-Audit Dedup

| Item | Disposition |
|---|---|
| `AVIF` EditorIDs `AV`-prefixed on FO3/FNV/Skyrim → no `ActorValues`/`ActorVitals`, FNV ruleset resolves empty, P2 melee cannot damage FO3/FNV actors | **Existing**: `docs/audits/AUDIT_ESM_2026-08-16.md` § ESM-2026-08-16-D7-01. Independently reproduced here; **not re-filed**. |
| `health_actor_value_key` returns engine enum `24` into a FormID-keyed map on a false premise | **Existing**: ESM-2026-08-16-D7-02. **Not re-filed**. |
| `pool_regen_tick_system` nested lock stack | **Existing**: #2153 (OPEN), routed to `/audit-concurrency`. |
| Component storage/shape of `CharacterLevel` / `Perks` / `FactionReputation` | `/audit-ecs`; no shape findings here. |
| `AVIF` / `CLAS` / `NPC_` sub-record decoding | `/audit-esm`. |
| `crates/core/src/combat.rs` + `stealth.rs` ownership | **Existing**: #2962 (OPEN). CHAR-D1-01 notes the new consumer uses neither, which is a different claim. |

Nineteen findings from 2026-08-15 remain OPEN and were re-checked as still-live but **not
re-filed**: #2936, #2937, #2938, #2939, #2940, #2941, #2942, #2943, #2944, #2945, #2946,
#2947, #2956, #2957, #2958, #2959, #2960, #2961, #2962.

Twelve were verified **fixed and not regressed**: #2932, #2933, #2934, #2935, #2948, #2949,
#2950, #2951, #2952, #2953, #2954, #2955.

---

## Known-Open Register (confirmed NOT re-filed)

| Deferred item | Status this audit |
|---|---|
| FNV/FO3 **tag-skill per-level** formula (undocumented) | Still absent, not fabricated. `base_skill` remains `2 + 2·gov + ceil(Luck × 0.5)` with no per-level term. Confirmed clean. |
| FO3↔FNV divergent **player** Health/AP | Not re-filed. `build_character_ruleset`'s docstring still records the master-name-disambiguation deferral. |
| **VATS runtime** (AP pool/regen, time-pause, limb health, hit-chance) | Not re-filed. Still formulas only. |
| `boot.rs` scheduler access declaration | Still OPEN as #2153; `/audit-concurrency`'s. |

---

## Disproved Candidates (investigated, not reported)

- **"`pool_regen_tick_system` drains the accumulator before checking `CharacterRuleset`, losing time."** True, but the drained time can only be lost when no ruleset exists, in which case nothing regenerates anyway. No observable effect; not a finding.
- **"`ActorValues` uses `std::collections::HashMap` on a 60 Hz path."** The hot-path hashing rule (#2923) is explicitly scoped to the per-frame render/skinning path; regen is not registered against a live config on any game today. Out of scope and not a violation as written.
- **"`combat_damage_system` re-computes damage instead of using the snapshot taken in `combat_input_system`."** Both calls hit the same `attack_damage` in the same frame with the same aggressor; no divergence is reachable. A real duplication, but no defect — and it belongs to the un-owned gameplay slice, not CHARAL.
- **"Dead actors keep regenerating."** `pool_regen_tick_system` iterates every `ActorValues` holder with no `Dead` filter, but it only touches Fatigue and Magicka, neither of which any wired game configures. Latent at most, and indistinguishable from the broader "regen is unwired" state already recorded in #2950's closure.
- **"`Perks::set_rank` accepts out-of-range ranks."** Real, and already OPEN as #2944.
- **"`derived_value` still does not enforce `scope`/`kind`."** By design — it is a documented caller contract, and both live consumers now honour it (#2932/#2933). Not a finding.
- **"Skyrim's `skyrim_ruleset` keys are wrong."** Real, but the root cause is the `AV` prefix, i.e. ESM-2026-08-16-D7-01. Folded into the coverage matrix rather than double-filed.

---

## Suggested Fix Order

1. **ESM-2026-08-16-D7-01** (not mine, but everything else is behind it) — the `AV` prefix.
2. **CHAR-D2-02** — land with #1, or the roster's `Guns`/`Survival`/`BigGuns` spellings turn a fixed lookup into a silently wrong one.
3. **CHAR-D6-01** — the real-data existence test; it is what stops #2 and #3 recurring.
4. **CHAR-D2-01** — decide whether FO4 Melee Damage is an actor value, then fix the row *and* its test's row count.
5. **CHAR-D1-01** — connect the combat consumer, once there is a resolvable ruleset for it to read.
