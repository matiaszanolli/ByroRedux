# Character / CHARAL Audit — 2026-08-27 — **Dimension 5 only** (Population Boundary)

**Scope**: `/audit-character` restricted to **Dimension 5: Population Boundary
(parse → ruleset → actor)**, `--depth deep`, all implemented families. Run as part of the
`streaming-deep` audit-suite preset. This is **not** a full CHARAL sweep: Dimensions 1
(Ruleset Seam), 2 (Derived Formulas), 3 (Leveling), 4 (Pools/Afflictions/Reputation) and 6
(Coverage & Doctrine) were **not** executed — the most recent full pass is
`docs/audits/AUDIT_CHARACTER_2026-08-24.md`, and its constant-verification table and
coverage matrix are not reproduced or re-verified here.

**Repo state**: HEAD `7f78ad9d`, branch `main`, clean tree. The two commits the dispatch
named as prime regression candidates are `9e44a0dd` (Fix #3171 / #3172 — "one
effective-level rule at the record boundary, and CHARAL rosters falsified against every
shipped master") and `7f78ad9d` (issue metadata only — no code).

**Tests recorded** (read-only, no engine launch):

| Command | Result |
|---|---|
| `cargo test -p byroredux-core character` | **113 passed**, 0 failed |
| `cargo test -p byroredux-plugin actor_value` | **13 passed**, 0 failed (+1 `#[ignore]`d real-data test) |
| `cargo test -p byroredux npc_spawn` | **57 passed**, 0 failed, 1 ignored |

**Verification method**: static analysis of the population boundary, plus **independent
binary extraction from five shipped masters** with a from-scratch Python walker written
this session (`/tmp/audit/character/tplt_census.py`, `tplt2.py`) — `Skyrim.esm`,
`Fallout4.esm`, `FalloutNV.esm`, `Fallout3.esm`, and the sixteen FO3/FNV `.esm` headers.
Every count below is measured, not estimated. Nothing was launched; no `byroredux` process
was started.

| Dimension | Area | New findings |
|---|---|---|
| 5 | Population Boundary | **0 CRITICAL · 0 HIGH · 4 MEDIUM · 0 LOW** |

---

## Executive Summary

### The named regression candidates are clean

`9e44a0dd`'s #3171 fix is **verified correct and complete**. `effective_actor_level` now
has exactly one definition (`crates/plugin/src/esm/records/actor/mod.rs:96`);
`grep -rn "effective_actor_level\|effective_npc_level"` returns no second copy anywhere in
the workspace. Both call sides import it, and the two numbers that used to disagree now
agree by construction: the Health curve's level term
(`actor_value_derive.rs:261`) and the value `CharacterLevel` receives
(`npc_spawn.rs:165`) are the same function applied to the same resolved record. The
`.max(0)` non-multiplier branch that `#3081` had settled is the one that survived. The
placement at the record boundary is the right one — the rule decodes `NpcRecord::level`,
so it belongs beside `NpcRecord`.

`9e44a0dd`'s #3172 half (the `ROSTER_CASES` / `assert_rosters_resolve` real-data loop over
five masters) is likewise a genuine falsifiability improvement and was not found to have
introduced anything.

**Two ancillary observations**, neither a finding:

- The dispatch's scope pointer — "`build_character_ruleset` … documented as FO4 + FO3NV
  only today" — is **stale**. `CharacterRulesProfile` (`crates/core/src/character/profile.rs`)
  now carries **three** distinct ruleset builders: `Fallout3`, `FalloutNewVegas`, and
  `Fallout4`. FO3 and FNV are no longer collapsed (that was CHAR-D5-03 of the 2026-08-15
  sweep, since fixed). Oblivion / Skyrim / FO76 / Starfield remain `RulesetBuilder::None`.
  Skyrim is the interesting middle case: `NpcStatModel::RaceBaseOffsets` **is** wired, so
  Skyrim NPCs receive `ActorValues` while having no `CharacterRuleset` at all.
- **#3219 is fixed on `main` but still OPEN** in the cached issue snapshot. TES5
  `RACE.DATA` starting Magicka (@40) and Stamina (@44) *are* parsed
  (`crates/plugin/src/esm/records/actor/mod.rs:1328-1335`) and *are* consumed
  (`derive_skyrim_actor_values`). This is issue hygiene, not code drift; noted here so a
  future auditor does not re-file it.

### What Dimension 5 actually turned up

Four MEDIUM findings, three of which share one root shape: **`derive_npc_actor_values` is
a three-arm match, and only one arm resolves `TPLT` template inheritance.** #2956
established the rule for the FO3/FNV auto-calc arm (`resolve_inherited_stats` /
`resolve_inherited_traits`) and `stamp_character_components` applies it to
`CharacterLevel` / `Background` for **every** game. The Skyrim and FO4 arms of the same
dispatch read the shell record's fields directly.

That is not merely an omission — on Skyrim it produces a **same-entity contradiction**:
`Background.race_form_id` is written from the `Use Traits`-resolved record while the
Health/Magicka/Stamina values on the very same entity are derived from the shell's own
`RNAM` race. This is exactly the defect class #3171 just closed (two components describing
one actor derived from two different source records), at roughly 30× the blast radius.

The fourth finding is a different seam: a creature (`CREA`) actor reaches the identical
spawn tail and receives **no `ActorValues` at all**, because the auto-calc arm's class
lookup is fed a field that on `CREA` holds something that is not a class.

### Measured blast radius (all counts from shipped masters, this session)

| Game | Population | Affected | Share |
|---|---|---|---|
| Skyrim | 5,118 `NPC_` | **875** end up with a different (Health, Magicka, Stamina) triple than the template chain gives | 17.1 % |
| Skyrim | 3,651 `NPC_` with `TPLT` | 887 whose own `RNAM` differs from the `Use Traits` source; 534 of those resolve to a *different* `RACE.DATA` pool triple | — |
| FO4 | 3,015 `NPC_` | **1,222** whose own `PRPS` pair set differs from the `Use Stats` source; **1,201** whose baked `DNAM` Health/AP differs | 40.5 % |
| FO4 | 3,015 `NPC_` | 330 carry `DNAM` Calculated Health `== 0` → no Health AV → no `ActorVitals` | 10.9 % |
| FNV | 1,578 `CREA` | **1,578** receive zero `ActorValues` | 100 % |
| FO3 | 533 `CREA` | **533** receive zero `ActorValues` | 100 % |

---

## Constant / Data Verification Table (Dimension 5 only)

| Claim under test | Code | Measured / documented | Verdict |
|---|---|---|---|
| `effective_actor_level` has exactly one definition | `actor/mod.rs:96` | 1 definition, 0 copies (workspace grep) | **PASS** (#3171 fixed) |
| Health-curve level term == `CharacterLevel` level | `actor_value_derive.rs:261` vs `npc_spawn.rs:165` | same fn, same resolved record | **PASS** |
| `calc_min.max(1) as i16` cannot wrap | `actor/mod.rs:97` | max `calc_min` across FNV/FO3/Skyrim/FO4 = **100** | **PASS** (no vanilla trigger) |
| Raw ACBS `level` fits `i16` | `actor/mod.rs` ACBS arms | max raw level 8000 (FO4), 4000 (FO3), 2000 (FNV/Skyrim) | **PASS** |
| FO3 ↔ FNV profile split threshold `HEDR < 1.0` | `records/mod.rs:146-147` | all 6 FO3 masters `0.94`; all 10 FNV masters `1.32`–`1.34` | **PASS** on every shipped master |
| Derived rows are resolve-or-skip, never keyed on `0` | `fallout.rs:48,52,63,70,79,130,139,146,159,165,198,204` | every site is `if let (Some(out), Some(..))` | **PASS** |
| AVIF resolution rejects sentinel FormIDs | `index.rs` `actor_value_form_id` | `avif.form_id != 0 && != u32::MAX` | **PASS** |
| FO4 `PRPS` AVIF FormIDs are in global space | `actor/mod.rs:1137` | `remap_fid(avif, remap)` | **PASS** |
| Skyrim pools degrade per-pool, not all-or-nothing | `actor_value_derive.rs:183-198` | per-pool `zip` + `continue`; no shared early return | **PASS** |
| `setav`/`modav` write the base, not a derived output | `commands/actor_value.rs:84-87` | `set_base` / `mod_permanent` | **PASS** |
| The only `build_character_ruleset` caller handles `None` | `cell_loader/references/mod.rs:276` | `if let Some(rs)`; no default inserted | **PASS** |
| FNV/FO3 tag-skill per-level term still absent (not guessed) | `actor_value_derive.rs:38-46` | deferral note intact, no fabricated term | **PASS** (known-open) |
| Skyrim NPC pools = race base + per-NPC adjustment | `derive_skyrim_actor_values` | `charal-skyrim-ruleset.md:603-605` | **PASS** (shape) |
| **Template source for the Skyrim/FO4 arms** | `actor_value_derive.rs:167-176` | shell record only — see D5-01 / D5-02 | **FAIL** |
| **Creature actor-value source** | `actor_value_derive.rs:172-174` | `CREA.CNAM` resolves to CLAS **0/2,111** times | **FAIL** |
| **`character_rules` survives a failed plugin parse** | `index.rs:830` | unconditional last-wins overwrite | **FAIL** |

---

## Findings

### CHAR-2026-08-27-D5-01: the Skyrim actor-value arm reads the shell `NPC_`, never the `TPLT` source — and contradicts `Background` on the same entity

- **Severity**: MEDIUM
- **Dimension**: Population Boundary
- **Game**: skyrim
- **Location**: `crates/plugin/src/esm/records/actor_value_derive.rs:167-176`
  (`derive_npc_actor_values`) and `:180-201` (`derive_skyrim_actor_values`), against
  `byroredux/src/npc_spawn.rs:150-171` (`stamp_character_components`)
- **Status**: NEW
- **Source**: `docs/engine/charal-fo4-ruleset.md:524-526` — *"**`TPLT` + ACBS Template
  Flags** — if 'Use Stats' is set, inherit SPECIAL / level / etc. from the template
  `NPC_`/`LVLN`"* — the inheritance chain the FO3/FNV arm already implements via
  `crates/plugin/src/equip.rs:257-278` (`resolve_inherited_stats` /
  `resolve_inherited_traits`, #2956). `docs/engine/charal-skyrim-ruleset.md:603-605`
  supplies the composition shape (*"race base … + per-NPC fixed adjustment"*) — i.e. both
  operands of the Skyrim formula are exactly the two fields the template flags govern.
  Population counts measured this session from `Skyrim.esm`.
- **Description**: `derive_npc_actor_values` is a four-way match on `NpcStatModel`. The
  `ClassAutoCalc` arm resolves `TPLT` first:

  ```rust
  NpcStatModel::ClassAutoCalc { health } => {
      let stats_npc =
          crate::equip::resolve_inherited_stats(npc, effective_actor_level(npc), index);
      derive_autocalc_actor_values(stats_npc, index, index.character_rules, health)
  }
  ```

  The `RaceBaseOffsets` (Skyrim) arm does not:

  ```rust
  NpcStatModel::RaceBaseOffsets => derive_skyrim_actor_values(npc, index),
  ```

  and `derive_skyrim_actor_values` then reads **both** operands off the shell:
  `index.races.get(&npc.race_form_id)` for the race base, and
  `npc.health_offset` / `npc.magicka_offset` / `npc.stamina_offset` for the per-NPC
  adjustment. `RNAM` is `Use Traits` data; the three ACBS offsets are `Use Stats` data.
  Both flags are parsed for Skyrim (`actor/mod.rs:939-949`, `template_flags` at ACBS
  offset 18) and both are honoured elsewhere in the same spawn tail.

  The result is not just "possibly the wrong number" — it is an internal contradiction.
  `stamp_character_components`, twenty lines away in `npc_spawn.rs`, writes:

  ```rust
  let traits_npc = resolve_inherited_traits(npc, shell_level, index);
  …
  Background { race_form_id: traits_npc.race_form_id, … }
  ```

  So on 887 Skyrim actors the entity's `Background` declares one race while its
  `ActorValues` Health/Magicka/Stamina were computed from a different one. A third site,
  `build_npc_equip_state` (`npc_spawn.rs:788`), uses the shell's `npc.race_form_id` again
  for the `RACE.WNAM` default skin — three sites, two conventions, no test pinning either.
- **Evidence**: independent walk of `Skyrim.esm` (`/tmp/audit/character/tplt2.py`,
  TES5 24-byte record headers, zlib-inflating compressed records, ACBS offsets read at the
  same byte positions the Rust parser uses; `RACE.DATA` H/M/S read as `f32` @ 36/40/44,
  matching `actor/mod.rs:1324-1335`):

  ```
  skyrim: NPC_ total=5118  with TPLT=3651  UseStats=3182  UseTraits=2053
  skyrim: UseTraits resolvable=1874, own RNAM differs from template=887,
          and their RACE.DATA (H,M,S) triple differs=534
  skyrim: UseStats  resolvable=2970, own (H,M,S) offsets differ from template=671
  skyrim: FINAL computed (H,M,S) differs from what the code produces = 875 / 5118
  ```

  The TPLT walk used in that script mirrors `resolve_inherited_record`'s own contract
  (flag-gated chain, depth cap 6, `LVLN` highest-eligible pick).
- **Impact**: 875 of 5,118 vanilla Skyrim actors (17.1 %) are seeded with the wrong
  Health / Magicka / Stamina. Because Health is what `stamp_actor_values` keys
  `ActorVitals` on, this is also the number combat, drowning damage
  (`systems/water.rs`), and every `GetActorValue` CTDA read against. Silent — no log line,
  no failing test, and the two contradicting components both look plausible in isolation.
  The affected population is exactly the templated `Enc*` encounter actors, i.e. the ones
  the player actually fights.
- **Related**: #2956 (CLOSED — established the rule for the FO3/FNV arm only); #3171
  (CLOSED — same defect class, `ActorValues` and `CharacterLevel` derived from different
  source records, 30 actors); CHAR-D5-02 of the 2026-08-15 sweep (the FO3/FNV original of
  this finding).
- **Suggested Fix**: hoist the template resolution above the match in
  `derive_npc_actor_values` — resolve `stats_npc` (`Use Stats`) and `traits_npc`
  (`Use Traits`) once and pass both down, so `derive_skyrim_actor_values` takes its race
  from the traits source and its three offsets from the stats source. That also removes the
  duplicated chain walk `stamp_character_components` performs separately. Pin it with a
  test asserting that `Background.race_form_id` and the race whose `RACE.DATA` fed
  `ActorValues` are the same FormID for a templated actor — the invariant that is currently
  unrepresented.

---

### CHAR-2026-08-27-D5-02: the FO4 stored actor-value arm has the same un-resolved-template gap, and `charal.md`'s recorded open item mis-scopes it

- **Severity**: MEDIUM
- **Dimension**: Population Boundary
- **Game**: fo4
- **Location**: `crates/plugin/src/esm/records/actor_value_derive.rs:167-176`
  (`derive_npc_actor_values`, the `NpcStatModel::Stored` arm) and `:208-224`
  (`derive_stored_actor_values`)
- **Status**: NEW
- **Source**: `docs/engine/charal-fo4-ruleset.md:519-529` — the explicit three-step
  inheritance chain: `RACE.PRPS` base → **`TPLT` + ACBS Template Flags ("Use Stats")** →
  `NPC_.PRPS` own overrides. Counts measured this session from `Fallout4.esm`.
- **Description**: identical structural gap to D5-01 on the other un-resolved arm.
  `derive_stored_actor_values(npc, index)` reads `npc.actor_value_props` (`PRPS`) and
  `npc.calculated_health` / `npc.calculated_action_points` (`DNAM`) straight off the shell
  record, with no `resolve_inherited_stats` call, while `stamp_character_components` on the
  same entity resolves `Use Stats` for `CharacterLevel` and `Background.class_form_id`.
  FO4 `template_flags` is parsed (`actor/mod.rs:919-927`, ACBS offset 14) and is used by
  the equip path, so the data is present and consumed elsewhere.

  There is a second, documentation-side half. `docs/engine/charal.md:568-570` records the
  remaining FO4 gap as *"the **`RACE`/template inheritance fallback** for NPCs that author
  no `PRPS` pairs of their own"*, and `:602-606` repeats it. That framing is falsified by
  the data: **0 of 3,015** vanilla FO4 `NPC_` records lack `PRPS`. The real gap is not a
  fallback for PRPS-less NPCs — it is a precedence question for the 1,222 shells that
  author a `PRPS` set differing from their `Use Stats` template's. A future reader working
  from `charal.md` would look for a population that does not exist and conclude the item is
  moot.
- **Evidence**: independent walk of `Fallout4.esm` (same script family; FO4 ACBS
  `template_flags` @ 14, `PRPS` as `(u32, f32)` pairs, `DNAM` as two leading `u16`):

  ```
  fo4: NPC_ total=3015  with TPLT=2289  UseStats=1972
  fo4: UseStats resolvable=1952
       PRPS pair-set differs from template = 1222
       DNAM (Calculated Health, Action Points) differs from template = 1201
       shell DNAM empty while template has one = 37
  fo4: NPC_ with zero PRPS = 0 / 3015
  fo4: NPC_ with DNAM Calculated Health == 0 = 330 / 3015
  ```

  The 330 with `calculated_health == 0` matter because `derive_stored_actor_values` pushes
  the Health pair only `if baked > 0`, so those actors get no Health AV, hence no
  `ActorVitals`, hence `combat.rs`'s `resolve_actor_root` filters them out entirely — 37 of
  them have a template that carries a real baked Health.
- **Impact**: 1,222 of 3,015 vanilla FO4 actors (40.5 %) are seeded with a SPECIAL /
  actor-value set that is not the one the template chain specifies, and 1,201 with the
  wrong baked Health/Action Points. FO4 `Health` and `ActionPoints` derived rows are
  `player_only()` by design (`fallout.rs:130-144`), so there is no `CharacterRuleset`
  fallback to mask a wrong `DNAM` — the stored value is the only value. Additionally the
  documented open item points at an empty population, so the gap reads as closed.
- **Related**: D5-01 (same root, other arm); #2956; `docs/engine/charal.md:568-570` and
  `:602-606` (the mis-scoped note).
- **Suggested Fix**: the same hoist as D5-01 fixes the code half. Separately, correct
  `charal.md` §8 item 3 and §9 to describe the gap as *template precedence for NPCs whose
  own `PRPS`/`DNAM` disagree with their `Use Stats` source* rather than a fallback for
  NPCs authoring no `PRPS`, and record the 0/3,015 measurement so the framing cannot drift
  back.

---

### CHAR-2026-08-27-D5-03: every FO3/FNV creature receives zero `ActorValues` — `CREA.CNAM` is not a class, and there is no creature arm

- **Severity**: MEDIUM
- **Dimension**: Population Boundary
- **Game**: fnv, fo3
- **Location**: `crates/plugin/src/esm/records/actor_value_derive.rs:167-176`
  (`derive_npc_actor_values` — no creature arm) and `:230-236`
  (`derive_autocalc_actor_values`'s `index.classes.get(&npc.class_form_id)` gate); fed by
  `crates/plugin/src/esm/records/actor/mod.rs:812-815` (the shared `CNAM` arm) and
  `crates/plugin/src/esm/records/dispatch_actor.rs:42-53` (`CREA` parsed by `parse_npc`);
  consumed at `byroredux/src/npc_spawn.rs:90-114` (`stamp_actor_values`) and
  `byroredux/src/npc_spawn/resumable.rs:328` (`spawn_placement_root` runs **before** the
  `is_creature` early return at `:347`)
- **Status**: NEW
- **Source**: measured from `FalloutNV.esm` and `Fallout3.esm` this session — the CLAS/IPDS
  resolution census below is itself the source; no external claim about `CREA.CNAM`'s
  semantics is needed, because the decisive fact is that it resolves to a `CLAS` record
  zero times and to an `IPDS` record 990 times.
- **Description**: `CREA` records are parsed into the same `NpcRecord` shape as `NPC_`
  (`dispatch_actor.rs`, deliberately — #442/#2567), and placed creatures route through the
  identical spawn tail: `spawn_placement_root` calls `stamp_faction_ranks`,
  `stamp_actor_values`, `stamp_character_components` *before* `prepare_runtime_state`
  branches on `npc.is_creature`. So a creature is stamped with whatever
  `derive_npc_actor_values` returns.

  On FO3/FNV that lands in the `ClassAutoCalc` arm, whose first statement is:

  ```rust
  let Some(class) = index.classes.get(&npc.class_form_id) else {
      return Vec::new();
  };
  ```

  `class_form_id` is populated by the shared `CNAM` arm, which is correct for `NPC_` and
  wrong for `CREA`: on `CREA` that FormID names a different record type entirely. The
  lookup therefore misses for **100 %** of creatures, `derive_npc_actor_values` returns an
  empty `Vec`, `stamp_actor_values` early-returns on `pairs.is_empty()`, and the creature
  gets neither `ActorValues` nor `ActorVitals`.

  There is no creature arm anywhere in the dispatch, and `NpcStatModel` has no creature
  variant. The module docstring's list of empty-result cases
  (`actor_value_derive.rs:159-162`) names *"an FNV NPC whose class wasn't parsed"* — which
  reads as a rare parse failure, not as "the entire bestiary, by construction".
- **Evidence**: independent walk of both masters, resolving each `CREA`'s `CNAM` against
  the plugin's own `CLAS` and `IPDS` FormID sets:

  ```
  FNV: CREA=1578  CLAS records=74  IPDS=60
       CREA CNAM resolves to CLAS:    0
       CREA CNAM resolves to IPDS:  793
       CREA with no CNAM:           785
       NPC_=3816, NPC_ CNAM resolves to CLAS: 3816   (100 %)

  FO3: CREA=533   CLAS records=53  IPDS=41
       CREA CNAM resolves to CLAS:    0
       CREA CNAM resolves to IPDS:  197
       CREA with no CNAM:           336
       NPC_=1647, NPC_ CNAM resolves to CLAS: 1647   (100 %)
  ```

  The `NPC_` rows are the control: the field is unambiguously a class FormID there and
  unambiguously not one on `CREA`.

  Downstream consequence, traced in code: `combat.rs:305-315` (`resolve_actor_root`)
  ends with `.filter(|actor| world.get::<ActorVitals>(*actor).is_some())`, and
  `stamp_actor_values` only inserts `ActorVitals` when the derived pairs contain the Health
  AVIF. A melee ray that lands on a creature's bone collider therefore records
  `"first obstruction is not an actor"` and emits no `HitEvent`.
- **Impact**: all 1,578 FNV and 533 FO3 `CREA` base records — the entire bestiary
  (deathclaws, geckos, super mutants, robots, radroaches) — spawn with no actor values.
  Concretely: untargetable and unkillable by the P2 melee slice; every `GetActorValue` CTDA
  against a creature is a structural `0.0`, indistinguishable from a genuine zero; no
  `ActorVitals` for the save-delta path to track. A secondary effect:
  `stamp_character_components` still writes `Background { class_form_id }` for creatures,
  so 990 creature entities carry an `IPDS` FormID in a field the component documents as a
  class.
- **Related**: #3004 (CLOSED — the `NPC_` half of "actors are not damageable"; creatures
  were never in its scope); #2567 (the commit that routed creatures into this spawn tail);
  #3305 (OPEN, renderer-side creature issue — unrelated mechanism).
- **Suggested Fix**: two independent steps. (1) Short-term, stop the mis-feed: do not
  populate `class_form_id` from `CNAM` when the record came from the `CREA` group — the
  one site that knows which group it read is `dispatch_actor.rs`, the same place
  `is_creature` is set. (2) Add a creature arm to `derive_npc_actor_values` sourced from
  `CREA`'s own `DATA` subrecord, whose field layout must be taken from the xEdit / fopdoc
  `CREA` definition rather than inferred — until that decode exists, the honest interim is
  an explicit, documented "creatures are unpopulated" note in the module docstring so the
  gap stops reading as a rare parse failure.

---

### CHAR-2026-08-27-D5-04: `EsmIndex::merge_from` adopts the last-merged index's `character_rules` unconditionally — including the empty index substituted when a plugin fails to parse

- **Severity**: MEDIUM
- **Dimension**: Population Boundary
- **Game**: all
- **Location**: `crates/plugin/src/esm/records/index.rs:829-830` (`merge_from`), reached
  from `byroredux/src/cell_loader/load_order.rs:285-290`; the value it overwrites is set at
  `crates/plugin/src/esm/records/mod.rs:194` via `character_rules_profile` (`:143-153`)
- **Status**: NEW
- **Source**: the FO3/FNV `HEDR < 1.0` discriminator is `records/mod.rs:146-147`; its
  robustness on shipped data was measured this session (all six FO3 masters `0.94`; all ten
  FNV masters `1.32`–`1.34`), which is why this is a latent seam rather than a live vanilla
  defect.
- **Description**: `character_rules` is the row that decides, for every actor in the load
  order, which skill roster is used, which Health curve seeds auto-calc NPCs, and which
  `CharacterRuleset` builder runs. `merge_from` takes it wholesale from whichever index was
  merged last:

  ```rust
  self.game = other.game;
  self.character_rules = other.character_rules;
  ```

  Two consequences follow, both silent.

  1. **Parse-failure erasure.** The load-order driver swallows a per-plugin parse failure
     and merges a default index instead:

     ```rust
     let plugin_records = esm::records::parse_esm_with_load_order(&bytes, Some(remap))
         .unwrap_or_else(|e| {
             log::warn!("Record parse failed for '{}': {}", path, e);
             esm::records::EsmIndex::default()
         });
     merged.merge_from(plugin_records);
     ```

     `EsmIndex::default()` carries `CharacterRulesProfile::NONE` (and
     `GameKind::default()` == `Fallout3NV`). If the *last* plugin in the order fails to
     parse, the merged index's profile becomes `NONE`, whose `npc_stat_model()` is
     `NpcStatModel::None` → `derive_npc_actor_values` returns `Vec::new()` for **every**
     actor in **every** cell, and `build_ruleset` returns `None` so no `CharacterRuleset`
     resource is ever inserted. The whole character layer switches off behind a single
     `log::warn!`. The `merge_from` docstring's own justification ("last-write-wins …
     multi-plugin loads always share a single game in practice") is about *plugins*, and
     does not contemplate the empty index the caller can hand it.
  2. **Profile flip.** Even on a successful parse, the FO3-vs-FNV split is decided solely
     by the last plugin's own `HEDR` float. Any last-loaded plugin authored with
     `HEDR < 1.0` on an FNV stack switches the entire load order to
     `CharacterRulesProfile::FALLOUT3` — a different 13-skill roster
     (`SkillSet::FALLOUT3` vs `SkillSet::FALLOUT_NV`) and a different Health curve
     (`90 + 20·END + 10·L` vs `95 + 20·END + 5·L`) for every actor.
- **Evidence**: code as quoted. The `HEDR` census that bounds risk (2):

  ```
  FNV masters: FalloutNV 1.34, GunRunnersArsenal 1.34, LonesomeRoad 1.34,
               OldWorldBlues 1.34, HonestHearts 1.33, CaravanPack/ClassicPack/
               DeadMoney/MercenaryPack/TribalPack 1.32
  FO3 masters: Fallout3, Anchorage, BrokenSteel, PointLookout, ThePitt, Zeta — all 0.94
  ```

  So no vanilla load order can trigger (2); it is reachable only through a third-party
  plugin. Trigger (1) needs no unusual data — only a plugin whose record walk errors.
- **Impact**: (1) is a total, silent loss of the character layer for the whole session,
  with the failure indication being a warn line about a *different* subject (the plugin
  parse). (2) mis-states every FO3/FNV actor's skills and Health. Both are the
  silent-wrong-constant class this audit exists for: no crash, no validation error, and no
  test can currently fail, because nothing asserts that the merged profile is a function of
  the *base master* rather than of whatever merged last. The `index.game` half of the same
  two lines has the same shape and a wider blast radius, but belongs to `/audit-esm`.
- **Related**: #2907 (the categories table that made category merging total — this pair of
  scalar fields sits outside it); D5-02 (also depends on `character_rules` selecting the
  right arm).
- **Suggested Fix**: make the overwrite conditional — keep `self.character_rules` when
  `other.character_rules` is `CharacterRulesProfile::NONE` (and `self`'s is not), which
  fixes (1) with one predicate. For (2), select the profile from the **first** plugin that
  yields a non-`NONE` row (the base master, which is what actually determines the game) and
  log at `warn` when a later plugin would have selected a different one, instead of
  adopting it silently. A test that merges a good FNV index followed by
  `EsmIndex::default()` and asserts the profile is still `FALLOUT_NEW_VEGAS` pins the whole
  class.

---

## Known-Open Register (restated, not re-filed)

| Deferred item | Status at HEAD |
|---|---|
| FNV/FO3 **tag-skill per-level** formula is undocumented and deliberately deferred | Confirmed still **absent, not guessed**. `actor_value_derive.rs:38-46` deferral note intact; `base_skill` implements only the sourced `2 + 2·gov + ceil(Luck·0.5)`. Not re-filed. |
| FO3↔FNV divergent **player** Health/AP deferred with the player actor | Still deferred. The FO3/FNV Health/AP rows are `player_only()` and the player carries no `ActorValues` at all (deliberate, `byroredux/src/scene.rs:1383-1392`). Not re-filed. |
| **VATS runtime** does not exist; only the AP formulas are in CHARAL | Unchanged. Not re-filed. |
| `fXPPerSkillRank` withdrawn 2026-08-24 as a settled design decision | Not re-flagged, per the skill's standing instruction. (#3221 remains OPEN in the tracker.) |
| Oblivion / FO76 / Starfield have no `RulesetBuilder` | Coverage information, restated: `CharacterRulesProfile::OBLIVION` is `NpcStatModel::None`, so **Oblivion actors receive no `ActorValues` at all**; FO76/Starfield are `Stored` but have no ruleset. Dimension 6 owns the full matrix; not filed as a bug. |

---

## Cross-Audit Dedup

- **Prior CHARAL Dimension-5 findings, all re-checked at HEAD**: CHAR-D5-01 (#2955,
  `CharacterLevel` from a PC-level multiplier) — **fixed**; CHAR-D5-02 (#2956, template
  flags ignored) — **fixed for FO3/FNV and for `CharacterLevel`/`Background` on all games**,
  which is precisely why D5-01/D5-02 above are the remaining halves; CHAR-D5-03 (FO3↔FNV
  collapse) — **fixed** by `CharacterRulesProfile`; CHAR-D5-04 (#2957, auto-calc deferral
  scale) — the corrected census is in the module docstring; CHAR-2026-08-20-D5-01 (#3171)
  — **fixed** by `9e44a0dd`. None re-filed.
- **`NPC_` / `CREA` / `AVIF` record decoding** (including the `CREA.DATA` layout D5-03's
  fix needs) → `/audit-esm` Dimension 4.
- **`EsmIndex::merge_from`'s `index.game` overwrite** (the sibling line to D5-04's
  `character_rules`) → `/audit-esm`.
- **`ActorVitals` / component storage shape** → `/audit-ecs`.
- **`resolve_armor_mesh` / race-skin selection using the shell race** (the third site named
  in D5-01's evidence) → `/audit-skyrim`, which already tracks #3357 in that area.
- **Combat targeting and the P2 melee slice** → the un-owned Gameplay Slice; D5-03's
  untargetable-creature consequence is reported here because its *cause* is at the CHARAL
  population boundary.
- **#3219** (TES5 RACE Magicka/Stamina) is fixed on `main` but still OPEN in the tracker —
  issue hygiene, not a finding.

---

## Not Covered

Dimensions 1, 2, 3, 4 and 6 were out of scope for this run — in particular **no
constant in `derived.rs`, `fallout.rs`, `tes.rs`, `skyrim.rs`, `leveling.rs`, `regen.rs`,
`affliction.rs`, `resistance.rs` or `reputation.rs` was re-verified against its capture
document this session**, nor were `crates/core/src/combat.rs` / `stealth.rs`. Those were
last verified in full on 2026-08-24. The four per-game formula values quoted above (FO3
`90 + 20·END + 10·L`, FNV `95 + 20·END + 5·L`, FO4 `floor(77.5 + 4.5·END + 2.5·L +
0.5·L·END)`) are cited only as context for the population path and were read, not
re-derived.

---

*Report generated 2026-08-27. Publish with:*

```
/audit-publish docs/audits/AUDIT_CHARACTER_2026-08-27.md
```
