# Character / CHARAL Audit — 2026-08-15

**Scope**: `/audit-character` — all 6 dimensions, all implemented families, `--depth deep`.
**This is the first audit this subsystem has ever had.** CHARAL had no owner skill
before 2026-08-13: `/audit-ecs` checked the *shape* of its components; nothing
checked its *numbers*.

**Repo state**: HEAD `c25f61e6`, branch `main`. Dedup baseline: 2832 issues
(263 OPEN). Test baseline: `cargo test -p byroredux-core character` — **94 passed,
0 failed** (unchanged throughout).

| Dimension | Area | Findings |
|---|---|---|
| 1 | Ruleset Seam & CHARAL Doctrine | 2 MEDIUM · 2 LOW |
| 2 | Derived-Stat Formulas | 2 MEDIUM · 4 LOW |
| 3 | Leveling & Progression | 3 MEDIUM · 5 LOW |
| 4 | Pools, Afflictions & Reputation | 1 MEDIUM · 6 LOW |
| 5 | Population Boundary | **1 HIGH** · 2 MEDIUM · 1 LOW |
| 6 | Coverage & Doctrine Drift | 4 MEDIUM · 1 LOW |
| **Total** | | **1 HIGH · 14 MEDIUM · 19 LOW (34)** |

---

## Executive Summary

**0 CRITICAL · 1 HIGH · 14 MEDIUM · 19 LOW.**

### The headline: the numbers are right; the wiring is not

This audit exists because a wrong constant in CHARAL changes gameplay silently —
no crash, no validation error, no failing test unless someone wrote one. So the
most important result is the negative one:

**Across 62 verified constants (26 in Dim 2, 36 in Dim 4), there were ZERO
numeric mismatches in any derived-stat formula, and 2 FAILs — both in
non-formula metadata (a band *name* and a doc cross-reference), not in a
coefficient.** Every bias, `c_a`, `c_b`, cross term, cap and rounding mode across
FO4 / FO3 / FNV / Oblivion / Skyrim matches its capture document.

Three traps the skill specifically named all came back clean:

- The cross term is non-zero in **exactly one** row (FO4 Health `0.5`), as the
  reference shape predicts — no stray cross terms anywhere else.
- `Floor` / `Ceil` appear **only where documented** — the "obviously it's round"
  guess did not happen.
- **The "cap `0` means uncapped" trap does not exist.** The sentinel is
  `f32::INFINITY`, no table passes `0.0`, and there is no `Default` derive that
  could zero it. Designed out, not survived by luck.

Dimension 1's doctrine check is equally clean: **no per-game branch in any
`CharacterRuleset` consumer**, one construction sink, no bypass writing derived
stats straight into `ActorValues`, AVIF keys confirmed remapped to global space,
and the engine-supplied / authored split holding in both directions (zero
hardcoded FormIDs outside tests).

For a never-audited layer whose whole premise is "data seam, not code seam,"
that is a strong result and should be read as such.

### What is actually wrong: ~60 % of shipped CHARAL is runtime-dead

The coverage matrix (Dim 6, reproduced below) is the artifact of this audit.
Five of seven games have complete, tested rulesets. **Only FO4 and FNV reach an
actor.** Oblivion and Skyrim are fully assembled with no construction site;
FO3's builder is shadowed by FNV's. Two whole subsystems are built but inert:
`regen` is registered in `boot.rs` yet permanently no-ops (`PoolRegenConfig`
has no insertion site anywhere in the workspace), and `affliction_tick_system`
is never registered at all and has no shipped table.

### The one HIGH — and why it is the one

**CHAR-D5-01: `CharacterLevel` is populated from a PC-level *multiplier*.**
`stamp_character_components` writes the raw ACBS level field verbatim. The
finding is backed by a probe over vanilla `FalloutNV.esm` / `Fallout3.esm`:
ACBS bit `0x0080` and `level > 100` select the *exact same* 268 FNV records
(188/197 on FO3), and the values are round steps 500…4000 — a fixed-point
multiplier, not a level. Two live consumers read it: `expand_leveled_form_id`
(an "actor_level" of 1000 makes every LVLI entry eligible → always the top gear
tier) and `GetXPForNextLevel` (150 050 instead of ~200). Source: the capture
document's level caps of 20/30.

This is HIGH rather than MEDIUM because it is **live on a wired game**, has two
real consumers, and its most visible symptom — every levelled NPC carrying
top-tier gear — reads as a content or balance problem rather than a parse bug.

### Findings cluster into four themes, not thirty-four problems

1. **Wiring / reachability** (D3-02, D5-03, D6-01…04, D4-03): builders, models
   and subsystems that exist, are correct, and are never called.
2. **Enum arms with no reader** (D1-02 / D2-01 `DerivedOutput::Multiplier`,
   D3-01 `Perks` vs `PerkList`, D3-04 `level_cap`): the type system says a case
   is handled; no consumer handles it. `Perks`/`PerkList` is the sharpest — two
   parallel perk components with an **empty intersection**, so perk checks can
   never succeed.
3. **Unit / convention ambiguity** (D2-02 fraction-vs-percentage, D2-06
   one-sided clamp, D4-05 REPU-vs-FACT keying): individually plausible rows
   that disagree with each other.
4. **Documentation drift** (D3-06, D3-07, D4-06, D4-07, D6-01…03): including
   `charal.md` still marked `Status: PROPOSED` while all four sibling layer docs
   (NIFAL / EXAL / PHYSAL / WATAL) read `ACTIVE`.

### A gap in the audit map itself

**CHAR-D6-05** is worth separating from the rest: `crates/core/src/combat.rs`
and `crates/core/src/stealth.rs` (~615 LOC) hold capture-sourced Oblivion damage
and FO3/FNV sneak constants, but sit **outside** this skill's declared
`crates/core/src/character/` scope. They were not verified by Dim 2 and are
owned by no audit skill. The scope line was drawn at a *directory* boundary
rather than a *subject-matter* one, and character rules living one directory up
fell through it. That is a finding about the audit suite, not only about the
code.

### Verification honesty

- **Verified against capture documents**: FO4, FNV, FO3, Oblivion, Skyrim.
- **NOT verified**: FO76 and Starfield — no ruleset builder exists for either,
  so there is nothing to check the documents against. FO76 is the only game
  whose capture is fully LOCKED with no builder; Starfield is correctly recorded
  as blocked on real PENDING data.
- **Circular sourcing** (D3-06): the Skyrim/Oblivion *leveling* constants are
  absent from their per-game captures and sourced only to `charal.md`'s
  implementation prose. Those rows passed only in the sense that code matches
  code — they are not independently confirmed.
- Dim 4 escalated past its documents where it could: for 9 unsourced reputation
  FormIDs it scanned `FalloutNV.esm` directly and confirmed all 13 are `REPU`
  records.

### Suggested fix order

1. **CHAR-D5-01** (HIGH) — live, two consumers, silently wrong loot tier.
2. **CHAR-D3-01** — `Perks` / `PerkList` empty intersection; perk checks cannot
   succeed today.
3. **CHAR-D1-02 / CHAR-D2-01** — `DerivedOutput::Multiplier` unread; FO4 Melee
   Damage surfaces a raw multiplier on a wired game.
4. **CHAR-D4-03** — the `boot.rs` comment that will actively mislead whoever
   wires regen (it names one prerequisite; there are two, and neither is
   inserted).
5. **CHAR-D6-05** — decide an owner for `combat.rs` / `stealth.rs` before their
   constants drift unaudited.
6. **CHAR-D3-02 / CHAR-D5-03** — FO3's shadowed ruleset (both halves).
7. Documentation cluster last, but `charal.md`'s `PROPOSED` status and the
   missing `regen` section are cheap and high-leverage.

---

## Known-Open Register (confirmed NOT re-filed)

All three deliberately-deferred items were excluded from every dimension and
none was re-reported:

| Deferred item | Status this audit |
|---|---|
| FNV/FO3 **tag-skill per-level** formula (undocumented) | Confirmed still *absent* rather than guessed — Dim 5 verified the base `2 + 2·gov + ceil(Luck/2)` is implemented and no fabricated per-level term ships. CHAR-D5-04 notes only that the deferral's *scale* is under-stated (40 % of FNV actors, not a tail). |
| FO3↔FNV divergent **player** Health/AP | Not re-filed. Dim 5 verified the *actor-general* parity claim independently (all six coefficients match), which is the half the collapse is justified on. |
| **VATS runtime** (AP pool/regen, time-pause, limb health, hit-chance) | Not re-filed as missing. Only the AP formulas are in CHARAL, as documented. |
| `boot.rs` scheduler access declaration | Already OPEN as **#2153**; routed to `/audit-concurrency`, not re-filed. |

---

## Coverage Matrix

| Game | Capture doc | Ruleset builder | Ruleset **wired** | Derived stats | Leveling model | Regen wired | Affliction wired |
|---|---|:---:|:---:|:---:|:---:|:---:|:---:|
| **Oblivion** | `charal-oblivion-ruleset.md` | ✓ `oblivion_ruleset` | ✗ (`build_character_ruleset` → `None`) | ✓ 8 rows / 5 stats | ✓ `LevelingModel::OBLIVION` | ~ config builder `oblivion_pool_regen_config` exists, **zero callers** | ✗ |
| **FO3** | `charal-fnv-fo3-ruleset.md` | ✓ `fallout3_ruleset` | ~ **unreachable** — `GameKind::Fallout3NV` → `falloutnv_ruleset` | ✓ 8 rows (unreachable) | ~ `LevelingModel::FO3` (unreachable) | ✗ | ✗ |
| **FNV** | `charal-fnv-fo3-ruleset.md` | ✓ `falloutnv_ruleset` | ✓ | ✓ 8 rows | ✓ `LevelingModel::FNV` | ✗ | ✗ |
| **Skyrim SE** | `charal-skyrim-ruleset.md` | ✓ `skyrim_ruleset` | ✗ (`None`) | ✓ 2 rows | ✓ `LevelingModel::SKYRIM` | ✗ | ✗ |
| **FO4** | `charal-fo4-ruleset.md` | ✓ `fallout4_ruleset` | ✓ | ✓ 4 rows | ✓ `LevelingModel::FO4` | ✗ | ✗ |
| **FO76** | `charal-fo76-ruleset.md` | ✗ | ✗ | ✗ | ✗ (`LevelReward::SpecialOrPerk` already claims FO76) | ✗ | ✗ |
| **Starfield** | `charal-starfield-ruleset.md` | ✗ (blocked, noted) | ✗ | ✗ | ✗ (blocked, noted) | ✗ | ✗ |
| *Morrowind* | — | — | — | — | — | — | — |


*(Reproduced from Dimension 6, which owns the matrix. `~` = present but
unreachable / partially wired.)*

**Reading**: five of seven games have a complete ruleset; two reach an actor.
FO76 is the only fully-LOCKED capture with no builder — its curve, reward shape
and 3 of 4 derived formulas already fit existing types. Starfield is correctly
blocked on PENDING data.

---

## Dimension 1



## Scope & Coverage

**Documents read (in the mandated order, before any Rust):**
1. `.claude/commands/_audit-common.md`
2. `.claude/commands/_audit-severity.md`
3. `.claude/commands/audit-character/SKILL.md` (Scope + Ground truth + Dimension 1)
4. `docs/engine/charal.md`
5. `docs/engine/charal-fo4-ruleset.md` (consulted for the Melee Damage output-kind
   semantics behind CHAR-D1-02)

**Code read:**
- `crates/core/src/character/ruleset.rs` (whole file — `CharacterRuleset`, `new`,
  `with_attributes`, `with_skills`, `with_derived`, `push_derived`,
  `derived_formula`, `derived_value`, `derived_len`)
- `crates/core/src/character/mod.rs` (whole re-export surface)
- `crates/core/src/character/attribute.rs`, `skill.rs` (rosters), `fallout.rs`,
  `tes.rs` + `skyrim.rs` (builder halves), `regen.rs` (whole file),
  `derived.rs` (`DerivedOutput` / `DerivedScope` / builder chain / `eval`)
- Every consumer found by `grep -rn "CharacterRuleset"` across the workspace:
  `byroredux/src/npc_spawn.rs` (`build_character_ruleset`),
  `byroredux/src/cell_loader/references/mod.rs` (`load_references_budgeted`),
  `byroredux/src/boot.rs` (the `add_exclusive_with_access` declaration),
  `crates/core/src/character/regen.rs` (`pool_regen_tick_system`),
  `crates/scripting/src/condition.rs` (`GetActorValue`, `GetXPForNextLevel`)
- Adjacent bypass candidates: `byroredux/src/commands/actor_value.rs`
  (`setav`/`modav`), `crates/plugin/src/esm/records/actor_value_derive.rs`
  (`derive_npc_actor_values`), `crates/plugin/src/esm/records/index.rs`
  (`actor_value_form_id`), `crates/plugin/src/esm/reader.rs`
  (`read_record_header` remap), `crates/plugin/src/esm/records/grup_walker.rs`
  (`extract_records`), `crates/plugin/src/esm/records/dispatch_misc_gameplay_b.rs`
  (the `AVIF` arm)

**Test baseline:** `cargo test -p byroredux-core character` → **94 passed, 0 failed**.

### Checklist items verified CLEAN

- **The doctrine check — no per-game branch in any `CharacterRuleset` consumer.**
  All five consumers were read end-to-end. `pool_regen_tick_system` and
  `condition.rs`'s `GetActorValue` / `GetXPForNextLevel` read the ruleset purely
  as data — no `GameKind`, no master-name compare, no `game ==`. The only
  `GameKind` match in the chain is inside `build_character_ruleset` itself (the
  sink) and in `byroredux/src/cell_loader/references/mod.rs`, which only *passes*
  `game` into that sink. **No CHARAL doctrine violation of the render-time-branch
  class exists.** (See CHAR-D1-03 for a weaker, structural variant.)
- **Single sink — construction.** The only production construction sites are
  `fallout4_ruleset` and `falloutnv_ruleset`, both reached exclusively through
  `build_character_ruleset`; the only production `insert_resource` of the type is
  the guarded block in `load_references_budgeted`. Every other
  `CharacterRuleset::new` call is inside `#[cfg(test)]`
  (`ruleset.rs`, `regen.rs`, `tes.rs`, `condition.rs`). `fallout3_ruleset`,
  `oblivion_ruleset` and `skyrim_ruleset` are compiled but **unreachable in
  production** — coverage information, already documented, not a finding here.
- **Single sink — no derived-stat bypass into `ActorValues`.** `setav`/`modav`
  (`edit_av`) write only `set_base` / `mod_permanent`, never a derived output.
  `derive_stored_actor_values` writes FO4's baked `DNAM` Health / Action Points,
  which is the documented FO4 *stored* model, not a bypass — and `GetActorValue`
  gives a carried value precedence over the formula, so there is no double count.
  The one genuine write-through-a-formula path is `pool_regen_tick_system`, which
  is CHAR-D1-01.
- **Derived-table N stays in the linear-scan range.** Row counts, measured from
  the builders and pinned by their own tests: FO4 **4**, FO3 **8**, FNV **8**,
  Oblivion **8** (5 distinct stats — Fatigue is 4 rows), Skyrim **2**. No game is
  anywhere near "dozens"; the flat-`Vec` rationale in `CharacterRuleset`'s
  docstring still holds for every implemented game.
- **Output keys are AVIF FormIDs in GLOBAL space.** Traced end to end: the
  builders' `resolve` is `EsmIndex::actor_value_form_id`, which returns
  `AvifRecord::form_id`; those records are inserted from the `b"AVIF"` arm of
  `dispatch_misc_gameplay_b.rs` via `extract_records`, whose `form_id` comes from
  `EsmReader::read_record_header` — which routes the raw id through
  `remap_form_id` (#445, guarded by
  `read_record_header_applies_installed_remap`). **Remapped, not plugin-local.**
- **ENGINE-SUPPLIED membership vs AUTHORED FormIDs — both directions.**
  `AttributeSet::{FALLOUT, TES_CLASSIC, SKYRIM, STARFIELD}` and
  `SkillSet::{OBLIVION, SKYRIM, FALLOUT_FO3_FNV, NONE}` are `&'static` const
  rosters carrying EditorIDs only; a repo-wide scan for hex literals in
  `crates/core/src/character/` finds **zero hardcoded FormIDs outside
  `#[cfg(test)]`**. Conversely, no roster length is derived from parsed data —
  `AttributeSet::len` / `SkillSet::len` read the const slice. The split holds
  both ways.
- **Multi-row `push_derived` contract** ("uncapped, unrounded, absolute rows").
  The only multi-row stat shipped is Oblivion Fatigue; `oblivion_fatigue_formulas`
  emits four `affine(av, 1.0, 0.0).player_only()` rows — uncapped, unrounded,
  `DerivedOutput::Absolute`, and uniform in scope, so `derived_formula`'s
  first-row scope read is not misleading. Contract currently honoured (it is
  unenforced, but no live violation exists, so it is not reported).

### Could NOT verify / out of this dimension

- Whether the *coefficients* in any per-game table match their capture documents
  — that is Dimension 2 by construction, and I deliberately did not open the
  five other capture docs.
- Whether `PoolRegenConfig`'s scheduler access declaration in `boot.rs` is
  complete — routed to `/audit-concurrency` Dim 4 per the skill's boundary rule
  (and already tracked OPEN as **#2153**).
- `mod.rs`'s module docstring omits `attribute`, `skill`, `fallout`, `tes`,
  `skyrim` from its enumeration. That is Dimension 6's explicit checklist item;
  routed there rather than reported here.
- Reachability/coverage of the unwired Oblivion / Skyrim rulesets — Dimension 3/5.

### Deduplication

`/tmp/audit/character/issues.json` (2832 issues, 263 OPEN) was scanned for
`charal / ruleset / actor value / derived / gmst / avif / special / skill /
character / perk / leveling / regen / affliction`. The only live CHARAL issue is
**#2153** (`pool_regen_tick_system` lock stack, OPEN) — a concurrency finding,
disjoint from all four below. `docs/audits/` was scanned for `CHARAL` /
`ActorValues` / `CharacterRuleset`: the hits are ECS (#1834/#1835 save-registry,
both CLOSED), CONCURRENCY, SAVE and per-game reports; none covers the ruleset
seam. No prior `AUDIT_CHARACTER_*` report exists. All four findings are **NEW**.

---

## Findings

### CHAR-D1-01: `pool_regen_tick_system` evaluates a `PlayerOnly` derived formula for every actor

- **Severity**: MEDIUM
- **Dimension**: Ruleset Seam
- **Game**: Oblivion (the only game whose regen config exists)
- **Location**: `crates/core/src/character/regen.rs:120-155` (`pool_regen_tick_system`), against `crates/core/src/character/tes.rs:44-48` (`oblivion_magicka_formula`)
- **Status**: NEW
- **Description**: `DerivedScope` exists precisely so a consumer can tell
  player-only stats from actor-general ones, and `CharacterRuleset::derived_value`
  deliberately does **not** enforce it — it is a caller contract. Of the two
  consumers that call it, `condition.rs` honours the contract
  (`if formula.scope == DerivedScope::ActorGeneral`), and
  `pool_regen_tick_system` does not: it calls
  `ruleset.derived_value(config.magicka_avif, avs, 1)` inside a loop over *every*
  entity carrying `ActorValues`, with no `derived_formula(...).scope` check.
  `oblivion_magicka_formula` — the only formula that can answer that lookup — is
  built `.player_only()`, on the stated grounds that NPCs ship baked pool values.
  So every NPC's Magicka regen rate is computed from the player-only
  `2 × Intelligence` max-pool formula rather than from the NPC's own baked base.
  The `.unwrap_or(0.0)` tail is the same gap seen from the other side: when no row
  produces the stat, the actor regenerates **nothing** even though it carries a
  populated Magicka pool, instead of falling back to its own base layer.
- **Evidence**:
  ```rust
  // regen.rs — pool_regen_tick_system
  for (_entity, avs) in avs_q.iter_mut() {
      ...
      if avs.get(config.magicka_avif).is_some() {
          let willpower = avs.current(config.willpower_avif);
          let max_magicka = ruleset
              .derived_value(config.magicka_avif, avs, 1)   // no scope check
              .unwrap_or(0.0);
  ```
  versus the honouring consumer in `crates/scripting/src/condition.rs`
  (`ConditionFunction::GetActorValue`):
  ```rust
  if let Some(formula) = rs.derived_formula(condition.param_1) {
      if formula.scope == DerivedScope::ActorGeneral { ... }
  }
  ```
- **Impact**: Wrong Magicka regeneration rate for every non-player actor in the
  TES family, silently — no crash, no failing test (the one system test,
  `tick_system_applies_fatigue_and_magicka_regen`, registers its stand-in Magicka
  row **without** `.player_only()`, so it cannot catch this). Blast radius is
  currently zero at runtime: `build_character_ruleset` returns `None` for
  Oblivion and nothing constructs `PoolRegenConfig` in production
  (`oblivion_pool_regen_config` has no production caller), so the path is latent.
  Severity is set on impact-when-wired, per `_audit-severity.md`'s opening rule —
  this fires the day Oblivion CHARAL wiring lands, which is exactly the event the
  `boot.rs` registration comment says the system is pre-registered for.
- **Related**: CHAR-D1-02 (the sibling contract, `DerivedOutput`, ignored by the
  other consumer). #2153 covers a different defect in the same system.
- **Suggested Fix**: Gate the lookup on
  `ruleset.derived_formula(config.magicka_avif).is_some_and(|f| f.scope == DerivedScope::ActorGeneral)`
  and fall back to the actor's own base layer for the max pool otherwise; add a
  regression test that registers the Magicka row with `.player_only()`. Longer
  term, consider a `derived_value_for_scope(avif, avs, level, scope)` accessor so
  the contract cannot be forgotten by the next consumer.

---

### CHAR-D1-02: Ruleset consumers ignore `DerivedOutput::Multiplier` — `GetActorValue` returns a raw multiplier for FO4 Melee Damage

- **Severity**: MEDIUM
- **Dimension**: Ruleset Seam
- **Game**: FO4 (live today); Oblivion latently (both armor-rating rows)
- **Location**: `crates/scripting/src/condition.rs` (`ConditionFunction::GetActorValue` arm, ~:415-453) against `crates/core/src/character/fallout.rs:98-104` (`fallout4_ruleset`, the Melee Damage row)
- **Status**: NEW
- **Description**: `DerivedStatFormula` carries **two** independent contract
  fields the ruleset does not enforce: `scope` (CHAR-D1-01) and `kind`
  (`DerivedOutput::{Absolute, Multiplier}`). `derived.rs` documents the
  multiplier kind as "`eval` returns the multiplier; the combat / XP system
  multiplies". `GetActorValue` is neither a combat nor an XP system, yet it
  returns `rs.derived_value(...)` verbatim after checking only `scope`. In
  `fallout4_ruleset`, Melee Damage is registered
  `affine(av(s), 0.1, 1.0).as_multiplier()` and is left **actor-general** (no
  `.player_only()`), so it passes the one check that is performed. A CTDA or
  console `cond <e> GetActorValue <MeleeDamage>` on an FO4 actor therefore
  evaluates to a bare ratio in `[1.0, 2.0]` where the game's own `GetActorValue`
  yields an actor-value reading. FO3/FNV are unaffected — their shared Melee
  Damage row is `affine(av(s), 0.5, 0.0)`, an absolute additive bonus. Oblivion's
  two `ARMOR_RATING_SKILL_COEFF` rows are the same shape as FO4's and are also
  actor-general, so they will behave identically once Oblivion is wired.
- **Evidence**: `fallout4_ruleset` —
  ```rust
  // Melee Damage = ×(1 + 0.1·STR).
  if let (Some(out), Some(s)) = (resolve("MeleeDamage"), strength) {
      rs.push_derived(out, DerivedStatFormula::affine(av(s), 0.1, 1.0).as_multiplier());
  }
  ```
  The consumer filters on `formula.scope` only; `formula.kind` is never read
  anywhere outside `derived.rs`'s own tests (`grep` for `DerivedOutput` /
  `as_multiplier` outside the crate returns no consumer).
- **Impact**: Any FO4 condition comparing Melee Damage silently compares against
  a multiplier — a plausible-looking small number that passes `> 0` gates and
  fails realistic threshold gates. Nothing crashes and no test fails; the FO4
  ruleset test asserts scopes but never asserts the kind reaching a consumer.
  Reachable **today** (FO4 is one of the two wired games).
- **Source**: `docs/engine/charal-fo4-ruleset.md:253-272` — "### Melee Damage —
  LOCKED (multiplier, actor-general)", `MeleeDamageMultiplier = 1 + Strength ×
  0.1`, "not an additive bonus, and **not a standalone resource AV**", and
  "Multiplier-kind formulas apply at combat/use time against a base (weapon
  damage)".
- **Related**: CHAR-D1-01 (same class: an unenforced `DerivedStatFormula`
  contract field dropped by a consumer).
- **Suggested Fix**: Have `GetActorValue` skip `DerivedOutput::Multiplier` rows
  (returning the absent-AV default, as it already does for player-only stats) and
  expose multiplier-kind stats through a distinct accessor for the combat/XP
  consumers that are meant to read them. Pin it with a test asserting
  `GetActorValue` on FO4 Melee Damage does not return `1 + 0.1·STR`.

---

### CHAR-D1-03: The FNV/FO3 skill auto-calc *rule* lives outside `CharacterRuleset` — spec's `skill_calc` field is absent and the SPECIAL roster is duplicated

- **Severity**: LOW
- **Dimension**: Ruleset Seam
- **Game**: FO3 / FNV
- **Location**: `crates/core/src/character/ruleset.rs` (`CharacterRuleset` — 4 fields) vs `crates/plugin/src/esm/records/actor_value_derive.rs` (`SPECIAL`, `special_index`, `SKILL_BASE`, `SKILL_ATTR_MULT`, `SKILL_LUCK_MULT`, `base_skill`, `derive_npc_actor_values`)
- **Status**: NEW
- **Description**: Two related structural gaps, both making a per-game character
  *rule* live somewhere other than the ruleset seam:
  1. `docs/engine/charal.md` §5 declares `CharacterRuleset` with a
     `skill_calc: SkillDerivation` field ("base / attr-mult / luck-mult (from
     GMST)"). The shipped struct has four fields — `attributes`, `skills`,
     `derived`, `leveling` — and **no `SkillDerivation` type exists anywhere in
     the workspace** (`grep -rn "SkillDerivation"` matches only the design doc).
     The three constants it was meant to hold are hardcoded `f32` literals in the
     parser crate, reached through `matches!(game, GameKind::Fallout3NV)`. So the
     FNV/FO3 half of the ruleset is a *code* seam in `crates/plugin`, not a data
     seam in `CharacterRuleset`. (`charal.md` §8 item 6 tracks the *GMST
     sourcing* half of this as open; the missing struct field and the seam's
     location are not covered there, and §2's tier table sanctions the plugin-side
     location, so the spec is internally inconsistent about where this belongs.)
  2. The same file re-declares the Fallout attribute roster as a local
     `const SPECIAL: [&str; 7]`, in the same order and with the same EditorIDs as
     the canonical `AttributeSet::FALLOUT`, plus a hardcoded `special[6]` for
     Luck. `SkillSet::FALLOUT_FO3_FNV`'s governing map was explicitly
     de-duplicated into CHARAL — `fallout.rs`'s module docs call it "the single
     source ... (no local duplicate)" — but the attribute roster beside it was
     not. Two engine-supplied copies of one roster, in two crates, with no test
     tying them together.
- **Evidence**:
  ```rust
  // crates/plugin/src/esm/records/actor_value_derive.rs
  const SPECIAL: [&str; 7] = ["Strength","Perception","Endurance","Charisma",
                              "Intelligence","Agility","Luck"];
  const SKILL_BASE: f32 = 2.0;       // fAVDSkill<name>Base
  const SKILL_ATTR_MULT: f32 = 2.0;  // fAVDSkillPrimaryBonusMult
  const SKILL_LUCK_MULT: f32 = 0.5;  // fAVDSkillLuckBonusMult
  ```
  `AttributeSet::FALLOUT.members()` is that exact list, in that exact order
  ("ordering is the canonical SPECIAL order").
- **Impact**: Maintainability / doctrine drift, not a live miscomputation — the
  two rosters agree today and the three constants match their cited geckwiki
  values. The cost is that the FNV/FO3 skill-derivation rule cannot be varied per
  game, cannot be fed from parsed GMSTs without editing parser code, and a second
  family adopting auto-calc adds another `GameKind` arm rather than another table
  row. A future reorder of `AttributeSet::FALLOUT` would leave the duplicate
  silently stale.
- **Source**: `docs/engine/charal.md:246-254` (the `CharacterRuleset` struct
  listing including `skill_calc: SkillDerivation`); `docs/engine/charal.md:530-535`
  (§8 item 6, the GMST-sourcing half, already known-open).
- **Related**: `charal.md` §8 item 6.
- **Suggested Fix**: Introduce `SkillDerivation { base, attr_mult, luck_mult }` as
  a `CharacterRuleset` field populated by the Fallout builders, have
  `derive_autocalc_actor_values` read it from the ruleset instead of local
  constants, and drop the local `SPECIAL` array in favour of
  `AttributeSet::FALLOUT.members()`. That also puts the GMST sourcing (§8.6) one
  step from done — the values then have one place to be read into.

---

### CHAR-D1-04: `derived_len` is documented as a stat count but returns a row count

- **Severity**: LOW
- **Dimension**: Ruleset Seam
- **Game**: all (observable on Oblivion)
- **Location**: `crates/core/src/character/ruleset.rs:122-125` (`derived_len`)
- **Status**: NEW
- **Description**: `derived_len`'s docstring reads "Number of derived stats this
  game computes"; the body is `self.derived.len()`, the number of formula **rows**.
  `push_derived` explicitly supports several rows under one `output_avif`, so the
  two diverge whenever a multi-row stat exists: `oblivion_ruleset` registers 8
  rows for **5** distinct stats (Fatigue is 4 rows). The same conflation reaches
  the `CharacterRuleset` struct docstring, whose flat-`Vec` rationale is stated as
  "A game computes only ~6–10 derived stats" when the quantity that actually
  bounds the linear scan is the row count.
- **Evidence**: `derived_len` returns `self.derived.len()`, and
  `oblivion_ruleset` calls `push_derived` four times from
  `oblivion_fatigue_formulas` under a single `resolve("Fatigue")` output id.
  The API's own test (`oblivion_ruleset_assembles_and_evaluates_end_to_end`)
  reads the summed value via `derived_value`, not `derived_len`, so nothing
  pins the intended meaning.
- **Impact**: Documentation only — `derived_len` has no production caller (used
  by tests as a table-shape assertion). It is nonetheless the one number an
  operator or a future audit would quote when re-checking the "N ≈ 6–10"
  data-structure rationale, and it over-reports for any multi-row game.
- **Related**: CHAR-D1-03 (both are `CharacterRuleset` surface accuracy).
- **Suggested Fix**: Rename to `derived_row_len` (or reword the docstring to
  "number of derived-stat formula rows"), and restate the flat-`Vec` rationale in
  the struct docstring in terms of rows.

---

**Findings: 4 — MEDIUM 2, LOW 2. No CRITICAL, no HIGH.**
The headline doctrine check (a per-game branch in a `CharacterRuleset` consumer)
came back **clean**, as did the single-sink, global-FormID-space, roster-split and
table-size checks.

---

## Dimension 2

# /audit-character — Dimension 2: Derived-Stat Formulas

Run: 2026-08-15 · Repo: `/mnt/data/src/gamebyro-redux` · First-ever audit of this subsystem.

## Scope & Coverage

**Capture documents read (before any Rust, per the skill's ordering rule):**
- `docs/engine/charal.md` (full)
- `docs/engine/charal-fnv-fo3-ruleset.md` (full)
- `docs/engine/charal-fo4-ruleset.md` (full)
- `docs/engine/charal-oblivion-ruleset.md` (§ The Complete Damage Formula, § Fatigue,
  § Health, § Magicka — the derived-pool sections; the skill/combat/commerce/disposition
  sections are out of this dimension)
- `docs/engine/charal-skyrim-ruleset.md` (§ Light Armor Rating Bonus, § Armor Rating /
  Damage Reduction, § Magicka, § Carry Weight — the sections behind the two shipped
  Skyrim derived rows)

**Code read:** `crates/core/src/character/derived.rs`, `fallout.rs`, `tes.rs`,
`skyrim.rs`, `resistance.rs` (the single source of the `(gov−1)·k` shape),
`ruleset.rs` (`push_derived` / `derived_value` semantics), plus the two live
consumers of `derived_value` found by grep: `crates/scripting/src/condition.rs`
(`evaluate_function` → `ConditionFunction::GetActorValue`) and
`crates/core/src/character/regen.rs` (`pool_regen_tick_system`).

**Families whose constants were verified against a capture document:** FO4, FO3, FNV,
Oblivion, Skyrim — i.e. **every family with a shipped derived table**.

**Not verified (and why):**
- **FO76 / Starfield** — no ruleset builder and no derived table exist in code
  (`charal-fo76-ruleset.md` / `charal-starfield-ruleset.md` are capture-only). Nothing to
  diff. That is coverage information for Dim 6, not a Dim 2 finding.
- **Population ordering (chaining)** — the skill assigns `npc_spawn.rs` to Dimension 5.
  I state the requirement below and verified only the *formula-side* half of it.
- `oblivion_health_gain_per_level`'s missing `floor` is deliberately **not** reported
  here: that symbol is a Dimension 3 entry point (leveling), and reporting it would
  duplicate the sibling agent.

**Test state:** `cargo test -p byroredux-core character` → **94 passed, 0 failed**,
including `formula_is_thirty_two_bytes_and_copy`.

**Dedup:** `/tmp/audit/character/issues.json` (2832 issues) searched for
`charal` / `derived` / `GetActorValue` / `DerivedScope` / `DerivedOutput` / `avif` /
`melee damage` / `player_only`. Only hit is **#2153 (OPEN) CHARAL-D3-01** —
`pool_regen_tick_system`'s nested lock stack, a concurrency finding with no overlap.
`grep -rn "derived_value\|DerivedStatFormula\|DerivedScope\|DerivedOutput" docs/audits/`
returns **nothing** — no prior report touched these formulas. All findings below are NEW.

---

## Constant Verification Table

Every row diffed code ⇄ capture document. `RM` = `RoundMode`.

| # | Formula / constant | Code value | Document value | Source | Verdict |
|---|---|---|---|---|---|
| 1 | FO4 Health | `bilinear(END, 4.5, LEVEL, 2.5, cross 0.5, bias 77.5).floored().player_only()`, cap ∞ | `floor(77.5 + 4.5·END + 2.5·L + 0.5·L·END)`, "player-only" | `charal-fo4-ruleset.md` § Health — LOCKED (player formula) | **PASS** |
| 2 | FO4 Action Points | `affine(AGI, 10.0, 60.0).player_only()`, cap ∞, RM None | `AP = 60 + 10·Agility` (`fAVDActionPointsBase=60`, `Mult=10`); NPCs read baked `DNAM` Calculated Action Points | `charal-fo4-ruleset.md` § Action Points + § "Derived stats are PRECOMPUTED in `DNAM`" | **PASS** |
| 3 | FO4 Carry Weight | `affine(STR, 10.0, 200.0)`, ActorGeneral | `200 + 10 × Strength`; "**Actor-general (not player-only)**" | `charal-fo4-ruleset.md` § Carry Weight — LOCKED (actor-general) | **PASS** |
| 4 | FO4 Melee Damage | `affine(STR, 0.1, 1.0).as_multiplier()`, ActorGeneral | `1 + Strength × 0.1`, multiplier, actor-general | `charal-fo4-ruleset.md` § Melee Damage — LOCKED (multiplier, actor-general) | **PASS** (coefficients) — consumer handling → CHAR-D2-01 |
| 5 | FO3 Health | `bilinear(END, 20.0, LEVEL, 10.0, cross 0.0, bias 90.0).player_only()`, RM None | `90 + END·20 + Level·10`, LOCKED (player) | `charal-fnv-fo3-ruleset.md` § Derived statistics, Health row | **PASS** |
| 6 | FO3 Action Points | `affine(AGI, 2.0, 65.0).capped(85.0).player_only()` | `65 + 2·AGI` (cap 85) | `charal-fnv-fo3-ruleset.md` § Derived statistics, Action Points row + § Action Points | **PASS** on bias/coeff/cap; **scope UNSOURCED** → CHAR-D2-03 |
| 7 | FNV Health | `bilinear(END, 20.0, LEVEL, 5.0, cross 0.0, bias 95.0).player_only()` | `100 + END·20 + (Level−1)·5` (≡ `95 + 20·END + 5·L`); worked END 10/L30 → 445 | `charal-fnv-fo3-ruleset.md` Health row + `charal-fo4-ruleset.md` cross-game Health table | **PASS** (algebraic re-anchoring is exact) |
| 8 | FNV Action Points | `affine(AGI, 3.0, 65.0).capped(95.0).player_only()` | `65 + 3·AGI` (cap 95) | `charal-fnv-fo3-ruleset.md` Action Points row | **PASS** on numbers; **scope UNSOURCED** → CHAR-D2-03 |
| 9 | FO3/FNV Carry Weight | `affine(STR, 10.0, 150.0)`, ActorGeneral, cap ∞ | `150 + 10·STR`, LOCKED (actor-general) | `charal-fnv-fo3-ruleset.md` Carry Weight row (`fAVDCarryWeight{Base=150,Mult=10}`) | **PASS** |
| 10 | FO3/FNV Melee Damage | `affine(STR, 0.5, 0.0)`, `DerivedOutput::Absolute` | `STR × 0.5`, "an **additive** bonus" | `charal-fnv-fo3-ruleset.md` Melee Damage row + § Melee Damage | **PASS** |
| 11 | FO3/FNV Critical Chance | `affine(Luck, 0.01, 0.0).capped(0.10)` | `Luck × 1%` (cap 10%) | `charal-fnv-fo3-ruleset.md` Critical Chance row | **PASS** on literal transcription; **unit convention** → CHAR-D2-02 |
| 12 | FO3/FNV Unarmed Damage | `affine(Unarmed, 0.05, 0.5).ceiled()`, cap ∞ | `ceil((10 + Unarmed)/20)`, skill-governed, no cap stated | `charal-fnv-fo3-ruleset.md` Unarmed Damage row + § Unarmed Damage | **PASS** (incl. `Ceil`) |
| 13 | FO3/FNV Radiation Resistance | `Affliction::RADIATION` `derive_coeff 2.0`, `resist_cap 85.0` → `affine(END, 2.0, −2.0).capped(85.0)`, ActorGeneral | `(END−1)·2`, cap 85 %, identical FO3==FNV, actor-general | `charal-fnv-fo3-ruleset.md` Radiation Resistance row + § Radiation Resistance | **PASS** |
| 14 | FO3/FNV Poison Resistance | `derive_coeff 5.0`, `resist_cap f32::INFINITY` → `affine(END, 5.0, −5.0)` uncapped | `(END−1)·5`, "**No documented FO3/FNV cap**… don't invent one" | `charal-fnv-fo3-ruleset.md` Poison Resistance row + § Poison Resistance | **PASS** (uncapped honoured) |
| 15 | Oblivion Health | `affine(END, 2.0, 0.0).player_only()` | `2 × Endurance`; "All player-scoped" | `charal-oblivion-ruleset.md` § Health; `charal.md` §5 TES derived pools | **PASS** |
| 16 | Oblivion Magicka | `affine(INT, 2.0, 0.0).player_only()` | `Magicka = INT + INT×fPCBaseMagickaMult(1.0) = 2×Intelligence` | `charal-oblivion-ruleset.md` § Magicka | **PASS** |
| 17 | Oblivion Fatigue | four `affine(attr, 1.0, 0.0).player_only()` rows summed under one output id, uncapped/unrounded | `Strength + Willpower + Agility + Endurance`, expressed as 4 affine rows | `charal-oblivion-ruleset.md` § Fatigue | **PASS** |
| 18 | Oblivion Armor Rating mult | `ARMOR_RATING_SKILL_COEFF = 0.0065`, `ARMOR_RATING_SKILL_BIAS = 0.35`, two `as_multiplier()` rows (Light/Heavy), ActorGeneral | `(0.35 + 0.0065 × ArmorSkill)`, two rows (`LightArmor→LightArmorRating`, `HeavyArmor→HeavyArmorRating`), both Multiplier-kind, ActorGeneral | `charal-oblivion-ruleset.md` § The Complete Damage Formula, item 3 | **PASS** (doc names the exact two-row wiring) |
| 19 | Skyrim Light Armor Rating | `LIGHT_ARMOR_RATING_COEFF = 0.004`, `affine(LightArmor, 0.004, 1.0).as_multiplier().player_only()` | `1 + 0.004 × LightArmorSkill` (player); NPC constant `0.015` deliberately not modelled | `charal-skyrim-ruleset.md` § Light Armor Rating Bonus + § Armor Rating (`0.4/100`, `1.5/100`) | **PASS** |
| 20 | Skyrim Carry Weight | `CARRY_WEIGHT_BIAS = 250.0`, `CARRY_WEIGHT_STAMINA_COEFF = 0.5`, `.a_from_base()`, ActorGeneral | `250 + 0.5 × BaseStamina`; "temporary changes to Stamina… do not have any effect" | `charal-skyrim-ruleset.md` § Carry Weight | **PASS** (base-layer read is correct and tested) |
| 21 | `SKYRIM_POOL_BASE` | `100.0` | base 100 H/M/S, +10/level pick | `charal-skyrim-ruleset.md` § Magicka; `charal.md` §5 | **PASS** (constant lives in `skyrim.rs`; leveling side is Dim 3's) |
| 22 | Cap sentinel | uncapped = `f32::INFINITY`; `eval` does `rounded.min(self.cap)` | doc's field comment names the four real caps (FO3 AP 85, FNV AP 95, Crit 0.10, FO4 VATS 0.95) | `derived.rs` `cap` field doc + per-game rows above | **PASS** — the "cap `0` means uncapped" trap **does not exist**: `0` is not the sentinel, no table passes `capped(0.0)` (grep: 6 call sites, all non-zero), `DerivedStatFormula` has **no** `Default` derive, and no struct literal exists outside the two constructors |
| 23 | Cross term | non-zero **only** in FO4 Health (`0.5`); FO3/FNV Health pass `0.0`; every other row is `affine` (cross `0.0` by construction) | "the most complex is FO4 Health… which needs the cross term; the rest are affine" | `charal.md` §6 / `derived.rs` module docs / `charal-fo4-ruleset.md` § Health | **PASS** — red-flag check clean |
| 24 | `RoundMode` per formula | `Floor` only on FO4 Health; `Ceil` only on Unarmed Damage; `None` on all 18 other rows | FO4 Health `floor(...)`; Unarmed `ceil((10+U)/20)`; every other captured formula is integer-valued or stated without rounding | `charal-fo4-ruleset.md` § Health; `charal-fnv-fo3-ruleset.md` § Unarmed Damage | **PASS** — no "obviously it's round" guess anywhere |
| 25 | `DerivedStatFormula` size / `Copy` / no game branch | 32 B, `Copy` (asserted by `formula_is_thirty_two_bytes_and_copy`); `eval` = 4 mults + 3 adds + 1 match + 1 `min`, no allocation; `grep GameKind` over `crates/core/src/character/` → **0 hits** | "`Copy` and 32 bytes… ~5 FMAs + one branch + one `min` — no allocation, no virtual dispatch"; per-game seam is data | `derived.rs` § Efficiency; `charal.md` §0 doctrine | **PASS** |
| 26 | `DerivedInput` sentinels | `0` = UNUSED, `u32::MAX` = LEVEL; `u32::MAX` is unreachable for a real FormID (index `0xFF` is reserved) | "the FormID `0` is the null form, never a real AV"; "never a plausible FormID" | `derived.rs` `DerivedInput` docs | **PASS** for `LEVEL`; **caller guarantee unenforced for `0`** → CHAR-D2-04 |

**Tally — 26 rows: 25 PASS, 1 UNSOURCED (row 6/8, the FO3/FNV Action Points `DerivedScope`), 0 numeric mismatches.**
No coefficient, bias, cross term, cap or rounding mode in any shipped table contradicts
its capture document. Every numeric constant in the derived tables is sourced.

---

## Findings

### CHAR-D2-01: `DerivedOutput::Multiplier` is ignored by the live `GetActorValue` consumer — FO4 Melee Damage leaks a ×1.0–2.0 multiplier as if it were an actor value
- **Severity**: MEDIUM
- **Dimension**: Derived Formulas
- **Game**: fo4 (latent for oblivion)
- **Location**: `crates/scripting/src/condition.rs` (`evaluate_function`, `ConditionFunction::GetActorValue` arm) · `crates/core/src/character/fallout.rs` (`fallout4_ruleset`) · `crates/core/src/character/ruleset.rs` (`derived_value`)
- **Status**: NEW
- **Source**: `docs/engine/charal-fo4-ruleset.md` § "Melee Damage — LOCKED (multiplier, actor-general)": *"A **multiplier** on melee + unarmed weapon damage (STR 0 → ×1.0, STR 5 → ×1.5, STR 10 → ×2.0) — **not an additive bonus, and not a standalone resource AV**"*, and the design note it motivates: *"`Multiplier`-kind formulas apply at combat/use time against a base; absolute-kind formulas produce the AV the runtime reads."*
- **Description**: The `GetActorValue` arm guards on `DerivedScope` (`if formula.scope == DerivedScope::ActorGeneral`) but never inspects `formula.kind`. `fallout4_ruleset` registers Melee Damage as `affine(STR, 0.1, 1.0).as_multiplier()` with the **default `ActorGeneral` scope** (correct per the document), so it passes the scope gate. A CTDA reading the FO4 `MeleeDamage` AV on an actor that does not carry it therefore receives `1 + 0.1·STR` — a dimensionless multiplier in `[1.0, 2.0]` — presented as the actor value itself. The `DerivedOutput` enum exists precisely to prevent this and has **zero readers** anywhere in the workspace (`grep DerivedOutput` outside `derived.rs`/`mod.rs` → only the constructors and unit tests).
- **Evidence**: `condition.rs` (`GetActorValue` arm) comments *"if this game **derives** the stat actor-generally (Carry Weight / **Melee Damage** / Crit Chance / Unarmed Damage from SPECIAL/skills), compute it from the per-game `CharacterRuleset`"* — the list is the FO3/FNV set, where Melee Damage is `Absolute` (`STR × 0.5`, an additive bonus). The same code path is shared by FO4, where the identically-named row is `Multiplier`. `fallout.rs` `fallout4_ruleset`: `DerivedStatFormula::affine(av(s), 0.1, 1.0).as_multiplier()` — no `player_only()`, so scope is `ActorGeneral`.
- **Impact**: Silent numeric type-confusion on the one wired non-Fallout-3/NV game. Every FO4 condition/dialogue/perk gate that reads `MeleeDamage` on an actor without a stored value compares against ~1.x instead of a damage figure — no crash, no test failure, no log line. Oblivion's two `Multiplier` armor rows (`LightArmorRating` / `HeavyArmorRating`, also `ActorGeneral`) are the same shape and will land in the same trap the moment `build_character_ruleset` gains an Oblivion arm. Blast radius is bounded because the derive branch only runs when the AV is *absent* from the actor.
- **Related**: CHAR-D2-05 (the other `derived_value` consumer skips the scope check too); Dim 5 owns whether FO4 population writes `MeleeDamage` at all.
- **Suggested Fix**: In the `GetActorValue` arm, require `formula.kind == DerivedOutput::Absolute` alongside the existing `ActorGeneral` scope check, returning the absent-AV default `0.0` for `Multiplier` rows; a multiplier belongs to the combat/XP consumer, not a generic AV read.

### CHAR-D2-02: The FO3/FNV derived table mixes two unit conventions — Critical Chance is a fraction, the resistances are percentages — with no documented rule
- **Severity**: MEDIUM
- **Dimension**: Derived Formulas
- **Game**: fo3, fnv
- **Location**: `crates/core/src/character/fallout.rs` (`add_fnv_fo3_shared`, the `CritChance` row) · `crates/core/src/character/resistance.rs` (`Affliction::RADIATION`, `Affliction::POISON`, `damage_multiplier`)
- **Status**: NEW
- **Source**: `docs/engine/charal-fnv-fo3-ruleset.md` § Derived statistics — Critical Chance row: *"`Luck × 1%` (cap 10%) … base `Luck × 1%` is the `critchance` AV"*; Radiation Resistance row: *"`(END−1)·2` (cap 85%)"*; Poison Resistance row: *"`(END−1)·5` (uncapped)"* — the document writes **all three as percentages**.
- **Description**: Both rows transcribe their document literally, but the documents use different notation for the same physical quantity, and the code inherits the split: Critical Chance evaluates to `0.05` at Luck 5 with `cap 0.10` (a **fraction**), while Radiation Resistance evaluates to `8.0` at END 5 with `cap 85.0` (a **percentage on 0–100**). CHARAL's only shipped percentage consumer fixes the 0–100 convention explicitly — `damage_multiplier(resist_pct, cap_pct)` computes `1.0 − r/100.0`. Nothing in `DerivedStatFormula`, `DerivedOutput`, or the ruleset records which convention a given output id uses, so a consumer reading two rows out of the same table cannot tell a 5 % crit chance (`0.05`) from an 8 % rad resistance (`8.0`) without hardcoding per-stat knowledge — exactly the per-game/per-stat branching CHARAL exists to remove.
- **Evidence**: `fallout.rs`: `DerivedStatFormula::affine(av(l), 0.01, 0.0).capped(0.10)` vs `resistance.rs`: `derive_coeff: 2.0, resist_cap: 85.0` feeding `affine(gov, 2.0, −2.0).capped(85.0)`. `resistance.rs` `damage_multiplier` divides by `100.0`. The unit tests encode both conventions side by side (`critical_chance_capped_and_xp_multiplier` asserts `0.05`; `radiation_formula_matches_wiki_and_caps` asserts `8.0`).
- **Impact**: A downstream reader (HUD, CTDA threshold, perk entry point) that assumes one convention is 100× off on the other, silently. Vanilla-authored thresholds are written against whatever the original engine stored, and CHARAL currently offers no way to answer that question from the data.
- **Related**: CHAR-D2-01 (same class: a formula's *interpretation* is not carried with its value).
- **Suggested Fix**: Record the convention on the formula (e.g. a `Percent` variant alongside `Absolute`/`Multiplier`, or normalise every percentage stat to 0–100 and restate the Critical Chance row as `affine(Luck, 1.0, 0.0).capped(10.0)`), and state the chosen convention in `derived.rs`'s module docs so the next row cannot pick the other one. Whichever direction is chosen, `damage_multiplier`'s `/100` fixes 0–100 as the incumbent.

### CHAR-D2-03: FO3/FNV Action Points is tagged `player_only` with no capture-document support, and the document's own `fAVD…` rule argues the opposite
- **Severity**: LOW
- **Dimension**: Derived Formulas
- **Game**: fo3, fnv
- **Location**: `crates/core/src/character/fallout.rs` (`fallout3_ruleset`, `falloutnv_ruleset` — the `ActionPoints` rows)
- **Status**: NEW
- **Source**: `docs/engine/charal-fnv-fo3-ruleset.md` § Derived statistics — the table annotates scope explicitly on every other row (Health *"**LOCKED** (player)"*, Carry Weight *"**LOCKED** (actor-general)"*, Radiation/Poison Resistance *"**LOCKED** (actor-general…)"*) and gives Action Points **no scope annotation at all**: *"| Action Points | AGI | `65 + 2·AGI` (cap 85) | `65 + 3·AGI` (cap 95) | **LOCKED** |"*. The same document's Carry Weight § states the discriminating rule: *"The `fAVD…` (Actor Value Derived) prefix means this derives the … AV for **any** actor"* — and its Action Points § names *"the same `fAVDActionPoints{Base,Mult}` GMST family"*.
- **Description**: The numbers are right (rows 6 and 8 above); the **scope tag** is an engine decision no capture line backs. By the document's stated `fAVD` heuristic, `fAVDActionPointsBase/Mult` would make AP actor-general for FO3/FNV, the same way it makes Carry Weight actor-general. The FO4 row *is* sourced — FO4 NPCs read a baked `DNAM` "Calculated Action Points" — but that evidence is FO4-specific (`PRPS`/`DNAM` are an FO4-era layout; the FO3/FNV § "NPC stat storage" note says only that auto-calc-OFF NPCs store explicit skill/SPECIAL values, saying nothing about AP). `fallout.rs`'s module docstring generalises FO4's justification (*"NPCs ship baked values or derive them differently"*) across all three games without a citation for two of them.
- **Evidence**: `fallout3_ruleset`: `DerivedStatFormula::affine(av(a), 2.0, 65.0).capped(85.0).player_only()`; `falloutnv_ruleset`: the `3.0/95.0` twin. Compare the Carry Weight row two functions away, deliberately left `ActorGeneral` on the strength of the `fAVD` rule.
- **Impact**: Conservative direction — `GetActorValue(ActionPoints)` on an FNV NPC without the AV returns the absent default `0.0` rather than a possibly-correct `65 + 3·AGI`. Nothing is over-computed, so no stat is inflated; the cost is a silently-missing derivation and an unsourced constant sitting in a table whose whole premise is that every entry is sourced. It also makes the FO3↔FNV *player* Health/AP deferral look wider than it is.
- **Related**: The known-open FO3↔FNV player Health/AP divergence (deliberately deferred — **not** re-filed here; this finding is about the *scope tag*, not the divergence).
- **Suggested Fix**: Either cite a line making FO3/FNV AP player-only and add it to the capture document, or flip the two rows to `ActorGeneral` per the `fAVD` rule; in the meantime annotate the code with "scope unsourced, chosen conservatively" so it is not mistaken for a captured fact.

### CHAR-D2-04: `DerivedInput::actor_value`'s "never `0`" caller guarantee is unenforced at the only real construction site, and one in-repo caller already violates it
- **Severity**: LOW
- **Dimension**: Derived Formulas
- **Game**: all
- **Location**: `crates/core/src/character/derived.rs` (`DerivedInput::actor_value`, `DerivedInput::UNUSED`) · `byroredux/src/npc_spawn.rs` (`build_character_ruleset`) · `crates/plugin/src/esm/records/index.rs` (`actor_value_form_id`) · `crates/core/src/character/tes.rs` (test `oblivion_ruleset_skips_unresolved_pools`)
- **Status**: NEW
- **Description**: `DerivedInput` packs "unused" into the value `0`, documented as a **caller** guarantee: *"(Caller guarantees the id is neither `0` nor `u32::MAX` — real Bethesda FormIDs never are.)"* The skill asks to check the callers, not the constructor. The single production construction site is `build_character_ruleset`, whose resolver is `|editor_id| index.actor_value_form_id(editor_id)`; `actor_value_form_id` returns `.map(|avif| avif.form_id)` with **no non-zero filter**. A `Some(0)` therefore flows into `DerivedInput::actor_value(0)`, which compares equal to `UNUSED`, and `read()` returns `0.0` — the coefficient is silently dropped and the formula still registers, producing a wrong value rather than the resolve-or-**skip** degradation the builders promise everywhere else. The `u32::MAX` half is genuinely unreachable (index `0xFF` is reserved), so only the `0` half is exposed. That the invariant is undefended is already demonstrated in-repo: `tes.rs`'s `oblivion_ruleset_skips_unresolved_pools` resolver maps `"Strength" => 0x00`, while its sibling test `oblivion_ruleset_assembles_and_evaluates_end_to_end` carries the comment *"Non-zero ids throughout: FormID 0 is the null form and also `DerivedInput::UNUSED`, so a real AV never resolves to it."* Two tests in one file, opposite assumptions.
- **Evidence**: `derived.rs` `read()`: `match self.0 { 0 => 0.0, u32::MAX => f32::from(level), … }` — a `0` input is indistinguishable from `UNUSED` at evaluation time. `index.rs` `actor_value_form_id` has no guard. No `debug_assert` exists in `actor_value`.
- **Impact**: Requires an AVIF whose (remapped) FormID is `0` — not observed in vanilla data, so this is defence-in-depth rather than a live defect. If it ever happens the failure is silent and per-stat: e.g. Carry Weight would evaluate to its bare bias (150/200) for every actor, with no warning.
- **Related**: Dim 1 owns whether these ids are remapped to global space; Dim 5 owns the population path.
- **Suggested Fix**: Make the guarantee enforceable rather than documented — have `build_character_ruleset`'s resolver filter `Some(0) → None` (turning the collision into the existing resolve-or-skip path), and add a `debug_assert!(avif_form_id != 0 && avif_form_id != u32::MAX)` in `DerivedInput::actor_value`. Fix the `tes.rs` test resolver's `0x00` while there.

### CHAR-D2-05: `pool_regen_tick_system` evaluates `derived_value` without the `DerivedScope` gate its sibling consumer applies
- **Severity**: LOW
- **Dimension**: Derived Formulas
- **Game**: oblivion (latent)
- **Location**: `crates/core/src/character/regen.rs` (`pool_regen_tick_system`) · `crates/core/src/character/tes.rs` (`oblivion_magicka_formula`)
- **Status**: NEW
- **Source**: `crates/core/src/character/derived.rs` `DerivedScope` docs: *"A consumer that computes a derived stat for an arbitrary entity checks this before trusting the result."* `docs/engine/charal.md` §5: the Oblivion pools are *"All player-scoped."*
- **Description**: There are exactly two `derived_value` consumers. `condition.rs` gates on `formula.scope == DerivedScope::ActorGeneral`; `pool_regen_tick_system` does not — it iterates **every** entity carrying `ActorValues` and calls `ruleset.derived_value(config.magicka_avif, avs, 1)` to obtain `MaxMagicka`. `oblivion_magicka_formula` is `.player_only()`, so the player-only formula is applied to every NPC in the cell. The scope flag is advisory in practice: half the consumers honour it.
- **Evidence**: `regen.rs`: `for (_entity, avs) in avs_q.iter_mut() { … let max_magicka = ruleset.derived_value(config.magicka_avif, avs, 1).unwrap_or(0.0); … }` — no `derived_formula(...).scope` check. Compare `condition.rs`'s `if formula.scope == DerivedScope::ActorGeneral`.
- **Impact**: **Latent today** — `build_character_ruleset` returns `None` for Oblivion, so no `CharacterRuleset` with a player-only Magicka row is ever inserted, and `PoolRegenConfig` is likewise never inserted (`oblivion_pool_regen_config` has no caller). It becomes live the moment the Oblivion ruleset is wired, and the failure is a wrong regen *rate* per NPC, not a crash. Filed because the checklist's whole point is that the deferral stays contained to player stats, and here it does not — the containment is accidental (unwired game), not structural.
- **Related**: CHAR-D2-01 (the other half of "formula metadata has no enforced reader"); #2153 (OPEN) covers this system's *locking*, not its scope handling — no overlap.
- **Suggested Fix**: Either check `DerivedScope` in `pool_regen_tick_system` before applying a derived pool to a non-player actor, or give the Oblivion Magicka pool an actor-general row and document why the player-only tag was dropped. A shared helper (`ruleset.derived_value_for_scope(...)`) would stop the third consumer from re-deciding.

### CHAR-D2-06: `eval` clamps only the upper bound — an absent or zero governing attribute produces a negative resistance
- **Severity**: LOW
- **Dimension**: Derived Formulas
- **Game**: fo3, fnv
- **Location**: `crates/core/src/character/derived.rs` (`DerivedStatFormula::eval`) · `crates/core/src/character/resistance.rs` (`Affliction::fo3_fnv_resistance_formula`)
- **Status**: NEW
- **Source**: `docs/engine/charal-fnv-fo3-ruleset.md` § Attributes — LOCKED: *"Chargen: each starts at **5**, 40 total… **Range 1–10**"*; § Radiation Resistance: *"`(Endurance − 1)·2 = 2·END − 2` … capped at **85 %**"* (the document states an upper cap only, and its worked examples never leave the 1–10 domain).
- **Description**: `eval` ends with `rounded.min(self.cap)` — there is no lower clamp. The `(gov − 1)·k` resistances are the only shipped rows whose bias is negative, so they are the only ones that can go below zero, and they do so exactly when the governing AV is **absent** (`read()` returns `0.0` for a missing AVIF, by design) or genuinely `0`: Radiation Resistance → `−2.0`, Poison Resistance → `−5.0`. That is outside the sourced attribute domain, and it is the "accidental zero" the chaining checklist warns about, one algebraic step downstream: the *documented* default for an absent input is `0.0`, but no capture line documents what `(0−1)·k` should mean.
- **Evidence**: `derived.rs` `eval`: `rounded.min(self.cap)`. `resistance.rs`: `DerivedStatFormula::affine(av, k, −k).capped(cap)`. `derived.rs` test `absent_input_reads_zero` fixes the absent-AV behaviour as intentional but only exercises a positive-bias formula (`affine(STR, 10.0, 200.0)` → `200.0`).
- **Impact**: Bounded. The dedicated consumer is safe — `damage_multiplier` re-clamps with `resist_pct.clamp(0.0, cap_pct)` — so no negative resistance can turn damage into healing. The exposure is the generic read: `GetActorValue(RadResist)` on an FNV actor whose Endurance was never populated returns `−2.0`, and any CTDA comparison or HUD readout takes it at face value. Requires an incompletely-populated actor, which is a Dim 5 ordering question.
- **Related**: CHAR-D2-02 (both are "the number leaves the table without its meaning"); the chaining/ordering requirement restated below.
- **Suggested Fix**: Add an optional `floor_at`/lower clamp to `DerivedStatFormula` (there is no spare padding left after `base_reads`, so this costs the 32-byte guarantee — alternatively clamp the two resistance rows at their construction site in `fo3_fnv_resistance_formula`, which keeps the layout and is where the negative bias is introduced).

---

## Checklist items verified with no finding

- **Every coefficient, bias, cross term, cap, rounding mode** — see the 26-row table. No mismatch.
- **The cross-term red flag** — non-zero cross exists in exactly one row (FO4 Health, `0.5`), as the document predicts. Every other row is affine.
- **`RoundMode`** — `Floor` on FO4 Health only, `Ceil` on Unarmed Damage only, `None` everywhere else, each traceable to a document line. No stat was given `round` by assumption.
- **Cap sentinel** — the "cap `0` means uncapped" hazard the checklist anticipates **is not present**: the sentinel is `f32::INFINITY`, no table passes `0.0` (all six `.capped(` call sites carry real values), `DerivedStatFormula` has no `Default` derive that could zero it, and no struct literal exists outside `affine`/`bilinear`.
- **`DerivedInput::LEVEL`** — `u32::MAX` cannot collide with a real AVIF FormID (plugin index `0xFF` is reserved); `read()` widens a `u16` level, so no truncation. Only the `0` sentinel is exposed (CHAR-D2-04).
- **`eval` shape** — allocation-free, no virtual dispatch, no `GameKind`/master-name/game-identity branch anywhere under `crates/core/src/character/` (grep: 0 hits). `DerivedStatFormula` is `Copy` and 32 bytes, asserted by a live test.
- **Multi-row summation contract** — `push_derived` documents that multi-row stats must be uncapped/unrounded/absolute; Oblivion Fatigue (the only shipped multi-row stat) honours it on all four rows.
- **Skyrim Carry Weight base-layer read** — `.a_from_base()` correctly implements the sourced "Fortify Stamina must not move Carry Weight" requirement, with a test that proves it.

## Chaining / ordering requirement (stated, not audited — Dimension 5 owns it)

Three shipped formulas read a **skill** rather than an attribute — FO3/FNV Unarmed
Damage (← `Unarmed`), Skyrim Light Armor Rating (← `LightArmor`), Oblivion Light/Heavy
Armor Rating (← the armor skills) — and Skyrim Carry Weight reads `Stamina`'s base
layer. There is no dependency graph: `eval` reads whatever `ActorValues` holds *now*.
The requirement the population path must satisfy is therefore: **base attributes and
skills must be written into `ActorValues` before any dependent derived stat is
evaluated.** On the formula side the failure mode is not a panic — `read()` returns
`0.0` for an absent input by design — but it is only *benign* for positive-bias rows
(Unarmed Damage degrades to `ceil(0.5) = 1`, the floor value; Carry Weight degrades to
its bias). For the negative-bias resistance rows it produces an out-of-domain negative
(CHAR-D2-06). A sibling agent has `npc_spawn.rs`.

## Known-open items — confirmed NOT re-filed

- FNV/FO3 **tag-skill per-level** formula — undocumented, deferred. Not touched.
- FO3↔FNV divergent **player** Health/AP — deferred with the player actor. CHAR-D2-03
  concerns the AP row's `DerivedScope` **tag**, not this divergence.
- **VATS runtime** — does not exist; only the AP formulas are in CHARAL. Not filed.

---

## Dimension 3



**Date**: 2026-08-15 · **Depth**: deep · **Games**: all implemented (FO3 / FNV / FO4 / Oblivion / Skyrim)

## Scope & Coverage

**Capture documents read first (before any Rust), in the mandated order:**
- `docs/engine/charal.md` (full — §1 family table, §3 AUTHORED/ENGINE-SUPPLIED split, §5 ruleset shape + all three `LevelingModel` variants, §7.1 companion progression, §8 rollout, §9 open research)
- `docs/engine/charal-skyrim-ruleset.md` (full, 710 lines)
- `docs/engine/charal-oblivion-ruleset.md` (section headers + all leveling / pool / attribute-bonus regions)
- `docs/engine/charal-fnv-fo3-ruleset.md` (§ *XP / level curve — LOCKED*, § level cap / perk cadence / level-up reward)
- `docs/engine/charal-fo4-ruleset.md` (§ *XP / level curve — LOCKED*, § *Perk chart — COMPLETE*, gating shape)
- Project memory: `tes_character_rules`, `perk_system` (both named in the skill's ground-truth list)

**Rust read (after the documents):**
- `crates/core/src/character/leveling.rs` (full)
- `crates/core/src/character/skyrim.rs` (full)
- `crates/core/src/character/tes.rs` (full)
- `crates/core/src/character/components.rs` (full)
- Consumer / boundary tracing: `crates/scripting/src/condition.rs` (`GetXPForNextLevel`, `HasPerk`),
  `byroredux/src/npc_spawn.rs` (`build_character_ruleset`, the `Perks`/`CharacterLevel` spawn stamp),
  `crates/core/src/character/fallout.rs` (builder entry points), `crates/core/src/ecs/components/perk_list.rs`,
  `crates/plugin/src/esm/records/misc/magic.rs` (`PerkRecord::num_ranks`),
  `crates/plugin/src/esm/records/actor/mod.rs` (`PRKR`), `crates/plugin/src/esm/records/index.rs` (`game_settings`),
  `crates/plugin/src/esm/reader.rs` (`GameKind`), `byroredux/src/save_io/round_trip_tests.rs`

**Test baseline**: `cargo test -p byroredux-core character` → **94 passed, 0 failed** (529 filtered out).

**Verified clean (checklist items with no finding):**
- **Three data variants, one consumer match.** `LevelingModel` is a single enum; every variant-specific branch lives inside its own accessor in `leveling.rs` (`xp_to_next`, `xp_from_skill_rank`, `pool_pick_gain`, `skill_points`, `grants_perk_at`, `level_cap`). A repo-wide grep finds exactly **one** non-test consumer of the model anywhere — `ConditionFunction::GetXPForNextLevel` in `crates/scripting/src/condition.rs`, which calls `rs.leveling.xp_to_next(level)` and never matches the enum itself. No consumer-side per-game branch exists. Doctrine holds.
- **`level_cap == 0` sentinel consistency.** `level_cap()` merges all three variants into one arm-pattern, so the sentinel cannot diverge per variant by construction. No off-by-one at the cap is possible because nothing consults it (see CHAR-D3-04).
- **Fallout XP curves.** FO4 `75·L + 125`, FO3/FNV `150·L + 50` — both match their capture documents exactly, and `xp_curves_match_wiki_tables` pins the document's own worked anchors (200/875/1775, 200/350/800).
- **Oblivion attribute-bonus banding.** `oblivion_attribute_bonus` implements `0 → +1`, `1–4 → +2`, `5–7 → +3`, `8–9 → +4`, `10+ → +5` with a `_ => 5` catch-all (no roll-over), matching the document. `attribute_bonus_tiers_match_uesp` pins both edges of every band. The 10-major-skill-ups threshold in `LevelingModel::OBLIVION` matches.
- **Skyrim base pools.** `SKYRIM_POOL_BASE = 100.0` and `pool_pick_gain() = 10.0` are independently confirmed by `charal-skyrim-ruleset.md`'s *Magicka* section (UESP *Skyrim:Magicka*), which is the one Skyrim leveling number with a real per-game-document citation.
- **Skyrim skill-XP cost curve.** `skyrim_skill_xp_to_next` = `mult · level^curve + offset` reproduces the document's Lockpicking anchors (15→16 ≈ 349.13; 15→20 ≈ 1815.5), and `skyrim_skill_xp_between` is a true sum of steps with a correct empty/inverted-range guard.
- **Perk rank additivity.** `Perks::set_rank` de-duplicates by `perk_form_id` — a second grant raises the existing entry rather than pushing a duplicate. That half of the checklist is satisfied; the *max-rank* half is not (CHAR-D3-05).

**Could not verify, and why:**
- **Whether the Skyrim XP constants (`xp_base 75` / `xp_mult 25` / `xp_per_skill_rank 1`) are numerically correct.** They are absent from `charal-skyrim-ruleset.md`; the only record is one line of implementation-summary prose in `charal.md` §5. I did not substitute my own recollection of UESP (`feedback_no_guessing`). Reported as a sourcing gap (CHAR-D3-06), **not** as a wrong value.
- **Oblivion's `level_cap = 0`.** No capture document states an Oblivion level cap either way; `charal.md` §5 and project memory both assert "no hard cap", which is what the code encodes. Accepted, not independently confirmable.
- **FNV `perk_cadence = 2`.** `charal-fnv-fo3-ruleset.md` states it but self-flags it as "well-known; not on the pages pulled so far — mark for a citing pass". Recorded as `MATCH (document self-flags as uncited)`.

## Wired / Unwired per model

`build_character_ruleset` (`byroredux/src/npc_spawn.rs`) is the only construction site, reached from
`byroredux/src/cell_loader/references/mod.rs:275`.

| Model | Ruleset constructor | Reachable at runtime? |
|---|---|---|
| Fallout `XpCurve` — FO4 | `fallout4_ruleset` | **WIRED** (`GameKind::Fallout4`) |
| Fallout `XpCurve` — FNV | `falloutnv_ruleset` | **WIRED** (`GameKind::Fallout3NV`) |
| Fallout `XpCurve` — FO3 | `fallout3_ruleset` | **UNWIRED** — no call site; `GameKind` collapses FO3 into `Fallout3NV`, which selects the **FNV** model (CHAR-D3-02) |
| classic-TES `SkillUse` — Oblivion | `oblivion_ruleset` | **UNWIRED** — `build_character_ruleset` returns `None` for `GameKind::Oblivion` |
| Skyrim `SkillXp` | `skyrim_ruleset` | **UNWIRED** — `build_character_ruleset` returns `None` for `GameKind::Skyrim` |

Oblivion and Skyrim being unwired is **coverage information, not a bug** — it is the documented state
(`charal.md` §8, `_audit-common.md`'s CHARAL row, and `byroredux/src/boot.rs:859`'s own comment).
Severities below are not inflated for unwired code: every MEDIUM finding here concerns a **wired** path
(FO4/FNV spawn, `GetXPForNextLevel`, `HasPerk`) or a wiring decision, and the purely-unwired Oblivion /
Skyrim numbers produced no findings beyond the documentation gap in CHAR-D3-06.

## Constant table

Verdicts: **MATCH** = code equals document. **UNSOURCED** = no per-game capture document carries it.

### Skyrim

| Constant | Code | Document | Verdict |
|---|---|---|---|
| `SKYRIM_POOL_BASE` | `100.0` | base 100 H/M/S — `charal-skyrim-ruleset.md` § *Magicka* (UESP *Skyrim:Magicka*), `charal.md` §5 | MATCH |
| `pool_pick_gain` | `10.0` | "+10 points … each time you level up" — `charal-skyrim-ruleset.md` § *Magicka*; `charal.md` §5 "+10 pool pick" | MATCH |
| `xp_mult` (`fXPLevelUpMult`) | `25.0` | `charal.md` §5 "`SKYRIM` = 25·L+75 XP" **only** — absent from `charal-skyrim-ruleset.md` | MATCH vs `charal.md`, **UNSOURCED** vs the per-game authority |
| `xp_base` (`fXPLevelUpBase`) | `75.0` | as above | MATCH vs `charal.md`, **UNSOURCED** vs the per-game authority |
| `xp_per_skill_rank` (`fXPPerSkillRank`) | `1.0` | `charal.md` §5 "1 XP/skill rank" only | MATCH vs `charal.md`, **UNSOURCED** vs the per-game authority |
| `level_cap` | `0` (uncapped) | `charal.md` §5 "no hard cap"; memory `tes_character_rules` "level cap removed in patch 1.9" | MATCH |
| `SKYRIM_SKILL_USE_CURVE` (`fSkillUseCurve`) | `1.95` | `charal.md` §3 row 147 "(`fSkillUseCurve` 1.95)" | MATCH |
| skill-XP step shape | `mult·L^curve + offset` | `charal.md` §3; anchors 349.13 / 1815.5 reproduced by `skill_xp_cost_matches_uesp_lockpicking` | MATCH |
| perk per level | `grants_perk_at` → `true` for `SkillXp` | `charal.md` §5 "+10 pool pick + perk/level" | MATCH |

### Oblivion

| Constant | Code | Document | Verdict |
|---|---|---|---|
| `major_skill_ups_per_level` | `10` | `charal.md` §5 "`OBLIVION` = 10 major-skill-ups/level"; memory `tes_character_rules` | MATCH |
| `level_cap` | `0` (uncapped) | `charal.md` §5 / memory "no hard cap" | MATCH |
| `oblivion_attribute_bonus` band `0` | `+1` | `charal.md` §5 "(0 / 1–4 / 5–7 / 8–9 / 10+)"; memory "0→+1" | MATCH |
| band `1–4` | `+2` | as above | MATCH |
| band `5–7` | `+3` | as above | MATCH |
| band `8–9` | `+4` | as above | MATCH |
| band `10+` | `+5`, capped, no roll-over | `charal.md` §5 "capped, no roll-over" | MATCH |
| `oblivion_health_gain_per_level` | `0.1 × END` | `charal-oblivion-ruleset.md` § *Health* ("per-level accrual `0.1×Endurance`", worked anchor END 98 → +9) | MATCH |
| `oblivion_health_formula` | `2 × END` | `charal-oblivion-ruleset.md` § *Health*; `charal.md` §5 | MATCH |
| `oblivion_magicka_formula` | `2 × INT` | `charal-oblivion-ruleset.md` § *Magicka* (`Intelligence + Intelligence×fPCBaseMagickaMult(1.0)`) | MATCH |
| `oblivion_fatigue_formulas` | `STR + WIL + AGI + END`, 4 rows × coeff `1.0` | `charal-oblivion-ruleset.md` § *Fatigue*; `charal.md` §5 (four affine rows) | MATCH |
| perks | `grants_perk_at` → `false` for `SkillUse` | `charal.md` §1 family table: TES classic → "Perks: none" | MATCH |

### Fallout (cross-checked because the FO3/FNV collapse is a Dim-3 finding)

| Constant | Code | Document | Verdict |
|---|---|---|---|
| FO4 `xp_a` / `xp_b` | `75.0` / `125.0` | `charal-fo4-ruleset.md` § *XP / level curve — LOCKED*: `75·L + 125` | MATCH |
| FO4 `level_cap` | `0` | same § "No level cap" | MATCH |
| FO4 `reward` | `SpecialOrPerk` | same § "one point … +1 SPECIAL **or** one perk rank" | MATCH |
| FO3 `xp_a` / `xp_b` | `150.0` / `50.0` | `charal-fnv-fo3-ruleset.md` § *XP / level curve*: `150·L + 50` (both games) | MATCH |
| FO3 `level_cap` | `20` | same §: "FO3 **20** (30 with *Broken Steel*)" | MATCH |
| FO3 `int_mult` / `perk_cadence` | `1.0` / `1` | same §: "int_mult: 1.0 (FO3) … perk_cadence: 1 (FO3)" | MATCH |
| FNV `level_cap` | `30` | same §: "FNV **30** (50 with the four add-ons)" | MATCH |
| FNV `int_mult` | `0.5` | same §: "0.5 (FNV)" | MATCH |
| FNV `perk_cadence` | `2` | same §: "2 (FNV)" — document self-flags as uncited | MATCH (document self-flags as uncited) |
| `LevelReward::SkillPoints.base` | `10.0` | same §: "base: 10" | MATCH |

**No numeric mismatch was found in any implemented model.** All eight findings below are structural,
wiring, sourcing, or documentation defects.

---

## Findings

### CHAR-D3-01: `Perks` is stamped at spawn but `HasPerk` reads `PerkList`, which nothing writes — two parallel perk components with an empty intersection
- **Severity**: MEDIUM
- **Dimension**: Leveling & Progression
- **Game**: FO4 (parse side), all (component side)
- **Location**: `crates/core/src/character/components.rs:29-84`; `crates/core/src/ecs/components/perk_list.rs:19-68`; writer at `byroredux/src/npc_spawn.rs:132-144`; reader at `crates/scripting/src/condition.rs:673-693`
- **Status**: NEW
- **Source**: `docs/engine/charal.md` §4.3 — "`pub struct Perks { entries: Vec<(u32 /* PERK FormID */, u8 /* rank */)> }` … The component the perk entry-point modifier pipeline iterates."
- **Description**: Two ECS components model "the perks an actor holds", and each docstring claims to be *the* perk surface. `Perks` (CHARAL, ranked) is the canonical type per `charal.md` §4.3, and is the one the spawn path writes: `spawn_npc_entity` builds it from the NPC's parsed `PRKR` pairs. `PerkList` (`Vec<FormId>`, rankless) is what the only runtime perk reader — `ConditionFunction::HasPerk`, CTDA index 449 (FO3/FNV) / 448 (Skyrim) — actually queries. A repo-wide grep confirms `PerkList` has **zero** production write sites; the codebase already knows this, and `byroredux/src/save_io/registry_completeness_tests.rs:134` records it as an accepted state with the note "do not confuse with the unrelated, already-tracked `Perks` character component" — an assertion of unrelatedness that both components' own docstrings contradict. The net effect is that the writer and the reader never meet.
- **Evidence**:
  ```rust
  // byroredux/src/npc_spawn.rs:135-143 — the only production perk writer
  world.insert(placement_root, Perks { entries: npc.perks.iter()
      .map(|&(perk_form_id, rank)| PerkRank { perk_form_id, rank }).collect() });

  // crates/scripting/src/condition.rs:679 — the only production perk reader
  let Some(perks) = world.get::<PerkList>(entity) else { return 0.0; };
  ```
  `grep -rn --include="*.rs" "PerkList" .` outside `crates/core` returns only
  `condition.rs` (read + one `#[cfg(test)]` insert) and the registry-completeness
  note. `crates/core/src/ecs/components/perk_list.rs:3-5` claims `PerkList` "is the
  ECS surface the perk system (`PERK` records, perk-grant/revoke) writes to"; nothing
  writes it.
- **Impact**: Every `HasPerk` condition takes its `return 0.0` fallback on every actor, including FO4 NPCs whose `PRKR` perks were correctly parsed and stamped. Perk-gated dialogue, quest and package CTDAs silently evaluate false. Structurally it is also a CHARAL/NIFAL canonical-type violation (`charal.md` §2 "Introduce a new canonical type only where none exists"), so the divergence will widen as either component grows a consumer.
- **Related**: #1667 (CLOSED — implemented `HasPerk` against `PerkList`), ECS-2026-08-13-04 in `docs/audits/AUDIT_ECS_2026-08-13.md` (the same "built component, no producer" shape for `FactionReputation`), #1835 (CLOSED — the save-registry guard that first documented `PerkList`'s zero write sites)
- **Suggested Fix**: Collapse to one component. `Perks` is the canonical type per `charal.md` §4.3 and already carries rank, so repoint `HasPerk` at `Perks` (`Perks::rank(id) > 0`) and delete `PerkList`, or make `PerkList` a projection the spawn path also writes. Do not leave both with a live docstring claiming ownership.

### CHAR-D3-02: FO3 silently receives FNV's leveling model — `LevelingModel::FO3` and `fallout3_ruleset` are unreachable, and all three FO3≠FNV leveling constants are documented divergences
- **Severity**: MEDIUM
- **Dimension**: Leveling & Progression
- **Game**: FO3
- **Location**: `byroredux/src/npc_spawn.rs:157-166` (`build_character_ruleset`); `crates/core/src/character/fallout.rs:110` (`fallout3_ruleset`); `crates/core/src/character/leveling.rs:86-95` (`LevelingModel::FO3`); `crates/plugin/src/esm/reader.rs:93-109` (`GameKind`)
- **Status**: NEW
- **Source**: `docs/engine/charal-fnv-fo3-ruleset.md` § *XP / level curve — LOCKED* — "**Level cap:** FO3 **20** … FNV **30**"; "**Perk cadence:** FO3 = 1 perk **every level**; FNV = 1 perk **every other level**"; "`LevelReward` for FO3/FNV = `SkillPoints { base: 10, int_mult: 1.0 (FO3) / 0.5 (FNV), perk_cadence: 1 (FO3) / 2 (FNV) }`"
- **Description**: `build_character_ruleset` maps `GameKind::Fallout3NV` — which covers **both** FO3 and FNV — to `falloutnv_ruleset`, so `fallout3_ruleset` has no call site and `LevelingModel::FO3` is dead outside its own unit tests. The function's docstring justifies the collapse solely on **derived stats**: "the *actor-general* derived stats (Carry Weight / Melee Damage / Crit Chance / Unarmed Damage …) are identical between them". It never mentions that the collapse also substitutes FNV's **leveling model**, and the capture document lists three leveling constants as explicitly divergent: `level_cap` (20 vs 30), `int_mult` (1.0 vs 0.5) and `perk_cadence` (1 vs 2). The XP curve itself is shared (`150·L + 50`), which is why the one live consumer (`GetXPForNextLevel`) does not currently expose the substitution — the divergence hides entirely in methods nothing calls yet.
- **Evidence**:
  ```rust
  // byroredux/src/npc_spawn.rs:161-165
  Some(match game {
      GameKind::Fallout4 => byroredux_core::character::fallout4_ruleset(resolve),
      GameKind::Fallout3NV => byroredux_core::character::falloutnv_ruleset(resolve),
      _ => return None,
  })
  ```
  `grep -rn "fallout3_ruleset"` outside `crates/core/src/character/` returns nothing.
  `GameKind::from_header` *does* receive the discriminating `hedr_version` (FO3 0.85 vs FNV 1.34) before collapsing them, so the information is available at the boundary and is discarded there, not absent.
- **Impact**: Latent today (no leveling runtime consumes `skill_points` / `grants_perk_at` / `level_cap`). Whenever leveling lands, an FO3 load order silently gets half its skill points per level, perks every *other* level instead of every level, and a cap of 30 instead of 20 — with no failing test, because the correct constructor is dead code that nothing exercises against a real game. Distinct from the deferred FO3↔FNV **player Health/AP** item in the known-open register: these are leveling constants, not derived stats.
- **Related**: The known-open FO3↔FNV player Health/AP deferral (`build_character_ruleset` docstring) — adjacent but a different set of values; `/audit-character` Dim 5 owns the derived-stat half of the same collapse.
- **Suggested Fix**: Either thread the already-available HEDR version (or master name) through so `GameKind::Fallout3NV` can select `fallout3_ruleset`, or — if the collapse is intentional for now — extend `build_character_ruleset`'s docstring to state plainly that FO3 inherits FNV's `level_cap` / `int_mult` / `perk_cadence`, and add a test pinning that deliberate divergence so it cannot be mistaken for correctness.

### CHAR-D3-03: every leveling constant is a hardcoded engine literal, shadowing 2,039 parsed-but-unreadable GMSTs, with no editor-ID accessor to resolve them
- **Severity**: MEDIUM
- **Dimension**: Leveling & Progression
- **Game**: all
- **Location**: `crates/core/src/character/leveling.rs:75-125` (all five model consts); `crates/core/src/character/skyrim.rs:36` (`SKYRIM_SKILL_USE_CURVE`); `crates/plugin/src/esm/records/index.rs:64` (`game_settings`); `crates/plugin/src/esm/records/global.rs:91` (`parse_gmst`)
- **Status**: NEW
- **Source**: `docs/engine/charal.md` §3 table — "| XP / level curve (`iXPBase`, `iXPLevelUpBase`, …) | **AUTHORED** — `GMST` | pending |"
- **Description**: `charal.md` §3 classifies the XP/level curve as **AUTHORED** data that should be read from parsed `GMST` records, and `LevelingModel::SkillXp`'s own docstring names the three GMSTs it encodes (`fXPLevelUpBase` / `fXPLevelUpMult` / `fXPPerSkillRank`). Every one of them is a hardcoded `f32` literal instead. The parse side is not the blocker — `parse_gmst` runs and `EsmIndex::game_settings` holds them (a real FNV load indexes ~2,039 per `crates/plugin/tests/parse_real_esm.rs`) — but the map is keyed by **FormID**, and no editor-ID accessor exists (contrast `EsmIndex::actor_value_form_id`, which CHARAL already uses for every AVIF). So no CHARAL code path *can* resolve a GMST today even if it wanted to. `charal.md` §8 item 6 scopes the open "GMST sourcing" gap to `actor_value_derive.rs`'s `fAVDSkill*` constants and calls them "the last real AUTHORED gap" — contradicted by §3's own separate, still-`pending` XP-curve row.
- **Evidence**:
  ```rust
  // crates/core/src/character/leveling.rs:119-125 — GMST-named, GMST-shaped, hardcoded
  pub const SKYRIM: Self = Self::SkillXp {
      xp_base: 75.0, xp_mult: 25.0, xp_per_skill_rank: 1.0,
      pool_pick_gain: 10.0, level_cap: 0,
  };
  ```
  ```
  crates/plugin/src/esm/records/index.rs:64: pub game_settings: HashMap<u32, GameSetting>,
  ```
  `grep -rn "game_settings"` outside `index.rs` returns only the dispatch insert
  (`dispatch_global.rs:21`) and test count assertions — zero readers.
- **Impact**: Any mod that retunes the XP curve, the Skyrim per-skill-rank award, or `fSkillUseCurve` is silently ignored — the class of breakage the CHARAL AUTHORED/ENGINE-SUPPLIED split exists to prevent. Reachable today only through `GetXPForNextLevel` (which returns vanilla values under a modded load order); the blast radius becomes the whole progression system the moment leveling is implemented.
- **Related**: `charal.md` §8 item 6 (the acknowledged `fAVDSkill*` half); `/audit-esm` owns the `EsmIndex` accessor surface
- **Suggested Fix**: Add an editor-ID→`GameSetting` accessor on `EsmIndex` mirroring `actor_value_form_id`, then make the per-game builders take a `gmst(&str) -> Option<f32>` resolver alongside the existing `resolve`, falling back to today's literals when a setting is absent (the same resolve-or-skip contract the derived tables already use). Correct `charal.md` §8 item 6's "last real AUTHORED gap" claim either way.

### CHAR-D3-04: `level_cap` has no consumer anywhere, and its docstring describes a DLC bump that no loader performs
- **Severity**: LOW
- **Dimension**: Leveling & Progression
- **Game**: FO3 / FNV (the only capped models)
- **Location**: `crates/core/src/character/leveling.rs:43-50` and `:201-210`; sole model consumer at `crates/scripting/src/condition.rs:574-585`
- **Status**: NEW
- **Source**: `docs/engine/charal-fnv-fo3-ruleset.md` § *XP / level curve* — "**Level cap:** FO3 **20** (30 with *Broken Steel*); FNV **30** (50 with the four add-ons, +5 each)"
- **Description**: `level_cap()` is correct and its `0 = uncapped` sentinel is structurally un-divergable (one merged match arm across all three variants), but nothing calls it — not `GetXPForNextLevel`, not the spawn path, not any test beyond `level_caps_per_game`, which only reads the raw stored values back. The doc comments assert behavior that does not exist: `"a hard `level_cap` (`0` = uncapped; add-ons raise it)"` and `"Add-ons raise it; the loader bumps it when DLC is present."` A repo-wide grep finds no code that raises a level cap for any DLC, and `build_character_ruleset` does not inspect the load order for add-ons at all. Consequently `GetXPForNextLevel` on an actor at or above the cap returns a positive XP requirement rather than reflecting the cap. I deliberately do not assert what the capped return *should* be — no capture document states it (`feedback_no_guessing`).
- **Evidence**:
  ```rust
  // crates/core/src/character/leveling.rs:201-202 — a claim about the loader
  /// The base-game hard level cap (`0` = uncapped). Add-ons raise it; the
  /// loader bumps it when DLC is present.
  ```
  ```rust
  // crates/scripting/src/condition.rs:584 — the only model consumer, cap-blind
  rs.leveling.xp_to_next(level)
  ```
  `grep -rn "level_cap"` outside `leveling.rs` returns nothing.
- **Impact**: Small today — one CTDA returns a vanilla-curve value past the cap. The real cost is the docstring: it reads as a description of shipped behavior and will be believed by whoever builds the leveling runtime, who then will not implement the DLC bump because they think the loader already does.
- **Related**: CHAR-D3-02 (the same accessor set is where the FO3/FNV divergence hides)
- **Suggested Fix**: Reword both doc comments to the imperative ("add-ons *should* raise it; not yet implemented") or file the DLC-bump work, and have `xp_to_next`'s caller consult `level_cap()` once the capped semantics are sourced.

### CHAR-D3-05: `Perks::set_rank` performs no rank validation at all — neither rejects nor clamps — while the capture document names `Perks` as the component that validates the gating half
- **Severity**: LOW
- **Dimension**: Leveling & Progression
- **Game**: FO4 (the only game whose `PRKR` ranks reach the component today)
- **Location**: `crates/core/src/character/components.rs:57-69` (`set_rank`); writer at `byroredux/src/npc_spawn.rs:132-144`; the available max-rank data at `crates/plugin/src/esm/records/misc/magic.rs:241-244` (`PerkRecord::num_ranks`)
- **Status**: NEW
- **Source**: `docs/engine/charal-fo4-ruleset.md` § *Perk chart — COMPLETE* — "Each perk has **1–5 ranks** … a perk is takeable iff `SPECIAL ≥ Val ∧ character_level ≥ rank_gate ∧ owns(prev_rank)` … Rank counts range 2–5 … This is the *gating* half the `Perks` component validates against"
- **Description**: The capture document states that `Perks` is where the gating half is validated. It validates nothing: `set_rank` writes any `u8` unconditionally, and the spawn path copies the `PRKR` rank byte straight through with no bound. The checklist's required behavior — a rank beyond the perk's declared max is *rejected*, not silently clamped — is not implemented in either form. The data needed to enforce it is already parsed: `PerkRecord::num_ranks` is decoded from the PERK `DATA` sub-record for both the FO3/FNV and Skyrim layouts. Two smaller hygiene gaps ride along: `set_rank(id, 0)` inserts an entry indistinguishable from "not owned" (`rank()` returns `0` for both), and there is no removal API, so the `PERK` lifecycle documented in project memory `perk_system` ("automatically removed when the perk is removed") has no ECS expression.
- **Evidence**:
  ```rust
  // crates/core/src/character/components.rs:59-69 — no bound, no Result, no max
  pub fn set_rank(&mut self, perk_form_id: u32, rank: u8) {
      if let Some(p) = self.entries.iter_mut().find(|p| p.perk_form_id == perk_form_id) {
          p.rank = rank;
      } else { self.entries.push(PerkRank { perk_form_id, rank }); }
  }
  ```
  ```rust
  // crates/plugin/src/esm/records/misc/magic.rs:241-244 — the max the component never sees
  /// DATA num_ranks (count of multi-rank steps). 1 for most perks;
  /// 3–5 for Skyrim skill-tree perks with progressive ranks.
  pub num_ranks: u8,
  ```
- **Impact**: Currently bounded by the fact that authored `PRKR` ranks are well-formed and nothing reads `Perks` ranks yet (see CHAR-D3-01). It becomes real the moment a level-up path or the perk entry-point pipeline calls `set_rank`: an out-of-range rank would select entries that do not exist on the `PERK` record, with no error and no clamp.
- **Related**: CHAR-D3-01 (same component, no reader); project memory `perk_system` / `perk_entry_points`
- **Suggested Fix**: Give `set_rank` a fallible sibling that takes the perk's `num_ranks` and rejects `rank == 0 || rank > num_ranks`, and add a `remove` to mirror `PerkList::remove`. If the max cannot be plumbed at the call site, at least make rank `0` a documented no-op rather than a stored ghost entry.

### CHAR-D3-06: the Skyrim and Oblivion leveling constants appear in no per-game capture document — their only record is `charal.md`'s implementation-summary prose
- **Severity**: LOW
- **Dimension**: Leveling & Progression
- **Game**: Skyrim, Oblivion
- **Location**: `crates/core/src/character/leveling.rs:59-72` and `:110-125`; `crates/core/src/character/skyrim.rs:33-36`; documents `docs/engine/charal-skyrim-ruleset.md`, `docs/engine/charal-oblivion-ruleset.md`
- **Status**: NEW
- **Source**: `docs/engine/charal.md` §5 — "`SkillXp { xp_base, xp_mult, xp_per_skill_rank, pool_pick_gain, level_cap }` (Skyrim — `SKYRIM` = 25·L+75 XP, 1 XP/skill rank, +10 pool pick + perk/level; UESP-sourced)" — the sole documentary record of these values
- **Description**: `_audit-common.md` and this skill both designate the six `charal-*-ruleset.md` captures as "the authority for every constant". Neither `charal-skyrim-ruleset.md` (18 sections) nor `charal-oblivion-ruleset.md` (13 sections) contains a leveling or XP-curve section at all — unlike `charal-fnv-fo3-ruleset.md` and `charal-fo4-ruleset.md`, which each carry a dedicated "## XP / level curve — LOCKED" section naming the source page. The Skyrim XP constants, Oblivion's 10-major-skill-ups threshold, the `+1..5` attribute-bonus bands and `fSkillUseCurve = 1.95` are recorded only in `charal.md` §3/§5, which are *implementation summaries* — prose describing what shipped rather than a capture that preceded it. The sourcing is therefore circular: the document that verifies the code was written from the code. The three GMST names the code cites for the Skyrim curve — `fXPLevelUpBase`, `fXPLevelUpMult`, `fXPPerSkillRank` — appear in **no** document in the repository; a grep across `docs/` finds them only in `leveling.rs`'s own docstring. Two of the affected numbers do have independent per-game confirmation and are exempt: `SKYRIM_POOL_BASE = 100` and `pool_pick_gain = 10` are both confirmed by `charal-skyrim-ruleset.md` § *Magicka*.
- **Evidence**:
  ```
  $ grep -rn "fXPLevelUp\|fXPPerSkillRank" docs/ crates/ byroredux/
  crates/core/src/character/leveling.rs:64:    /// UESP *Skyrim:Leveling* (`fXPLevelUpBase`/`fXPLevelUpMult`/
  crates/core/src/character/leveling.rs:65:    /// `fXPPerSkillRank`).
  $ grep -n "^## " docs/engine/charal-skyrim-ruleset.md   # 18 sections, none on leveling
  $ grep -n "^## " docs/engine/charal-oblivion-ruleset.md # 13 sections, none on leveling
  ```
- **Impact**: These values cannot be audited. A future audit re-running this dimension will find the same closed loop and be unable to do better than "code matches the paragraph describing the code". Both models are unwired today, so nothing is currently mis-statted — this is a verification gap, not a wrong number, and it is reported at LOW precisely because no live path consumes them.
- **Related**: `charal.md` §9 "TES skill → governing-attribute maps + leveling curves — **mostly closed**" (the claim this finding qualifies); CHAR-D3-03 (the same constants, the GMST angle)
- **Suggested Fix**: Add a "## XP / level curve — LOCKED" section to `charal-skyrim-ruleset.md` and a "## Leveling" section to `charal-oblivion-ruleset.md` carrying the GMST names, their values, and the UESP page each came from — matching the shape the two Fallout captures already use. No code change.

### CHAR-D3-07: `leveling.rs`'s module docstring calls `SkillXp` "a future third variant" and the Oblivion attribute bonus "deferred" — both shipped
- **Severity**: LOW
- **Dimension**: Leveling & Progression
- **Game**: Skyrim, Oblivion
- **Location**: `crates/core/src/character/leveling.rs:13-20` (module doc) vs `:59-72` (`SkillXp`) and `crates/core/src/character/tes.rs:190` (`oblivion_attribute_bonus`)
- **Status**: NEW
- **Source**: `docs/engine/charal.md` §5 — "`LevelingModel` is now an **enum** with all three shapes"; §3 row 147 — "shipped: `oblivion_attribute_bonus` (+1…+5), `skyrim_skill_xp_to_next` / `_between`"
- **Description**: The module docstring is the first thing a contributor reads in this file, and two of its statements are stale. Line 20 says "Skyrim's per-skill-XP model (`SkillXp`) is a future third variant" — `SkillXp` is a fully implemented variant 46 lines below it, with a `SKYRIM` const, three accessors and a passing test (`skyrim_skill_xp_matches_uesp`). Lines 16-18 describe the classic-TES level-up attribute bonuses as "the deferred leveling-efficiency mechanic (`docs/engine/charal.md` §5), not modelled here" — the "not modelled *here*" is literally true (it lives in `tes.rs`), but calling it *deferred* contradicts `charal.md` §3, which lists `oblivion_attribute_bonus` as shipped, and it invites exactly the duplicate implementation the global instruction against duplicating logic warns about.
- **Evidence**:
  ```rust
  // crates/core/src/character/leveling.rs:20
  //! Skyrim's per-skill-XP model (`SkillXp`) is a future third variant.
  ```
  ```rust
  // crates/core/src/character/leveling.rs:66 — 46 lines later
  SkillXp { xp_base: f32, xp_mult: f32, xp_per_skill_rank: f32, ... },
  ```
- **Impact**: Documentation only, but on the entry-point docstring of the module this dimension owns — the highest-traffic place for this class of rot.
- **Related**: `charal.md` §8's own struck-through-items note about the rollout list having drifted stale (`feedback_audit_findings`)
- **Suggested Fix**: Rewrite lines 13-20 to describe `SkillXp` as the shipped third variant and cross-link `tes::oblivion_attribute_bonus` as the classic-TES level-up mechanic's actual home.

### CHAR-D3-08: `CharacterLevel` and `Perks` are save-exempt as "re-derived from static ESM data" — an invariant that holds only while no progression runtime exists
- **Severity**: LOW
- **Dimension**: Leveling & Progression
- **Game**: all
- **Location**: `byroredux/src/save_io/round_trip_tests.rs:735-758` (`REDERIVED_NOT_SAVED`); the stamped state at `byroredux/src/npc_spawn.rs:110-144`; `crates/core/src/character/components.rs:12-27`
- **Status**: NEW
- **Source**: `docs/engine/charal.md` §4.2 — "`pub struct CharacterLevel { level: u16, xp: f32 /* progress toward next */ }` … The per-game leveling strategy (§5 `LevelingModel`) advances it."
- **Description**: The save registry deliberately excludes `CharacterLevel`, `Background` and `Perks`, classified as "Re-derived from static ESM `NPC_` data. Most entries are write-once." That is currently accurate: `spawn_npc_entity` writes `level` from `NPC_` and `xp: 0`, and `Perks` verbatim from `PRKR`. But `CharacterLevel.xp` is defined by CHARAL as *progress toward the next level* — accumulated runtime state that is by construction **not** derivable from static ESM data — and `Perks` will hold level-up-granted ranks the same way. The exemption is therefore a snapshot of "no leveling runtime exists yet" recorded as if it were a property of the components, with no tripwire that fires when the first XP award lands.
- **Evidence**:
  ```rust
  // byroredux/src/save_io/round_trip_tests.rs:749-756
  const REDERIVED_NOT_SAVED: &[&str] = &[
      "FactionRanks", "CharacterLevel", "Background", "Perks", ...
  ```
  ```rust
  // byroredux/src/npc_spawn.rs:119-122 — xp is a literal 0 today, which is what makes the claim true
  CharacterLevel { level: npc.level.max(0) as u16, xp: 0 },
  ```
- **Impact**: Zero today. On the day leveling ships, every save silently discards accumulated XP and every perk rank earned after spawn, and the round-trip guard passes because the component is on the allow-list. Same class as #1834 / #1835 and `AUDIT_ECS_2026-08-13`'s `FactionReputation` finding, but for progression state rather than reputation.
- **Related**: #1834, #1835 (both CLOSED — the `ActorValues` / `PerkList` save gaps), ECS-2026-08-13-04; `/audit-save` owns the registry itself
- **Suggested Fix**: Narrow the exemption comment to say *why* it holds ("`xp` is always 0 and `Perks` is `PRKR`-verbatim until a leveling runtime exists") and add an assertion that `CharacterLevel::xp == 0` at snapshot time, so the exemption breaks loudly rather than silently the moment progression starts writing.

---

## Cross-audit routing

- CHAR-D3-01's `PerkList` deletion and CHAR-D3-08's registry entry touch `/audit-save` and `/audit-ecs` (component shape / snapshot completeness) — filed here because both are about the progression model's reachability, which is this dimension's scope.
- CHAR-D3-03's `EsmIndex` accessor is `/audit-esm` surface; the CHARAL side is filed here.
- CHAR-D3-02 overlaps Dimension 5's FO3↔FNV collapse check from the *leveling* angle only; the derived-stat half is Dim 5's.

## Known-open register (confirmed not re-filed)

- FNV/FO3 **tag-skill per-level** growth — not touched; no finding references it.
- FO3↔FNV divergent **player Health/AP** — explicitly distinguished from CHAR-D3-02, which concerns `level_cap` / `int_mult` / `perk_cadence`, not Health/AP.
- **VATS runtime** — out of this dimension entirely; not referenced.

---

## Dimension 4



**Date**: 2026-08-15 · **Depth**: deep · **Games**: all (Oblivion regen, FO3/FNV
resistance + Karma + Reputation, FO4 affinity)

## Scope & Coverage

**Capture documents read (before any Rust, in the mandated order)**
- `docs/engine/charal.md` (full) — §4.5/§4.6 reputation + affliction storage, §7.1
  the six reputation-family instances (FO4 affinity constants live here).
- `docs/engine/charal-fnv-fo3-ruleset.md` (full) — Karma, FNV Reputation, Radiation
  / Poison Resistance.
- `docs/engine/charal-fo4-ruleset.md` (full) — FO4 resistance re-architecture, AP
  regen rate.
- `docs/engine/charal-oblivion-ruleset.md` (Fatigue / Health / Magicka regen
  sections, lines 370–580) — the only source for every regen constant.
- `docs/engine/charal-skyrim-ruleset.md` (Disease / Vampirism sections) — the
  "do NOT reuse `AfflictionTable`" boundary.
- `docs/engine/charal-fo76-ruleset.md` (full) — Disease Resistance, the third
  resistance-family shape.

**Code read**
- `crates/core/src/character/regen.rs` (full)
- `crates/core/src/character/affliction.rs` (full)
- `crates/core/src/character/resistance.rs` (full)
- `crates/core/src/character/reputation.rs` (full)
- `crates/core/src/character/components.rs` (full — `FactionReputation`, the
  reputation family's storage half)
- `crates/core/src/character/tes.rs` (`oblivion_pool_regen_config`,
  `oblivion_ruleset`, the three pool formulas the regen tick chains through)
- `byroredux/src/boot.rs:845-887` (the regen registration site)
- `crates/scripting/src/condition.rs:690-730` (`GetReputation` /
  `GetReputationThreshold` — the only live consumer of anything in this dimension)
- `crates/core/src/ecs/components/actor_values.rs` (`restore`, the regen sink)
- `crates/core/src/ecs/components/faction_ranks.rs` (FACT-keyed sibling)
- `crates/physics/src/world.rs:360-400` (`PHYSICS_DT` / `MAX_SUBSTEPS`, the
  accumulator regen.rs claims to mirror)
- `byroredux/src/save_io.rs` (registry list, for the affliction/reputation
  save-pairing question)

**Tests**: `cargo test -p byroredux-core character` → **94 passed, 0 failed**.

**Verified clean (checklist items that survived every attempt to break them)**
- **Backlog clamp.** `PoolRegenAccumulator::advance` clamps the accumulator to
  `MAX_REGEN_SUBSTEPS × POOL_REGEN_DT` *before* draining, discards the surplus,
  and is pinned by `accumulator_ticks_at_sixty_hz_and_caps_catchup`
  (`advance(10.0) == 8`, and the next frame yields 0). No unbounded catch-up.
- **Zero / negative / NaN dt cannot spin.** `frame_dt.max(0.0)` absorbs negatives
  and NaN (Rust's `f32::max` returns the non-NaN operand); `ticks == 0` early-returns.
  `advance(f32::INFINITY)` saturates into the same clamp.
- **Fixed-tick quantisation.** Regen is applied as `rate × (ticks × POOL_REGEN_DT)`,
  never `rate × frame_dt`; fractional time carries across frames
  (`accumulator_carries_fractional_time_across_frames`). Batching the ticks into one
  application is exact here because both rates are constant within a frame.
- **Regen cannot overshoot the pool.** `ActorValues::restore` floors `damage` at
  `0.0` (`actor_values.rs:127-130`), so no separate max-pool clamp is needed —
  the module doc's claim is accurate.
- **Affliction diff-and-reapply is correct.** `reevaluate_affliction` reverses the
  *previous* band's penalties (`mod_temporary(-p.delta)`) before applying the new
  band's, and no-ops when the band is unchanged. Escalation, cure, and
  multi-affliction independence are each pinned by a test
  (`reevaluate_swaps_penalties_when_the_band_escalates` asserts "still exactly −1,
  not −2"). No compounding.
- **Band boundaries are half-open and total.** `AfflictionTable::band_for` uses
  `rposition(|b| pool >= b.min_pool)` → `[min_pool, next_min_pool)`; a value exactly
  on a boundary lands in exactly one band (`band_for(200.0) == Some(0)`,
  `band_for(199.9) == None`). Same for `karma_band`, `affinity_band`, and
  `FactionRepThresholds::range` — all `>=` ladders, all with exact-boundary tests.
- **Resistance can never exceed 100 %.** `damage_multiplier` clamps to `[0, cap]`
  then floors `1 − r/100` at `0` — 120 % resistance yields `0.0`, never a heal.
- **The 4×4 standing grid is NOT transposed.** `STANDING_GRID` is
  `[infamy_range][fame_range]` and `from_ranges(fame, infamy)` indexes `[i][f]`;
  every one of the 16 cells matches `charal-fnv-fo3-ruleset.md:489-494`
  (Infamy rows × Fame columns), including the asymmetric off-diagonals
  (`(fame 2, infamy 1) = Smiling Troublemaker`, `(fame 1, infamy 2) = Sneering
  Punk`). `FactionReputation::standing` passes `(fame, infamy)` in that order.
- **Both clamps bound both ends.** `clamp_karma` → `[-1000, +1000]`,
  `clamp_affinity` → `[-1000, +1100]`, each with a two-sided test.
- **No fabricated affliction thresholds.** No shipped `AfflictionTable` exists;
  the only tables are `stand_in_radiation_table()` inside `#[cfg(test)]`, labelled
  "NOT sourced data". This matches `charal.md:232-237` exactly — the mechanism
  shipped ahead of its data, deliberately.
- **All 13 `FactionRepThresholds` triples** match the capture doc's table row for
  row (Boomers 8/25/50 … The Strip 6/20/40).

**Could not verify**
- **Whether `MAX_REGEN_SUBSTEPS = 8` is a *game* constant.** It is engine-supplied
  tuning; the only document that states `8` is `charal-oblivion-ruleset.md:404`,
  which describes the code written the same day. Treated as PASS (self-consistent
  engine tunable), but see CHAR-D4-06 for the false parity claim attached to it.
- **The per-cell colour of the FNV standing grid.** No capture document records
  which of the 16 titles are green/black/red — only the one-line legend. See
  CHAR-D4-02.
- **Runtime behaviour of anything in this dimension.** Nothing here has a live
  producer: `PoolRegenConfig` is never inserted, `AfflictionStatus` is never
  stamped, `FactionReputation` is populated only in tests, and `karma_band` /
  `clamp_karma` / `affinity_*` / `damage_multiplier` have **no callers outside
  their own modules** (verified by workspace-wide grep). Every finding below is
  therefore latent-by-construction, which is the documented state of the layer,
  not a defect in itself.

**Deliberately not re-reported**: the `boot.rs` scheduler access declaration
(**#2153**, OPEN); FNV/FO3 tag-skill growth; FO3↔FNV player Health/AP; the missing
VATS runtime; `CHAR-D1-01` (Dim 1's `DerivedScope` finding — `pool_regen_tick_system`
is one of its downstream evaluators, since `oblivion_magicka_formula` is
`.player_only()` and the tick evaluates it for every actor; that is Dim 1's, not
mine); the CHARAL-component save-registry class (**#1835**, CLOSED with a
"register when a mutator lands" resolution — `AfflictionStatus` and
`FactionReputation` are the same class and neither has a mutator).

---

## Constant Table

Every regen rate, band threshold, resistance cap and reputation constant in the
dimension's four files. `document` cites the capture-document line the value comes
from; a row with no document value is `UNSOURCED` and is itself a finding.

| constant | code | document | verdict |
|---|---|---|---|
| `POOL_REGEN_DT` | `1.0/60.0` | `charal-oblivion-ruleset.md:403-405` | PASS |
| `MAX_REGEN_SUBSTEPS` | `8` | `charal-oblivion-ruleset.md:404` ("capped at … (8)") | PASS (value) — parity claim wrong, CHAR-D4-06 |
| `FATIGUE_REGEN_PER_SEC` | `10.0` | `charal-oblivion-ruleset.md:386-388` (`fFatigueReturnBase(10.0)`, `Mult(0.0)`) | PASS |
| `MAGICKA_REGEN_WILLPOWER_COEFF` | `0.02` | `charal-oblivion-ruleset.md:520` | PASS |
| `MAGICKA_REGEN_BASE` | `0.75` | `charal-oblivion-ruleset.md:520` | PASS |
| `magicka_regen_per_sec` shape `(WIL·c+b)·(MaxMagicka/100)` | as written | `charal-oblivion-ruleset.md:519-521` | PASS |
| `magicka_regen_per_sec` stunted → `0.0` | `0.0` | `charal-oblivion-ruleset.md:540-548` | PASS |
| `magicka_regen_per_sec` `max_magicka<=0` → `0.0` | `0.0` | — (defensive guard, not a game constant) | PASS (n/a) |
| Health passive regen | **absent** | `charal-oblivion-ruleset.md:459-468` ("NO passive regeneration at all") | PASS (correct absence) |
| `Affliction::RADIATION.derive_coeff` | `2.0` | `charal-fnv-fo3-ruleset.md:91, 264` (`(END−1)·2`) | PASS |
| `Affliction::RADIATION.resist_cap` | `85.0` | `charal-fnv-fo3-ruleset.md:91, 264` (cap 85 %) | PASS |
| `Affliction::POISON.derive_coeff` | `5.0` | `charal-fnv-fo3-ruleset.md:92, 278` (`(END−1)·5`) | PASS |
| `Affliction::POISON.resist_cap` | `f32::INFINITY` | `charal-fnv-fo3-ruleset.md:280` ("No documented FO3/FNV cap") | PASS |
| `fo3_fnv_resistance_formula` bias | `-derive_coeff` | `charal-fnv-fo3-ruleset.md:264, 278` (`k·END − k`) | PASS |
| `damage_multiplier` curve | `1 − r/100`, `r` clamped `[0,cap]`, floored `0` | `charal-fnv-fo3-ruleset.md:264` ("damage is reduced by this percentage") | PASS (cannot exceed 100 %) |
| FO4 resistance derivation | **absent** | `charal-fo4-ruleset.md:283-298` (FO4 dropped the END derivation; curve unsourced) | PASS (correct absence) |
| FO76 `DiseaseResistMult` | **absent** | `charal-fo76-ruleset.md:71-83` | PASS (not claimed built) |
| `AfflictionTable` shipped thresholds | **none** | `charal.md:232-237` (PENDING) | PASS (correct absence) |
| `KARMA_MIN` / `KARMA_MAX` | `-1000` / `1000` | `charal-fnv-fo3-ruleset.md:358` | PASS |
| `KARMA_VERY_GOOD_MIN` | `750` | `charal-fnv-fo3-ruleset.md:369` | PASS |
| `KARMA_GOOD_MIN` | `250` | `charal-fnv-fo3-ruleset.md:370` | PASS |
| `KARMA_NEUTRAL_MIN` | `-249` | `charal-fnv-fo3-ruleset.md:371` | PASS |
| `KARMA_EVIL_MIN` | `-749` | `charal-fnv-fo3-ruleset.md:372` | PASS |
| `KarmaBand::name` ×5 | Very Evil…Very Good | `charal-fnv-fo3-ruleset.md:366-374` | PASS |
| `REPUTATION_BUMP_POINTS` | `[0,1,2,4,7,12]` | `charal-fnv-fo3-ruleset.md:452-456` | PASS |
| `REPUTATION_AXIS_MAX` | `100` | `charal-fnv-fo3-ruleset.md:434` | PASS |
| 13 × `FactionRepThresholds` (r1/r2/r3) | see `fnv_faction_thresholds` | `charal-fnv-fo3-ruleset.md:462-477` | PASS (13/13) |
| `BY_FORM_ID` — Boomers / NCR / Legion / BoS | `0xFFAE8 / 0xF43DE / 0xF43DD / 0x11E662` | `charal-fnv-fo3-ruleset.md:480-484` | PASS |
| `BY_FORM_ID` — the other 9 FormIDs | Followers … The Strip | *(doc lists 4 "e.g." of 13)* | **UNSOURCED** — CHAR-D4-04 (verified correct vs `FalloutNV.esm` in this audit) |
| `STANDING_GRID` 16 titles + `[infamy][fame]` orientation | as written | `charal-fnv-fo3-ruleset.md:489-494` | PASS (not transposed) |
| `ReputationStanding::sentiment` 16→3 bucketing | 5 Positive / 5 Negative / 6 Mixed | `charal-fnv-fo3-ruleset.md:486-487` gives the colour *legend* only | **UNSOURCED** — CHAR-D4-02 |
| `AFFINITY_MIN` / `AFFINITY_MAX` | `-1000` / `1100` | `charal.md:424-425` | PASS |
| Affinity band thresholds | `1000/750/500/250/0/-500` | `charal.md:425-426` | PASS |
| `AffinityBand::name` ×6 (Hatred…Confidant) | as written | `charal.md:425-426` | PASS |
| `AffinityBand::Idolize.name()` | `"Infatuation"` | `charal.md:426` names the band **Idolize** | **FAIL** — CHAR-D4-01 |
| Affinity reaction deltas | `±15` like/dislike, `±35` love/hate | `charal.md:426-428` | PASS |
| `AffinityReactionSize` scalars | `0.5 / 1.0 / 1.5` | `charal.md:428` (`CA_Size_{Small,Normal,Large}`) | PASS |
| `affinity_passive_gain` | `40 − 0.033·a` | `charal.md:428-430` (worked: 500 → +23.5) | PASS |
| `Affliction` struct size (doc comment) | "24 bytes" then "40 bytes" | — (self-contradictory; test pins 40) | **FAIL** — CHAR-D4-07 |

**Tallies**: 32 PASS · 2 FAIL · 2 UNSOURCED.

---

## Findings

### CHAR-D4-01: `AffinityBand::Idolize.name()` returns "Infatuation" — a band name no capture document contains
- **Severity**: LOW
- **Dimension**: Pools, Afflictions & Reputation
- **Game**: fo4
- **Location**: `crates/core/src/character/reputation.rs:252-265` (`AffinityBand::name`)
- **Status**: NEW
- **Source**: `docs/engine/charal.md:425-426` — "7 bands (Hatred/Disdain/Neutral/Friend/Admiration/**Confidant/Idolize**) at thresholds `-500/0/250/500/750/1000`".
- **Description**: Six of the seven `AffinityBand` variants return exactly the name
  the capture document records. The seventh returns `"Infatuation"`, a string that
  appears in **no** CHARAL capture document (`grep -rn "Infatuation" docs/` → no
  hits; the only near-match anywhere in the corpus is FNV Reputation's unrelated
  `Idolized` grid cell). The method's own doc comment asserts provenance it does not
  have — "The wiki's relationship name for this band."
- **Evidence**:
  ```rust
  AffinityBand::Confidant => "Confidant",
  AffinityBand::Idolize => "Infatuation",   // enum says Idolize, name says Infatuation
  ```
  No test asserts any `AffinityBand::name()` value — `affinity_bands_at_exact_boundaries`
  and `affinity_band_is_ordered_and_one_byte` cover thresholds and layout only, so
  nothing pins the string.
- **Impact**: The max-affinity band is the one that gates the companion perk, so it
  is the band most likely to be surfaced in UI or a quest condition. Any consumer
  displaying or string-matching `.name()` gets a label that disagrees with both the
  enum variant and the capture document. No gameplay path today (no caller).
- **Related**: CHAR-D4-02 (the other unsourced classifier string set in this file).
- **Suggested Fix**: Either rename the returned string to `"Idolize"`, or — if
  "Infatuation" is the real FO4 in-game label — add the citation to
  `charal.md` §7.1 and rename the variant to match. Pin whichever wins with a
  `name()` test, as `KarmaBand`/`ReputationStanding` effectively have via their
  assert messages.

### CHAR-D4-02: The 4×4 standing sentiment bucketing is unsourced, and its test only restates the code
- **Severity**: LOW
- **Dimension**: Pools, Afflictions & Reputation
- **Game**: fnv
- **Location**: `crates/core/src/character/reputation.rs:362-375` (`ReputationSentiment`), `:433-447` (`ReputationStanding::sentiment`), test `standing_sentiment_matches_grid_colours` (`:560-588`)
- **Status**: NEW
- **Source**: `docs/engine/charal-fnv-fo3-ruleset.md:486-487` — the grid is captured as
  "16 standing titles (shared across all factions; positive=green, mixed=black,
  negative=red)" followed by the title table at `:489-494`. The document records the
  **titles** and the colour **legend**; it never records which title is which colour.
- **Description**: `sentiment()` assigns all 16 cells to three buckets (5 Positive /
  5 Negative / 6 Mixed). That mapping has no capture-document backing. It is
  *internally* plausible — green ⇔ fame-range > infamy-range with infamy ≤ 1, red the
  mirror image, black the diagonal — but it is not derivable from the doc, and the
  two cells that break the "higher axis wins" rule (`DarkHero` at fame 3/infamy 2 and
  `SoftHeartedDevil` at fame 2/infamy 3 are both Mixed, not Positive/Negative) are
  exactly where a from-memory transcription would go wrong without anyone noticing.
- **Evidence**: the test that is supposed to guard this is
  `standing_sentiment_matches_grid_colours`, which iterates three hardcoded lists and
  asserts `s.sentiment()` equals the bucket the same source file just assigned — it
  can only fail if someone edits one of the two lists and not the other. It cannot
  detect a mis-transcribed colour, despite its name claiming fidelity to "grid
  colours".
- **Impact**: Consumers reading `sentiment()` (the intended shape is dialogue/vendor
  hostility gating) would get a plausible-looking wrong answer for the two ambiguous
  cells. Latent — no caller today.
- **Related**: CHAR-D4-01, CHAR-D4-04 (both are unsourced values in the same file).
- **Suggested Fix**: Add the per-cell colour to
  `charal-fnv-fo3-ruleset.md`'s grid (the source page renders them), then cite it
  from the `sentiment()` doc comment; or drop `sentiment()` until a caller needs it,
  rather than shipping an unsourced classifier under a test that reads as verified.

### CHAR-D4-03: `pool_regen_tick_system` has a second, undocumented prerequisite (`PoolRegenAccumulator`) that nothing inserts — the tick will not go live when the config wiring lands
- **Severity**: MEDIUM
- **Dimension**: Pools, Afflictions & Reputation
- **Game**: oblivion (the only game with a `PoolRegenConfig` builder)
- **Location**: `crates/core/src/character/regen.rs:111-131` (`pool_regen_tick_system` doc + preamble), `byroredux/src/boot.rs:855-879` (registration comment)
- **Status**: NEW
- **Source**: n/a — a wiring/precondition defect, not a numeric one.
- **Description**: The tick needs **two** resources: `PoolRegenConfig` *and*
  `PoolRegenAccumulator`. Its docstring names only the first ("A no-op if
  `PoolRegenConfig` hasn't been inserted … or if less than one 60 Hz tick has
  elapsed"), and `boot.rs`'s comment states the system is "registered now so the
  tick is already live the moment that wiring lands", where "that wiring" is
  described purely as the `PoolRegenConfig` insertion via `build_character_ruleset`.
  A workspace-wide grep finds **no insertion site for either resource** —
  `PoolRegenAccumulator` appears only in `regen.rs` (definition + tests) and in
  `boot.rs`'s `Access` declaration. `World::try_resource_mut` does not
  default-insert, so a wiring commit that inserts only the config leaves the system
  silently dead forever.
- **Evidence**:
  ```rust
  let Some(config) = world.try_resource::<PoolRegenConfig>() else { return; };
  let Some(mut accumulator) = world.try_resource_mut::<PoolRegenAccumulator>() else { return; };  // undocumented second gate
  ```
  Both of `regen.rs`'s live-path tests construct the accumulator by hand
  (`world.insert_resource(PoolRegenAccumulator::default())`), so the test suite
  cannot catch the omission either; `tick_system_is_a_noop_without_config` asserts
  the no-op is *tolerated*, never that it stops being one.
- **Impact**: When Oblivion's `CharacterRuleset` wiring lands, Fatigue and Magicka
  regeneration are dead with no error, no warning, and no failing test — a
  gameplay system that looks wired (registered in `boot.rs`, declared in
  `sys.accesses`) but never runs. Diagnosing it means noticing that a system
  registered specifically to be "already live" is returning at its second line.
- **Related**: #2153 (the same system's lock-stack declaration, OPEN, routed to
  `/audit-concurrency`); *CHAR-D1-01* (the same system evaluates a `PlayerOnly`
  derived formula for every actor).
- **Suggested Fix**: Insert `PoolRegenAccumulator::default()` unconditionally at
  boot (it is `Default` + `Copy` and harmless when no config exists), or fold the
  accumulator into `PoolRegenConfig` so a single insertion arms the whole tick.
  Either way, correct the docstring and the `boot.rs` comment to name both
  preconditions.

### CHAR-D4-04: Nine of the thirteen `BY_FORM_ID` reputation FormIDs have no capture-document value
- **Severity**: LOW
- **Dimension**: Pools, Afflictions & Reputation
- **Game**: fnv
- **Location**: `crates/core/src/character/reputation.rs:191-218` (`fnv_faction_thresholds::BY_FORM_ID`, `thresholds_for`)
- **Status**: NEW
- **Source**: `docs/engine/charal-fnv-fo3-ruleset.md:480-484` — "the canonical
  FalloutNV.esm faction FormIDs are now captured (*Gamebryo console commands*) …
  **e.g.** Boomers `000FFAE8`, NCR `000F43DE`, Legion `000F43DD`, BoS `0011E662`".
  Four of thirteen values were transcribed; the remaining nine (Followers, Great
  Khans, Powder Gangers, White Glove Society, Freeside, Goodsprings, Novac, Primm,
  The Strip) exist only in code.
- **Description**: The keys of the fallback lookup — the values that decide whether
  `GetReputationThreshold` finds a faction at all — are 69 % unsourced by the
  capture layer that is supposed to be the authority for every constant.
- **Evidence**: verified this audit against real game data rather than left open:
  scanning `FalloutNV.esm` for each FormID's owning record header shows **all
  thirteen are `REPU` records**, which is the record type `GetReputation`'s
  `param_1` carries — so the shipped values are *correct*. The gap is documentary,
  not numeric:
  ```
  BOOMERS [('REPU','0xffae8')]  BOS [('REPU','0x11e662')]  LEGION [('REPU','0xf43dd')]
  FOLLOWERS [('REPU','0x124ad1')]  GREAT_KHANS [('REPU','0x11989b')]  … 13/13 REPU
  ```
- **Impact**: None to runtime today. The risk is that a future correction or
  extension of the table has nothing to check itself against, and the next audit
  must re-derive from game data (as this one did) instead of diffing a document.
- **Related**: CHAR-D4-05 (same table, provenance described wrongly).
- **Suggested Fix**: Add the full 13-row `(REPU FormID, r1/r2/r3)` table to
  `charal-fnv-fo3-ruleset.md`, replacing the "e.g." list, and note that the values
  were confirmed against `FalloutNV.esm` record headers.

### CHAR-D4-05: `fnv_faction_thresholds` is keyed by REPU FormIDs but named and documented as FACT faction data
- **Severity**: LOW
- **Dimension**: Pools, Afflictions & Reputation
- **Game**: fnv
- **Location**: `crates/core/src/character/reputation.rs:131-137` (`FactionRepThresholds` doc), `:170-172` + `:191-195` (`fnv_faction_thresholds`, `BY_FORM_ID` doc), `:212-213` (`thresholds_for` doc); storage side `crates/core/src/character/components.rs:100-110` (`FactionStanding::faction_form_id`)
- **Status**: NEW
- **Source**: `docs/engine/charal-fnv-fo3-ruleset.md:440-442` — "All take `param_1` =
  the `REPU` FormID (`ptReputation` — **reputation is its own REPU record, not the
  FACT faction**)".
- **Description**: Three separate doc comments in `reputation.rs` name the FACT
  faction record as the authoritative source — "vanilla FNV values live on the
  faction record", "the authoritative source remains the parsed faction record",
  "thresholds for a faction by its FalloutNV.esm base FormID" — and the stored key
  is called `faction_form_id`. The keys are in fact **REPU** FormIDs (verified in
  CHAR-D4-04), and `crates/plugin` already parses `REPU` as its own record type
  (`dispatch_misc_gameplay_b.rs:126-133`, `EsmIndex` reputations). The identically
  named `faction_form_id` on `FactionRanks`
  (`crates/core/src/ecs/components/faction_ranks.rs:25`) holds genuine **FACT**
  FormIDs from `NPC_.SNAM` — two different FormID spaces behind one field name in
  the same crate.
- **Evidence**: `condition.rs:700` gets it right ("`param_1` is the global-space
  `REPU` FormID"), which is precisely why the mismatch is invisible: the one live
  caller compensates for prose that would mislead the next one. A future path that
  resolves a faction from `FactionRanks` and passes it to
  `FactionReputation::fame()` / `thresholds_for()` gets `0` / `Range 0` / `Neutral`
  — a silently plausible answer, never an error.
- **Impact**: Latent. `FactionReputation` has no production producer yet, so the
  wrong-space lookup cannot occur today; the cost is a documented invitation to
  wire the wrong record type when it does.
- **Related**: CHAR-D4-04.
- **Suggested Fix**: Rename to `repu_form_id` (or document the key as "the REPU
  record's FormID, not the FACT faction's") across `FactionStanding`,
  `FactionReputation`'s accessors, and `thresholds_for`, and correct the three
  "faction record" provenance sentences to name `REPU`.

### CHAR-D4-06: `MAX_REGEN_SUBSTEPS` claims to mirror `crates/physics::MAX_SUBSTEPS`, which is 5, not 8
- **Severity**: LOW
- **Dimension**: Pools, Afflictions & Reputation
- **Game**: all
- **Location**: `crates/core/src/character/regen.rs:45-48` (`MAX_REGEN_SUBSTEPS` doc), `:71-73` (`PoolRegenAccumulator` doc)
- **Status**: NEW
- **Source**: `docs/engine/charal-oblivion-ruleset.md:401-405` — the design intent is
  "one global fixed-step clock **mirroring `crates/physics::PhysicsWorld`'s own
  accumulator** — the only other fixed-timestep precedent in the engine", capped at
  "`MAX_REGEN_SUBSTEPS` (8)".
- **Description**: `POOL_REGEN_DT`'s "Matches `crates/physics::PHYSICS_DT`" is true
  (both `1.0/60.0`). The sibling claim on the substep cap is not:
  `crates/physics/src/world.rs:15` defines `MAX_SUBSTEPS = 5`, while
  `MAX_REGEN_SUBSTEPS = 8`, under a doc comment reading "Mirrors
  `crates/physics::MAX_SUBSTEPS`."
- **Evidence**: the clamp bodies are otherwise character-for-character identical
  (`accumulator += dt.max(0.0)`; clamp to `N × DT`; floor-divide; subtract), so the
  only divergence is the constant the comment asserts parity on. Behaviourally the
  two clocks drift during a hitch: regen advances up to 133 ms of simulated pool
  time per frame where physics advances at most 83 ms.
- **Impact**: Documentation-level today; the practical effect is a slightly larger
  post-hitch regen burst than the design intent describes, and a maintainer tuning
  one constant "to keep them in sync" would be reasoning from a false premise.
- **Related**: CHAR-D4-03 (same module's precondition doc).
- **Suggested Fix**: Either set `MAX_REGEN_SUBSTEPS = 5` (matching the claim and the
  physics clock) or keep 8 and replace "Mirrors" with the reason it differs.
  `crates/physics` also carries a second wall-clock guard (`SUBSTEP_TIME_BUDGET`)
  that regen has no analogue for — worth saying so if the mirroring language stays.

### CHAR-D4-07: `Affliction`'s doc comment states two different struct sizes in consecutive sentences
- **Severity**: LOW
- **Dimension**: Pools, Afflictions & Reputation
- **Game**: fnv / fo3
- **Location**: `crates/core/src/character/resistance.rs:48-64` (`Affliction` doc comment)
- **Status**: NEW
- **Source**: n/a — internal doc contradiction, no capture-document constant involved.
- **Description**: The comment says "**24 bytes**, `Copy`; the `&'static str`
  EditorIDs are resolved …" and then, two lines later, "**40 bytes** (two
  `&'static str` fat pointers + two `f32`), `Copy`." The pinned test
  `descriptors_are_copy_and_compact` asserts `size_of::<Affliction>() == 40`, so the
  first sentence is stale — a leftover from a pre-EditorID shape.
- **Evidence**: both sentences are live in the same doc block; the 24-byte claim has
  no supporting assertion anywhere.
- **Impact**: Cosmetic, but this crate uses struct-size assertions as real contracts
  (`AvPenalty` 8 B, `ActiveAffliction` 24 B, `FactionRepThresholds` 6 B), so a size
  claim that is wrong in the doc erodes the value of the ones that are right.
- **Related**: —
- **Suggested Fix**: Delete the "24 bytes" sentence.

---

## Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 1 |
| LOW | 6 |

The numeric core of this dimension is in good shape: every regen rate, every
resistance coefficient and cap, both Karma and Affinity band ladders, the bump
table, all 13 faction threshold triples, and the 4×4 standing grid (including its
orientation) match their capture documents exactly, and the affliction mechanism's
diff-and-reapply is provably non-compounding. The defects are at the edges — one
unsourced band name, one unsourced classifier, a wiring precondition that is
documented away, and three provenance/parity statements that do not survive
checking.

---

## Dimension 5



## Scope & Coverage

**Capture documents read (before any Rust, in the mandated order)**
- `docs/engine/charal.md` (full)
- `docs/engine/charal-fnv-fo3-ruleset.md` (full, 510 lines)
- `docs/engine/charal-fo4-ruleset.md` (full, 551 lines)
- `.claude/commands/_audit-common.md`, `.claude/commands/_audit-severity.md`,
  `.claude/commands/audit-character/SKILL.md` (Scope + Ground truth + Dimension 5)

**Code read**
- `byroredux/src/npc_spawn.rs` — `build_character_ruleset`, `stamp_actor_values`,
  `stamp_character_components`, `stamp_faction_ranks`, `build_npc_equip_state`
- `byroredux/src/npc_spawn/resumable.rs` — `spawn_placement_root` (the actual spawn tail)
- `crates/plugin/src/esm/records/actor_value_derive.rs` — `derive_npc_actor_values`,
  `derive_autocalc_actor_values`, `derive_stored_actor_values`, `base_skill`, `special_index`
- `crates/plugin/src/esm/records/actor/mod.rs` — `NpcRecord`, `parse_npc_core` (ACBS arms),
  `parse_npc_actor_values` (PRPS / DNAM / PRKR)
- `byroredux/src/cell_loader/references/mod.rs` — `load_references_budgeted` (the
  `CharacterRuleset` resource lookup)
- `byroredux/src/commands/actor_value.rs` — `SetAvCommand`, `ModAvCommand`, `edit_av`
- `crates/core/src/character/{ruleset,fallout,leveling}.rs` — `CharacterRuleset`,
  `push_derived`, `derived_value`, `fallout3_ruleset`, `falloutnv_ruleset`,
  `fallout4_ruleset`, `add_fnv_fo3_shared`, `LevelingModel`
- `crates/scripting/src/condition.rs` — `GetActorValue` / `GetXPForNextLevel` arms
- `crates/plugin/src/equip.rs` — `resolve_inherited_inventory`, `expand_leveled_form_id`,
  `TEMPLATE_FLAG_USE_INVENTORY`
- `byroredux/src/boot.rs` — the `pool_regen_tick_system` registration block
- `crates/plugin/src/esm/records/index.rs` — `actor_value_form_id`

**Real-data measurement.** A throwaway `#[ignore]` probe was compiled against
`byroredux-plugin`, run over vanilla `FalloutNV.esm` and `Fallout3.esm`, and then
deleted (tree left clean). Every population-rate number in this report comes from
that run, not from estimation.

**Verified CLEAN**
- **`None` handling (the dimension's highest-impact item).** `build_character_ruleset`
  has exactly **one** production caller — `load_references_budgeted`
  (`byroredux/src/cell_loader/references/mod.rs`) — and it is
  `if let Some(rs) = … { world.insert_resource(rs) }`. On `None` **no resource is
  inserted at all**; there is no `Default` impl on `CharacterRuleset` and no
  `world.resource::<CharacterRuleset>()` (panicking/defaulting) call anywhere. All
  three consumers take the `try_resource` path and return a documented neutral value:
  `GetActorValue` (`condition.rs`) falls through to `0.0`, `GetXPForNextLevel` returns
  `0.0`, `pool_regen_tick_system` (`regen.rs`) early-returns. A TES actor therefore
  cannot be handed Fallout formulas. **No finding.**
- **`resolve` is resolve-or-skip.** Every row in `fallout4_ruleset` /
  `fallout3_ruleset` / `falloutnv_ruleset` / `add_fnv_fo3_shared` is guarded by
  `if let (Some(out), Some(input)) = …` before `push_derived`, so an unresolved
  EditorID skips its formula rather than registering one keyed on `0`. The residual
  `Some(0)` angle is `CHAR-D2-04`'s and is not re-reported here.
- **FNV/FO3 *actor-general* coefficient parity.** Every actor-general row in
  `add_fnv_fo3_shared` matches the capture document **and is identical between the two
  games**: Carry Weight `150 + 10·STR`, Melee Damage `0.5·STR`, Critical Chance
  `0.01·Luck` cap `0.10`, Unarmed Damage `ceil(0.5 + 0.05·Unarmed)`, Radiation
  Resistance `(END−1)·2` cap 85, Poison Resistance `(END−1)·5` uncapped.
  *Source*: `charal-fnv-fo3-ruleset.md` "Derived statistics" table (all six rows carry
  identical FO3 and FNV columns). The docstring's stated justification for the
  `Fallout3NV` collapse therefore **holds for the derived table** — what it does not
  cover is `CHAR-D5-03`.
- **FNV Health algebra.** Code writes `95 + 20·END + 5·L`; the document writes
  `100 + END·20 + (Level−1)·5`. These are the same function. Not a finding.
  *Source*: `charal-fnv-fo3-ruleset.md` "Derived statistics" Health row.
- **FNV/FO3 auto-calc base skill.** `base_skill` implements
  `SKILL_BASE + SKILL_ATTR_MULT·gov + ceil(SKILL_LUCK_MULT·Luck)` = `2 + 2·gov + ceil(Luck/2)`,
  with the geckwiki worked example pinned by `base_skill_matches_documented_example`.
  *Source*: `charal-fnv-fo3-ruleset.md` "Skill base (auto-calc), BUILT". The
  **tag-skill per-level part is genuinely absent, not guessed** — no `+15` and no
  per-level term exists anywhere in `actor_value_derive.rs`; the module docstring
  states the deferral explicitly. Correct per the known-open register.
- **`setav` / `modav` write the base layer.** `edit_av` dispatches to
  `ActorValues::set_base` / `ActorValues::mod_permanent`. Derived stats are **not
  materialised** at spawn (CHARAL §6), and `GetActorValue` returns a carried value in
  preference to a formula (`if avs.get(param_1).is_some() { return avs.current(…) }`),
  so a `setav` on a derived AVIF *overrides* the formula rather than being recomputed
  away. No silent revert. **No finding.**
- **FO4 `PRPS` FormID space.** `parse_npc_actor_values` applies `remap_fid` to each
  `PRPS` AVIF FormID, so `derive_stored_actor_values`' verbatim passthrough is in
  global load-order space as documented. Baked `DNAM` Health/AP resolve-or-skip and
  treat `0` as absent.

**Could NOT verify / routed elsewhere**
- **FO3/FNV NPC_ stat *storage*.** `charal-fnv-fo3-ruleset.md` "NPC stat storage"
  states auto-calc-OFF NPCs store explicit skill/SPECIAL values in a DNAM-era layout.
  `parse_npc_actor_values` (the only DNAM consumer) is gated on
  `GameKind::uses_actor_value_properties`, i.e. FO4+, so those bytes are never
  captured for FO3/FNV. That is an **NPC_ parsing gap → `/audit-esm` Dimension 4**;
  its CHARAL-side consequence is `CHAR-D5-04`.
- **LVLN-targeted templates.** 587 FNV / 159 FO3 `Use Stats` NPCs point at an `LVLN`
  rather than a direct `NPC_`. Quantifying their divergence needs an LVLN tier walk;
  only the direct-`NPC_` divergence is measured below. The mechanism is ignored either
  way (`CHAR-D5-02`).
- **Noted, not filed**: `derive_autocalc_actor_values` performs 22 `actor_value_form_id`
  calls per NPC, each a linear `find` over `EsmIndex::actor_values`, instead of consuming
  the already-resolved rosters `CharacterRuleset` holds (`AttributeSet` / `SkillSet`).
  Spawn-time only and subsumed by `CHAR-D1-03`'s suggested fix; not worth a separate ID.
- **Not re-reported** (sibling agents, this run): `CHAR-D1-02` / `CHAR-D2-01`
  (`DerivedOutput::Multiplier` unread), `CHAR-D2-04` (`DerivedInput::actor_value`
  `Some(0)`), `CHAR-D1-01` / `CHAR-D2-05` (`pool_regen_tick_system` scope),
  `CHAR-D2-02` (fraction vs percentage units), `CHAR-D2-03` (FO3/FNV AP scope).
- **Not re-reported** (known-open register): FNV/FO3 tag-skill per-level formula;
  FO3↔FNV divergent *player* Health/AP; VATS runtime; `#2153` boot.rs access declaration.

---

## Ordering trace — the sequence invariant

There is no dependency graph; ordering *is* the resolution mechanism. This is the
actual write sequence, confirmed by reading each call site.

```
[once per session, before any REFR is spawned]
cell_loader/references/mod.rs :: load_references_budgeted   (job == None branch)
  └─ if world.try_resource::<CharacterRuleset>().is_none()
       └─ npc_spawn::build_character_ruleset(game, record_index)
            ├─ GameKind::Fallout4   → fallout4_ruleset(resolve)
            ├─ GameKind::Fallout3NV → falloutnv_ruleset(resolve)      ← see CHAR-D5-03
            └─ _                    → None   ⇒ NO resource inserted   ← verified clean
       └─ world.insert_resource(rs)

[per placed ACHR, inside NpcSpawnJob::advance → prepare_runtime_state /
 prepare_prebaked_state → spawn_placement_root]
npc_spawn/resumable.rs :: spawn_placement_root
  1. world.spawn()                                        → placement_root
  2. world.insert Transform
  3. world.insert GlobalTransform
  4. world.insert Name                                    (skipped if editor_id empty)
  5. stamp_faction_ranks       → FactionRanks             (skipped if no SNAM)
  6. stamp_actor_values        → derive_npc_actor_values(npc, index, game)
       ├─ FO4+  : PRPS (avif, value) pairs, then baked DNAM Health / ActionPoints
       └─ FNV/FO3: [a] 7 SPECIAL  ← class.base_attributes[i]   (raw [u8;7] from CLAS ATTR)
                   [b] 15 skills  ← base_skill(class.base_attributes[gov], luck)
                                     = 2 + 2·gov + ceil(Luck/2)
     → ActorValues::from_pairs(pairs)                     (component SKIPPED if pairs empty)
  7. stamp_character_components → CharacterLevel { level: npc.level.max(0) as u16 }  ← CHAR-D5-01
                                → Background { race_form_id, class_form_id }         ← CHAR-D5-02
                                → Perks                  (only if npc.perks non-empty)

[later, per NIF unit] … apply_ai_package_behavior · tag_descendants_as_actor

[read side — much later, at condition-eval / regen-tick time]
condition.rs :: GetActorValue
  → carried ActorValues value wins; else CharacterRuleset::derived_value(avif, avs, level)
    with level = CharacterLevel.level (0 if absent)
```

**Verdict: the invariant holds — but by construction, not by ordering.** Step 6 writes
base attributes *and* skills in a single `ActorValues::from_pairs`, and the FNV chain
`Unarmed Damage ← Unarmed skill ← SPECIAL` is resolved *inside*
`derive_autocalc_actor_values` off the raw `[u8; 7]` `class.base_attributes`, never by
re-reading the partially-populated `ActorValues`. So there is no intra-step-6 hazard to
get wrong. Derived stats are not materialised at spawn at all (CHARAL §6 — computed on
demand), so by the time any dependent is evaluated, steps 6 and 7 have both completed
and the ruleset resource has existed since before the REFR loop.

Two consequences a future reader should keep:
1. **Step 6 can skip the component entirely.** `stamp_actor_values` early-returns on an
   empty pair list, so an actor with no resolvable class gets **no `ActorValues` at
   all** — and `GetActorValue` then short-circuits to `0.0` at its
   `let Some(avs) = … else { return 0.0 }` guard, *before* the actor-general derived
   fallback. Such an actor reads 0 even for Carry Weight, which the ruleset could have
   computed. This is correct today only because the `pairs.is_empty()` case coincides
   with "we know nothing about this actor".
2. **Step 7 is after step 6, and must stay that way** if derived stats are ever
   materialised at spawn — `CharacterLevel` is an input to every `DerivedInput::LEVEL`
   row. Nothing in the type system enforces it.

---

## Findings

### CHAR-D5-01: `CharacterLevel` is populated from the raw ACBS level field, which holds a PC-level *multiplier* on 7 % of FNV and 12 % of FO3 actors
- **Severity**: HIGH
- **Dimension**: Population Boundary
- **Game**: fnv, fo3
- **Location**: `byroredux/src/npc_spawn.rs` (`stamp_character_components`) ·
  `crates/plugin/src/esm/records/actor/mod.rs` (`parse_npc_core`, the 24-byte
  `b"ACBS"` arm) · consumed by `crates/plugin/src/equip.rs`
  (`expand_leveled_form_id`) and `crates/scripting/src/condition.rs`
  (`GetXPForNextLevel`)
- **Status**: NEW
- **Source**: `docs/engine/charal-fnv-fo3-ruleset.md`, "XP / level curve — LOCKED":
  *"**Level cap:** FO3 **20** (30 with *Broken Steel*); FNV **30** (50 with the four
  add-ons, +5 each)."* A canonical `CharacterLevel.level` of 500–4000 is 17×–200× the
  documented cap, so the value cannot be a level under any reading of the ruleset.
- **Description**: `stamp_character_components` writes
  `CharacterLevel { level: npc.level.max(0) as u16, xp: 0 }` verbatim from the ACBS
  level field. On FO3/FNV that field is overloaded: when the ACBS "PC Level Mult" flag
  is set the field carries a **level multiplier**, not an absolute level. `acbs_flags`
  *is* parsed and stored on `NpcRecord`, but the only bit anything consults is bit 0
  (gender, via `Gender::from_acbs_flags`); nothing in the CHARAL population path — or
  anywhere else — checks the multiplier flag before treating the field as a level.
- **Evidence**: probe over vanilla `FalloutNV.esm` / `Fallout3.esm` via
  `byroredux_plugin::esm::parse_esm`, correlating `NpcRecord::level` against each
  `acbs_flags` bit:

  | | FNV | FO3 |
  |---|---|---|
  | NPC_ records | 3816 | 1647 |
  | `level > 100` | **268** (7.0 %) | **188** (11.4 %) |
  | `acbs_flags & 0x0080` set | 268 | 197 |
  | …of which `level > 100` | **268 / 268** | **188 / 197** |

  The partition on FNV is exact: bit `0x0080` and `level > 100` select the *same* 268
  records. No other bit correlates (bit `0x0010`, the auto-calc bit, covers 2283 FNV
  NPCs of which only the same 268 exceed 100). The out-of-range values are exclusively
  round steps — FNV `{500, 750, 800, 850, 900, 1000, 1100, 1200, 1250, 1300, 2000}`,
  FO3 adds `{600, 1500, 1750, 3000, 4000}` — i.e. a fixed-point multiplier, not a
  level. `1000` alone accounts for 184 FNV and 103 FO3 records.

  Two live consumers read the corrupted value:
  ```rust
  // crates/plugin/src/equip.rs :: expand_leveled_inner
  let eligible: Vec<&_> = lvli.entries.iter()
      .filter(|e| e.level as i32 <= actor_level as i32).collect();
  …
  let pick = eligible.iter().max_by_key(|e| e.level)   // single-pick: highest tier
  ```
  `build_npc_equip_state` seeds `actor_level = npc.level`, so an actor whose "level" is
  1000 makes **every** LVLI entry eligible and always draws the top tier. And
  `GetXPForNextLevel` evaluates `rs.leveling.xp_to_next(1000)` = `150·1000 + 50` =
  **150 050** instead of ~200.
- **Impact**: 268 FNV and ~190 FO3 base actors — the PC-level-scaled population, i.e.
  most generic raiders / troopers / Legionaries, the ones that appear in bulk — carry a
  canonical `CharacterLevel` two to three orders of magnitude wrong. Visible today as
  end-game leveled gear on low-level encounters; latent for every future CHARAL
  consumer, since `DerivedInput::LEVEL` rows, the leveling model, and the M45 save
  snapshot all read this field. The FO3/FNV `LEVEL`-bearing derived rows are
  `player_only`, so `GetActorValue` currently masks the derived-stat half — but
  `pool_regen_tick_system` evaluates `derived_value` *without* the scope gate
  (`CHAR-D1-01` / `CHAR-D2-05`), so that mask is one wiring change from lifting.
- **Related**: `CHAR-D1-01`, `CHAR-D2-05` (unscoped `derived_value`); `#1650`
  (CLOSED — the Oblivion ACBS parse gap, same field, different failure);
  the leveled-list half routes to `/audit-esm` Dim 4 / equipment.
- **Suggested Fix**: gate on the ACBS multiplier flag in `stamp_character_components`
  before writing `CharacterLevel` — when set, the field is a multiplier and the actor's
  level is a function of the player's, which is not modelled yet, so the honest write is
  the ACBS `calc_min` (already in the wire layout, currently skipped) or no
  `CharacterLevel` at all rather than the raw multiplier. **Do not divide by a guessed
  constant**: the `×1000` scale is inferred from the value distribution here, not
  sourced — pin it against xEdit `wbDefinitionsFNV.pas` first
  (`feedback_no_guessing`). The same gate belongs on `build_npc_equip_state`'s
  `actor_level`.

### CHAR-D5-02: CHARAL population ignores every NPC_ template-inheritance flag except "Use Inventory" — 55 % of FNV actors declare "Use Stats" and are still stat-derived from their own shell record
- **Severity**: MEDIUM
- **Dimension**: Population Boundary
- **Game**: fnv, fo3 (and FO4, which keeps the same TPLT model)
- **Location**: `byroredux/src/npc_spawn.rs` (`stamp_actor_values`,
  `stamp_character_components`) · `crates/plugin/src/esm/records/actor_value_derive.rs`
  (`derive_autocalc_actor_values`) · against `crates/plugin/src/equip.rs`
  (`resolve_inherited_inventory`, `TEMPLATE_FLAG_USE_INVENTORY`)
- **Status**: NEW
- **Source**: `docs/engine/charal-fo4-ruleset.md`, "Inheritance chain (where a given
  NPC's SPECIAL comes from)": *"**`TPLT` + ACBS Template Flags** — if "Use Stats" is
  set, inherit SPECIAL / level / etc. from the template `NPC_`/`LVLN` (FO4 keeps the
  FO3/FNV template model…)"* — item 2 of a four-step chain, ahead of the NPC's own
  overrides. Corroborated by `NpcRecord::template_flags`' own doc comment, which names
  `0x0001` Use Traits and `0x0002` Use Stats.
- **Description**: `NpcRecord::template_flags` parses all twelve bits and the field
  doc enumerates them, but `TEMPLATE_FLAG_USE_INVENTORY` (`0x0100`) is the **only** one
  any code consults — a repo-wide grep for `template_flags` outside tests returns
  `resolve_inherited_inventory` and nothing else. The CHARAL population path reads the
  NPC's *own* `class_form_id` (`derive_autocalc_actor_values` →
  `index.classes.get(&npc.class_form_id)`), its own `level` and its own
  `race_form_id`/`class_form_id` (`stamp_character_components` → `Background`),
  unconditionally. When `Use Stats` (`0x0002`) is set those fields are engine-ignored
  and the authoritative values live on the `TPLT` target.
- **Evidence**: probe over vanilla masters. `Use Traits`/`Use Stats` counts are from
  `template_flags`; divergence is measured against the record at `template_form_id`
  where that resolves to a direct `NPC_`.

  | | FNV | FO3 |
  |---|---|---|
  | NPC_ records | 3816 | 1647 |
  | `template_form_id != 0` | 2573 | 986 |
  | `Use Stats` (`0x0002`) set | **2097 (55.0 %)** | **879 (53.4 %)** |
  | …target is a direct `NPC_` | 1510 | 720 |
  | …target is an `LVLN` | 587 | 159 |
  | **own class ≠ template's class** | **117 / 1510** | **105 / 720** |
  | **own level ≠ template's level** | **86 / 1510** | **56 / 720** |
  | `Use Traits` (`0x0001`) set | 744 | 337 |
  | …own race ≠ template's race | 2 | 19 |

  A differing class is not cosmetic: `derive_autocalc_actor_values` takes
  `class.base_attributes` as the actor's whole SPECIAL and then derives all 15 skills
  from it via `base_skill`, so one wrong class FormID mis-states **22 actor values** on
  that actor. The 587 FNV / 159 FO3 `LVLN`-targeted cases are never resolved at all.
  Note the earlier disproof attempt: shell NPCs do **not** omit `CNAM` — every FNV and
  FO3 NPC_ carries a non-zero, resolvable `class_form_id` (measured: 0 with
  `class_form_id == 0`, 0 unresolvable). So the failure is not "no stats" but
  "stats derived from the record the engine ignores".
- **Impact**: at least 117 FNV and 105 FO3 base actors get a full SPECIAL + 15-skill
  set derived from the wrong class, plus 86/56 with a wrong `CharacterLevel` and
  `Background`. Silent — no log, no fallback, and `GetActorValue` returns a
  plausible-looking number. Every skill-check condition, package gate and future
  combat/dialogue consumer reads it. The engine already paid this exact bug once on the
  inventory axis (`#1658`, templated NPCs spawning naked); the stats axis has the same
  shape and no equivalent resolver.
- **Related**: `#1658` (CLOSED — the inventory half, which is where
  `resolve_inherited_inventory` came from and why the pattern is already proven);
  `CHAR-D5-01` (the other `CharacterLevel` defect, independent).
- **Suggested Fix**: generalise `resolve_inherited_inventory` into a
  `resolve_inherited_stats(npc, index) -> &NpcRecord` that walks `TPLT` (with the same
  `TPLT_MAX_DEPTH` cap and the same `LVLN` tier pick) when `0x0002` is set, and route
  `derive_autocalc_actor_values` + `stamp_character_components` through it; do the same
  for `0x0001` on `Background::race_form_id`. Promote
  `TEMPLATE_FLAG_USE_INVENTORY`'s neighbours to named constants beside it so the bit
  values stay single-sourced from xEdit.

### CHAR-D5-03: The `Fallout3NV` → FNV collapse also swaps the *leveling model*, which the documented justification does not cover — and the correct `fallout3_ruleset` is unreachable
- **Severity**: MEDIUM
- **Dimension**: Population Boundary
- **Game**: fo3
- **Location**: `byroredux/src/npc_spawn.rs` (`build_character_ruleset`) ·
  `crates/core/src/character/fallout.rs` (`fallout3_ruleset`) ·
  `crates/core/src/character/leveling.rs` (`LevelingModel::FO3`, `LevelingModel::FNV`)
- **Status**: NEW
- **Source**: `docs/engine/charal-fnv-fo3-ruleset.md`, "XP / level curve — LOCKED":
  *"**Level cap:** FO3 **20** (30 with *Broken Steel*); FNV **30** …"*, *"**Perk
  cadence:** FO3 = 1 perk **every level**; FNV = 1 perk **every other level**"*, and
  *"`LevelReward` for FO3/FNV = `SkillPoints { base: 10, int_mult: 1.0 (FO3) / 0.5
  (FNV), perk_cadence: 1 (FO3) / 2 (FNV) }`"*. The XP curve itself (`150·L + 50`) is
  shared, so only these three constants diverge.
- **Description**: `build_character_ruleset`'s docstring justifies collapsing both
  games onto `falloutnv_ruleset` on the grounds that *"the **actor-general** derived
  stats … are identical between them"*. That claim is **true** — I verified all six
  rows against the capture document (see Scope & Coverage). But the function returns
  the *whole* `CharacterRuleset`, and `CharacterRuleset` also carries
  `leveling: LevelingModel`, which is **not** identical:

  | | `LevelingModel::FO3` | `LevelingModel::FNV` |
  |---|---|---|
  | `level_cap` | 20 | 30 |
  | `int_mult` | 1.0 | 0.5 |
  | `perk_cadence` | 1 | 2 |

  So an FO3 load silently receives FNV's level cap, Skill Rate and perk cadence. The
  divergence is neither mentioned in the docstring nor covered by the known-open
  FO3↔FNV *player Health/AP* deferral, which is scoped to the derived table.
- **Evidence**: `build_character_ruleset` is a two-arm match with no FO3 arm:
  ```rust
  Some(match game {
      GameKind::Fallout4 => byroredux_core::character::fallout4_ruleset(resolve),
      GameKind::Fallout3NV => byroredux_core::character::falloutnv_ruleset(resolve),
      _ => return None,
  })
  ```
  `fallout3_ruleset` exists, is correct, is exercised by
  `fnv_and_fo3_share_skill_stats_but_differ_on_health_ap` and
  `skill_and_attribute_rosters_travel_with_the_ruleset`, and is re-exported from
  `crates/core/src/character/mod.rs` — but a repo-wide grep finds **no production call
  site**. It is dead outside `#[cfg(test)]`. The two green tests give the misleading
  impression the FO3 path is live.
- **Impact**: latent today — the only live `LevelingModel` consumer is
  `GetXPForNextLevel`, which calls `xp_to_next`, and that *is* identical across the two
  games (`150·L + 50`). `level_cap()`, `skill_points()` and `grants_perk_at()` have no
  production caller yet. So nothing is currently wrong on screen; the defect is that
  the first consumer to land will be silently wrong on FO3, and the docstring will
  still read as if the collapse had been justified for it. Blast radius = every FO3
  actor and the FO3 player once chargen/leveling exists.
- **Related**: `CHAR-D1-03` (the ruleset's missing `skill_calc` field — the same
  "ruleset carries more than the derived table" observation from the other side).
- **Suggested Fix**: either (a) narrow the docstring to state plainly that the
  leveling model is knowingly FNV's for both games and name the three divergent
  constants, or (b) do the master-name disambiguation the docstring already
  contemplates for Health/AP — `Fallout3.esm` vs `FalloutNV.esm` is available at load
  order — and route FO3 to `fallout3_ruleset`. (b) also retires the dead-code state.
  Whichever is chosen, add a test asserting which builder `GameKind::Fallout3NV`
  resolves to, so the choice is deliberate rather than incidental.

### CHAR-D5-04: The auto-calc deferral note frames non-auto-calc NPCs as an exception; on vanilla they are 40 % of FNV and 43 % of FO3 actors
- **Severity**: LOW
- **Dimension**: Population Boundary
- **Game**: fnv, fo3
- **Location**: `crates/plugin/src/esm/records/actor_value_derive.rs` (module docstring,
  "Deferred (intentionally, not guessed)" → the *Non-auto-calc NPCs* bullet) ·
  `derive_autocalc_actor_values`
- **Status**: NEW
- **Source**: `docs/engine/charal-fnv-fo3-ruleset.md`, "NPC stat storage — NOTE
  (distinct from FO4)": *"Auto-calc-OFF NPCs store explicit skill/SPECIAL values in
  their `NPC_` record (DNAM-era layout); auto-calc-ON NPCs are computed from class base
  attributes (the #1663 path)."* Flag identity from
  `docs/engine/charal-fo4-ruleset.md`, "Inheritance chain" item 4: *"**ACBS
  "Auto-calc stats"** flag (bit 4) — as in FO3/FNV."*
- **Description**: the deferral itself is correct and correctly declared — the code
  does not guess a formula, which is the right call. What is wrong is its stated scale.
  The docstring says *"Correct for the auto-calc **majority**; an approximation for
  hand-tuned actors"*, which reads as a long tail. Measured against the ACBS bit the
  FO4 capture document names, the auto-calc set is a bare majority and the
  "hand-tuned" set is ~40 % of every actor in both games. A reader sizing the gap from
  the comment will under-weight it by an order of magnitude.
- **Evidence**: probe over vanilla masters, counting `acbs_flags & 0x0010`:

  | | FNV | FO3 |
  |---|---|---|
  | NPC_ records | 3816 | 1647 |
  | auto-calc **ON** (`0x0010` set) | 2283 (59.8 %) | 935 (56.8 %) |
  | auto-calc **OFF** | **1533 (40.2 %)** | **712 (43.2 %)** |

  `derive_autocalc_actor_values` never reads `acbs_flags`; it goes straight to
  `index.classes.get(&npc.class_form_id)` for every FNV/FO3 actor. The stored values it
  should prefer are not merely unread — they are **unparsed**: the only `b"DNAM"` arm
  that captures actor values (`parse_npc_actor_values`) is gated on
  `GameKind::uses_actor_value_properties`, i.e. FO4+.
- **Impact**: documentation accuracy, not behaviour — the behaviour cannot improve until
  the parse lands. But the gap is load-bearing for milestone planning: ~1500 FNV and
  ~700 FO3 actors, including most hand-authored named NPCs (exactly the ones quests and
  dialogue conditions target), carry class-averaged stats instead of their authored
  ones.
- **Related**: the enabling parse gap (FO3/FNV NPC_ DNAM skill/SPECIAL block) routes to
  `/audit-esm` Dimension 4 — it is NPC_ record parsing, not CHARAL. `CHAR-D5-02`
  compounds it (a templated actor can be wrong on both axes at once).
- **Suggested Fix**: correct the docstring to state the measured split and name
  `acbs_flags` bit 4 as the discriminator, and add the `/audit-esm` cross-reference for
  the blocking parse work. Once the DNAM skill block is parsed,
  `derive_npc_actor_values` gains a third arm gated on that bit — the resolve-or-skip
  shape it already uses elsewhere.

---

## Summary

| ID | Severity | Title |
|---|---|---|
| CHAR-D5-01 | HIGH | `CharacterLevel` populated from a PC-level multiplier on 7 % FNV / 12 % FO3 actors |
| CHAR-D5-02 | MEDIUM | NPC_ template-inheritance flags ignored by CHARAL population (55 % of FNV actors set "Use Stats") |
| CHAR-D5-03 | MEDIUM | `Fallout3NV` → FNV collapse silently swaps the leveling model; `fallout3_ruleset` unreachable |
| CHAR-D5-04 | LOW | Auto-calc deferral note under-states its scale (40 % FNV / 43 % FO3, not a tail) |

Checklist items closing **clean**: `None`-handling across every caller (the dimension's
highest-impact item), resolve-or-skip in `push_derived`, FNV/FO3 actor-general
coefficient parity, the auto-calc base formula, the absence of a guessed tag-skill
per-level term, and `setav`/`modav` writing the base layer.

---

## Dimension 6



`/audit-character` · CHARAL · 2026-08-15 · depth `deep`

---

## Scope & Coverage

**Capture documents read (all seven, in full):**
`docs/engine/charal.md` (592 L), `docs/engine/charal-fnv-fo3-ruleset.md` (510 L),
`docs/engine/charal-fo4-ruleset.md` (551 L), `docs/engine/charal-fo76-ruleset.md` (126 L),
`docs/engine/charal-oblivion-ruleset.md` (783 L), `docs/engine/charal-skyrim-ruleset.md` (710 L),
`docs/engine/charal-starfield-ruleset.md` (174 L).

**Code read:** every file in `crates/core/src/character/` (`mod.rs`, `ruleset.rs`,
`derived.rs`, `leveling.rs`, `attribute.rs`, `skill.rs`, `fallout.rs`, `tes.rs`,
`skyrim.rs`, `regen.rs`, `affliction.rs`, `resistance.rs`, `reputation.rs`,
`components.rs`); plus `crates/core/src/combat.rs`, `crates/core/src/stealth.rs`,
`crates/core/src/lib.rs`, `byroredux/src/npc_spawn.rs` (`build_character_ruleset`),
`byroredux/src/boot.rs` (the `pool_regen_tick_system` registration),
`crates/plugin/src/esm/records/actor_value_derive.rs`,
`crates/plugin/src/esm/records/actor/mod.rs` (PRPS/DNAM arms),
`crates/plugin/src/esm/reader.rs` (`GameKind`).

**Docs cross-checked:** `docs/feature-matrix.md` (all sections + the
"What Doesn't Work Yet" gap table), `ROADMAP.md`, `README.md`,
`.claude/commands/_audit-common.md` (the CHARAL layout row),
`.claude/commands/audit-character/SKILL.md` (Scope).

**Test baseline:** `cargo test -p byroredux-core character` → **94 passed, 0 failed**.

**Verified CLEAN (checklist items with no finding):**
- **Vocabulary/doctrine.** CHARAL's verbs hold. `translate` / `canonical` /
  `resolve` / `derive` are the only layer verbs in use (`canonical` appears in 7 of
  14 character files; `resolve` is the uniform EditorID→FormID verb across
  `AttributeSet::resolve`, `SkillSet::resolve`, and every `*_ruleset(resolve)`
  builder). No sibling concept invents competing vocabulary — `combat.rs` and
  `stealth.rs` both explicitly cite the CHARAL boundary rather than redefining it.
  `derive` is a sanctioned addition, declared in `charal.md`'s opening paragraph.
- **Starfield scope loss.** `charal.md` §8 item 8 explicitly states
  `SkillSet::STARFIELD` / *LevelingModel::STARFIELD* / *starfield_ruleset* are not
  buildable, naming the two PENDING blockers. `AttributeSet::STARFIELD` (the one
  piece that *is* sourced — "no attributes") ships. Correctly noted, not silent.
- **Affliction pool tables.** `crates/core/src/character/affliction.rs`'s module
  docstring states plainly that no `AfflictionTable` ships and why (the pool→penalty
  numbers have no citable source). `charal.md` §4.6 says the same. Mechanism-built /
  data-pending is explicitly recorded, not silent.
- **Oblivion "BUILT" claims in the capture.** Spot-verified the four load-bearing
  ones: `modified_skill`, `oblivion_weapon_damage_multiplier`,
  `oblivion_hand_to_hand_damage` (all in `crates/core/src/combat.rs`) and the armor
  rating pair (`ARMOR_RATING_SKILL_COEFF` / `ARMOR_RATING_SKILL_BIAS`, two rows in
  `oblivion_ruleset`). All four exist. The capture is truthful about what shipped —
  the drift is in where those modules are *indexed* (CHAR-D6-05).
- **Disposition / Relationship Rank / crew skills.** Each is marked out-of-scope or
  "not built" in `charal.md` §7.1 with a reason. Explicit, not silent.

**Could not verify (and why):**
- **FO76 / Starfield constants.** No ruleset builder exists for either, so there is
  nothing to verify against. Recorded as a matrix gap, not a numeric finding.
- **Whether the FO4 70-perk chart is reachable.** `PerkRecord` parses and
  `EsmIndex::perks` exists, but tracing perk-gate evaluation is Dimension 5's
  population boundary, not this dimension's. `charal.md` §3 marks perk gates
  "pending", so it is explicitly noted either way.
- **Runtime behaviour of any wired ruleset.** No game data was loaded; every claim
  below is static (grep + read), per the no-parallel-engine-launch rule.

**Deduplication:** `/tmp/audit/character/issues.json` (2,832 issues, 263 OPEN)
searched for `charal`, `character`, `ruleset`, `feature-matrix`, `docstring`,
`coverage`, `fo76`, `scope`, `combat.rs`, `stealth`, `leveling`, `derived`.
Only `#2153` (OPEN, `pool_regen_tick_system` lock stack) touches CHARAL — excluded
by instruction. `#2417` (feature-matrix has no Quests section) and `#2047`
(feature-matrix lists NPC AI as unstarted) are CLOSED precedents for the *shape* of
CHAR-D6-04, not duplicates of it. All five findings below are **NEW**.

---

## The Coverage Matrix

Legend: **✓** built and reachable · **~** built but unreachable/no-op ·
**✗** absent · **—** not applicable.

| Game | Capture doc | Ruleset builder | Ruleset **wired** | Derived stats | Leveling model | Regen wired | Affliction wired |
|---|---|:---:|:---:|:---:|:---:|:---:|:---:|
| **Oblivion** | `charal-oblivion-ruleset.md` | ✓ `oblivion_ruleset` | ✗ (`build_character_ruleset` → `None`) | ✓ 8 rows / 5 stats | ✓ `LevelingModel::OBLIVION` | ~ config builder `oblivion_pool_regen_config` exists, **zero callers** | ✗ |
| **FO3** | `charal-fnv-fo3-ruleset.md` | ✓ `fallout3_ruleset` | ~ **unreachable** — `GameKind::Fallout3NV` → `falloutnv_ruleset` | ✓ 8 rows (unreachable) | ~ `LevelingModel::FO3` (unreachable) | ✗ | ✗ |
| **FNV** | `charal-fnv-fo3-ruleset.md` | ✓ `falloutnv_ruleset` | ✓ | ✓ 8 rows | ✓ `LevelingModel::FNV` | ✗ | ✗ |
| **Skyrim SE** | `charal-skyrim-ruleset.md` | ✓ `skyrim_ruleset` | ✗ (`None`) | ✓ 2 rows | ✓ `LevelingModel::SKYRIM` | ✗ | ✗ |
| **FO4** | `charal-fo4-ruleset.md` | ✓ `fallout4_ruleset` | ✓ | ✓ 4 rows | ✓ `LevelingModel::FO4` | ✗ | ✗ |
| **FO76** | `charal-fo76-ruleset.md` | ✗ | ✗ | ✗ | ✗ (`LevelReward::SpecialOrPerk` already claims FO76) | ✗ | ✗ |
| **Starfield** | `charal-starfield-ruleset.md` | ✗ (blocked, noted) | ✗ | ✗ | ✗ (blocked, noted) | ✗ | ✗ |
| *Morrowind* | — | — | — | — | — | — | — |

### Supporting substrate (game-independent, for completeness)

| Piece | Symbol | State |
|---|---|---|
| Attribute rosters | `AttributeSet::{FALLOUT, TES_CLASSIC, SKYRIM, STARFIELD}` | ✓ all four |
| Skill rosters | `SkillSet::{OBLIVION, SKYRIM, FALLOUT_FO3_FNV, NONE}` | ✓ four; no `STARFIELD` (blocked, noted) |
| Derived-stat engine | `DerivedStatFormula`, `eval` | ✓ |
| Regen tick | `pool_regen_tick_system` | registered in `boot.rs`, **permanently no-op** — `PoolRegenConfig` is never inserted outside tests |
| Affliction tick | `affliction_tick_system` | ✓ implemented, **never registered** in `boot.rs`; no `AfflictionTable` ships |
| Resistance half | `Affliction::ALL`, `damage_multiplier` | ✓ consumed by `add_fnv_fo3_shared` |
| Reputation family | `karma_band`, `ReputationStanding`, `affinity_band`, `FactionReputation` | ✓ classifiers only; `FactionReputation` is not stamped at spawn (no player entity) |
| CHARAL-adjacent math | `crates/core/src/combat.rs`, `crates/core/src/stealth.rs` | ✓ built, **outside the owner audit's declared scope** (CHAR-D6-05) |

### What the matrix says is missing

Three distinct gaps, in descending order of cost-to-close:

1. **The wiring gap is the dominant one.** Five of seven games have a complete,
   tested ruleset; only **two** (FO4, FNV) reach an actor. Oblivion and Skyrim are
   fully assembled — rosters, leveling model, derived tables, 94 green tests — and
   `build_character_ruleset` returns `None` for both. FO3's ruleset is built *and*
   shadowed by FNV's (`CHAR-D3-02`). So ~60 % of the shipped CHARAL surface is dead
   code at runtime, and the cheapest possible progress here is three match arms plus
   the FO3↔FNV master-name disambiguation that is already deferred.
2. **Two whole subsystems are built but have no live tick.** Regen is registered and
   permanently no-ops (no `PoolRegenConfig` insertion site anywhere — Oblivion has a
   builder with zero callers). Affliction is never registered at all and has no
   shipped table. Both gaps are *documented* — but only in `boot.rs` comments and the
   Oblivion capture, never in `charal.md` or the crate's own entry-point docstring.
3. **FO76 is the only game whose ruleset data is fully closed but unbuilt** — and it
   is the only capture document absent from `charal.md`'s rollout order entirely
   (CHAR-D6-03). Starfield, by contrast, is correctly recorded as blocked on real
   PENDING data.

The matrix also shows what is *not* missing and is easy to misread as missing:
attribute/skill rosters, the derived-stat engine, the resistance half, and all five
reputation-family classifiers are complete. The bottleneck is construction sites and
scheduler registration, not formulas.

---

## Findings

### CHAR-D6-01: `character/mod.rs`'s docstring omits 5 of 13 sub-modules — including every per-game ruleset builder — and names 2 of 6 capture documents

- **Severity**: MEDIUM
- **Dimension**: Coverage & Doctrine
- **Game**: all
- **Location**: `crates/core/src/character/mod.rs` (module docstring, above the `pub mod` block)
- **Status**: NEW
- **Source**: `docs/engine/charal.md` §5 (the six per-game captures the docstring
  should point at); `docs/engine/charal.md` §8 items 3/5/7 (the shipped family impls)
- **Description**: The CHARAL crate-slice docstring is the entry point every future
  contributor reads first, and the skill flags it as load-bearing because it *names
  files*. It enumerates eight sub-modules — `derived`, `leveling`, `ruleset`,
  `reputation`, `resistance`, `affliction`, `regen`, `components` — out of thirteen
  live ones. The five it never mentions are `attribute`, `skill`, `fallout`,
  `skyrim`, and `tes`: that is the entire attribute/skill roster half *and* all three
  per-game family implementations. A reader working only from this docstring would
  conclude the crate has no ruleset builders at all — `fallout4_ruleset`,
  `falloutnv_ruleset`, `fallout3_ruleset`, `oblivion_ruleset`, and `skyrim_ruleset`
  are invisible, despite being re-exported eleven lines below.
  Three further drifts in the same block:
  (a) it cites only `docs/engine/charal-fo4-ruleset.md` and `charal-fnv-fo3-ruleset.md`,
  though `tes.rs` and `skyrim.rs` ship against `charal-oblivion-ruleset.md` and
  `charal-skyrim-ruleset.md`, and two more captures exist;
  (b) the `leveling` bullet describes only the FO XP-curve and "the TES skill-use
  model (Oblivion: 10 major-skill-ups → level)" — `LevelingModel::SkillXp` /
  `LevelingModel::SKYRIM` is absent, the same class of drift `CHAR-D3-07` found one
  file over in `leveling.rs`, and independent of it;
  (c) the closing paragraph locates per-game work at "the parser boundary … in
  `byroredux_plugin`", which is true of *population* but silently omits
  `build_character_ruleset` (`byroredux/src/npc_spawn.rs`) — the documented **single
  construction site** for the resource this whole module exists to produce.
- **Evidence**: `pub mod` list in `crates/core/src/character/mod.rs` names thirteen
  modules: `affliction`, `attribute`, `components`, `derived`, `fallout`, `leveling`,
  `regen`, `reputation`, `resistance`, `ruleset`, `skill`, `skyrim`, `tes`. The
  docstring's `* [`…`]` bullets cover eight. `pub use fallout::{fallout3_ruleset,
  fallout4_ruleset, falloutnv_ruleset};`, `pub use skyrim::{skyrim_ruleset, …}` and
  `pub use tes::{…, oblivion_ruleset}` all appear below an index that never mentions
  their modules.
- **Impact**: Contributor misdirection at the exact place the skill identifies as
  highest-leverage. The omitted modules are where every per-game *number* lives, so a
  reader onboarding to CHARAL is pointed at the mechanism (`derived`, `leveling`) and
  away from the data (`fallout`, `tes`, `skyrim`) — the inverse of what this
  subsystem's risk profile calls for. No runtime effect.
- **Related**: `CHAR-D3-07` (same drift class, `leveling.rs`, distinct file and
  distinct sentence — both should be fixed together); `CHAR-D6-02`.
- **Suggested Fix**: Add bullets for `attribute`, `skill`, and a single "per-game
  family impls" bullet covering `fallout` / `tes` / `skyrim` with their builder names;
  extend the `leveling` bullet to the three-variant enum; list all six capture
  documents; and name `build_character_ruleset` as the single construction site.

---

### CHAR-D6-02: `charal.md` — the layer spec — is stale in four verifiable places, and omits the `regen` module entirely

- **Severity**: MEDIUM
- **Dimension**: Coverage & Doctrine
- **Game**: all (FO4 and Skyrim specifically)
- **Location**: `docs/engine/charal.md` §4, §5, §8 item 3, §9 item 1
- **Status**: NEW
- **Source**: `docs/engine/charal-fo4-ruleset.md`, section "NPC SPECIAL storage —
  RESOLVED (xEdit `Core/wbDefinitionsFO4.pas`, dev-4.1.6)" — the capture that closes
  `charal.md` §9's first open-research item
- **Description**: `charal.md` is the authority for the layer's shape and its
  remaining work. Four of its claims are contradicted by the current tree:
  1. **§5 states `skyrim_ruleset` ships "an **empty derived table**"** (with a
     parenthetical rationale that Health/Magicka/Stamina aren't attribute-derived).
     `skyrim_ruleset` pushes **two** formulas: an Armor Rating multiplier
     (`LIGHT_ARMOR_RATING_COEFF`, player-only) and Carry Weight
     (`CARRY_WEIGHT_BIAS` + `CARRY_WEIGHT_STAMINA_COEFF`, base-layer only). The
     rationale is still correct for the *pools*; the "empty" claim is not.
  2. **§8 item 3 states FO4 NPC *population* "is unstarted"**, naming "PRPS property
     pairs vs. `RACE`/template inheritance vs. both" as the open question. Steps 1–2
     of the capture's own implementation path shipped: `derive_stored_actor_values`
     reads `npc.actor_value_props` (the PRPS pairs) plus the baked `DNAM`
     `calculated_health` / `calculated_action_points`, gated on
     `GameKind::uses_actor_value_properties`, with wire-level tests in
     `crates/plugin/src/esm/records/actor/tests.rs`. Only step 3 (RACE/template
     inheritance fallback) remains open — a much narrower gap than "unstarted".
  3. **§9's first open-research item** ("FO4 NPC SPECIAL storage … Research was in
     flight when CHARAL was proposed; resume before implementing FO4") is closed by
     `charal-fo4-ruleset.md`'s own **RESOLVED** section, which gives the authoritative
     xEdit definition. The spec's open-questions list still carries a question its
     own child document answered.
  4. **The `regen` module appears nowhere in `charal.md`.** `grep -i regen` over the
     spec returns zero hits, while `crates/core/src/character/regen.rs` ships
     `PoolRegenAccumulator`, `PoolRegenConfig`, `pool_regen_tick_system`,
     `POOL_REGEN_DT`, `FATIGUE_REGEN_PER_SEC`, `MAGICKA_REGEN_BASE`,
     `MAGICKA_REGEN_WILLPOWER_COEFF`, and `magicka_regen_per_sec`, and the tick is
     registered in `byroredux/src/boot.rs`. A shipped module carrying sourced numeric
     constants *and* the layer's only fixed-timestep system is documented only in
     `charal-oblivion-ruleset.md` and a `boot.rs` comment — never in the spec that
     §4 presents as the canonical component inventory.
- **Evidence**: §5 "with an **empty derived table**" vs. the two `rs.push_derived(…)`
  calls in `skyrim_ruleset`. §8 item 3 "is unstarted" vs. `derive_stored_actor_values`
  in `crates/plugin/src/esm/records/actor_value_derive.rs`. §9 item 1's "Research was
  in flight" vs. the capture's "**Answer: the `PRPS` (Properties) subrecord**".
  `grep -c -i regen docs/engine/charal.md` → 0.
- **Impact**: The spec understates what shipped in two places and overstates the open
  work in two more. Its §8/§9 lists are what a milestone-planner reads to decide what
  to build next; both currently point at work that is done. The `regen` omission is
  the more structural one — the next contributor to touch pool regeneration has no
  entry point from the layer doc.
- **Related**: `CHAR-D6-01` (same omission of `regen`'s wiring status from the crate
  docstring); `CHAR-D6-03`; `CHAR-D3-06` (Skyrim/Oblivion constants sourced only to
  `charal.md` prose — this finding is why that circular sourcing is fragile).
- **Suggested Fix**: Correct the §5 Skyrim sentence to "two derived rows, no
  attribute-derived pools"; narrow §8 item 3 to the RACE/template fallback; strike §9
  item 1 with a pointer to the capture's RESOLVED section; add a §4.7 for `regen`
  recording the mechanism, its constants, and that it is registered-but-no-op.

---

### CHAR-D6-03: `charal.md` still reads `Status: PROPOSED (design)` and its rollout order omits FO76 — the only fully-LOCKED capture with no builder

- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: FO76 (status half: all)
- **Location**: `docs/engine/charal.md` (Status header, line ~19; §8 rollout list)
- **Status**: NEW
- **Source**: `docs/engine/charal-fo76-ruleset.md` — "## Leveling — LOCKED …
  `XP_to_next(L) = 160·L − 120`"; the derived table rows `Carry Weight … 150 + 5·STR
  **LOCKED**`, `Health … 250 + 5·END (no level term) **LOCKED**`, `Action Points …
  60 + 10·AGI **LOCKED** (matches FO4)`
- **Description**: Two related drifts in the same document.
  **(a) Status.** `charal.md` is the only one of the five abstraction-layer specs
  still marked `PROPOSED`: `nifal.md` reads `ACTIVE (opened 2026-05-28)`, `exal.md`
  `ACTIVE`, `physal.md` `ACTIVE (opened 2026-06-14)`, `watal.md`
  `ACTIVE (design 2026-06-19; implementation checkpoint 2026-08-10)`. CHARAL ships 13
  sub-modules, five ruleset builders, two wired games, a registered scheduler system
  and 94 green tests. A reader applying the sibling convention concludes CHARAL is
  unbuilt design.
  **(b) FO76's absence from §8.** The rollout order runs items 1–8 and never mentions
  FO76. Every ruleset row FO76 needs is **LOCKED** in its capture, and every shape it
  needs already exists in code: `AttributeSet::FALLOUT` (the capture states FO76 needs
  no changes to it), `SkillSet::NONE` (no skills), `LevelingModel::XpCurve` (fits
  `160·L − 120` exactly), and `LevelReward::SpecialOrPerk` — whose own docstring
  already claims "**FO4 / FO76**". Three of its four derived stats are clean affines
  the existing `DerivedStatFormula::affine` expresses directly. The one genuinely open
  modelling question is FO76's weapon-type-split Melee Damage (`STR/20` for 1H/2H vs
  `STR/10` unarmed), which a single-row table cannot hold both halves of.
  The capture does carry a self-note ("Not yet in the CHARAL §8 rollout order"), so
  this is not fully silent — but that note lives in the child document and explains
  *why FO76 can't just reuse FO4*, not *why it isn't built*. The document that owns
  the rollout is silent, so nothing on the planning path records that the cheapest
  remaining game is buildable today.
- **Evidence**: `grep "^\*\*Status\*\*" docs/engine/{nifal,exal,physal,watal,charal}.md`
  → four `ACTIVE`, one `PROPOSED (design, 2026-06-29)`. `grep -n "pub const \(FO3\|FNV\|FO4\|OBLIVION\|SKYRIM\|FO76\|STARFIELD\)" crates/core/src/character/leveling.rs`
  → five consts, no `FO76`. `LevelReward::SpecialOrPerk`'s docstring: "FO4 / FO76: one
  point per level…".
- **Impact**: Planning-surface drift. The status line understates a shipped layer to
  every reader who compares it against its four siblings; the §8 omission hides the
  one remaining game whose data is closed. Neither has runtime effect.
- **Related**: `CHAR-D6-02` (same document, content staleness); `CHAR-D6-04`.
- **Suggested Fix**: Move the status to `ACTIVE` with an implementation-checkpoint
  date, matching `watal.md`'s form. Add an FO76 item to §8 stating the four LOCKED
  formulas, that the leveling/reward shapes already exist, and that split Melee Damage
  is the single open modelling question.

---

### CHAR-D6-04: `docs/feature-matrix.md` has no character/progression section and no CHARAL gap row — the subsystem is invisible in the "what works per game" document

- **Severity**: MEDIUM
- **Dimension**: Coverage & Doctrine
- **Game**: all
- **Location**: `docs/feature-matrix.md` (no such section; the gap table at
  "## What Doesn't Work Yet (live gaps as of 2026-08-12)")
- **Status**: NEW
- **Source**: `docs/engine/charal.md` §8 (the rollout order the matrix would mirror);
  `docs/engine/charal-fnv-fo3-ruleset.md` / `charal-fo4-ruleset.md` (the two games
  whose rulesets are wired and therefore matrix-reportable today)
- **Description**: `docs/feature-matrix.md` is documented as the living per-game
  runtime-status document and as lagging the code — so a lag is reportable doc rot,
  which is exactly what this is. It carries sections for Cell Loading, Rendering, NPC
  Spawning, Animation, Audio, Physics, Scripting, Quests, UI and Starfield-specifics.
  It has **zero** rows for character stats, actor values, derived stats, leveling, or
  progression: `grep -ci charal docs/feature-matrix.md` → 0, and no row mentions
  `ActorValues`. Its NPC Spawning table (the closest home) covers spawn, skeleton,
  FaceGen, equipment, inventory, skinning and AI — but not whether the spawned actor
  has stats. Nor does the gap table carry a CHARAL row, so the two most consequential
  live gaps — Oblivion/Skyrim rulesets built but unwired, and both CHARAL tick systems
  inert — appear in no planning document at all. This is the same shape as the CLOSED
  precedents `#2417` (no Quests/M43 section despite two sessions of work) and `#2047`
  (NPC AI listed as unstarted despite seven shipped runtimes).
- **Evidence**: `grep -c -i "charal" ROADMAP.md HISTORY.md docs/feature-matrix.md` →
  `ROADMAP.md:0`, `HISTORY.md:9`, `docs/feature-matrix.md:0`. Section headers in
  `docs/feature-matrix.md` are Cell Loading / Rendering / NPC Spawning / Animation /
  Audio / Physics / Scripting / Quests / UI / Starfield-Specific / What Doesn't Work
  Yet — no character or progression heading. `README.md` mentions CHARAL twice; the
  matrix and `ROADMAP.md` never do.
- **Impact**: The document a reader consults to answer "does FNV have working
  character stats?" cannot answer it, in either direction. Worse for planning: the
  wiring gap the matrix above identifies as CHARAL's dominant cost is recorded nowhere
  a milestone planner looks. `ROADMAP.md` has no CHARAL row either, so `HISTORY.md`
  and the capture documents are the only trace.
- **Related**: `CHAR-D6-03` (the §8 rollout omission this would mirror); `#2417`,
  `#2047` (CLOSED precedents, same shape, different subsystem).
- **Suggested Fix**: Add a "Character / Progression (CHARAL)" section with one row per
  matrix column above (ruleset wired, derived stats, leveling, regen, affliction)
  across the seven game columns, and one gap-table row for "Oblivion/Skyrim rulesets
  built but unwired; regen + affliction ticks inert".

---

### CHAR-D6-05: Two CHARAL-sourced modules live outside `character/` and outside this audit's declared scope — their per-game constants have no owner

- **Severity**: MEDIUM
- **Dimension**: Coverage & Doctrine
- **Game**: Oblivion (`combat.rs`), FO3/FNV (`stealth.rs`)
- **Location**: `crates/core/src/combat.rs`, `crates/core/src/stealth.rs`;
  scope declarations in `.claude/commands/audit-character/SKILL.md` and the CHARAL row
  of `.claude/commands/_audit-common.md`
- **Status**: NEW
- **Source**: `docs/engine/charal-oblivion-ruleset.md`, "## The Complete Damage
  Formula — closes Marksman/Hand-to-Hand, adds Luck-chained skill + Armor Rating — all
  now BUILT" (`ModifiedSkill = Skill + 0.4×(Luck−50)`; Hand-to-Hand
  `1 + 10.5 × (Strength/100) × (ModifiedSkill/100)`) and "## Melee weapon damage
  (Blade/Blunt) — BUILT" (`× 0.5 × (0.75 + Strength × 0.005) × (0.2 + WeaponSkill ×
  0.015)`); `docs/engine/charal-fnv-fo3-ruleset.md`, "### Sneak Detection (FNV) —
  LOCKED"
- **Description**: Two top-level modules in `crates/core/src` hold constants sourced
  from CHARAL capture documents and derived from CHARAL actor values, but sit outside
  `crates/core/src/character/`. Both are honest about it — each module docstring
  explains the boundary and cites `charal.md` §7 — so this is not undisclosed code.
  The problem is **ownership**: this skill's Scope block and the CHARAL row of
  `_audit-common.md` both declare the crate slice as `crates/core/src/character/`
  only, so an audit run exactly as specified structurally cannot reach either file.
  Dimension 2 verified 26 constants; none of them are these. The affected numbers are
  precisely the kind this audit exists for — `modified_skill`'s `0.4` Luck
  coefficient, `oblivion_weapon_damage_multiplier`'s four coefficients, and
  `oblivion_hand_to_hand_damage`'s cross-term, all engine-hardcoded from UESP with no
  GMST read (the capture itself lists their GMST names: `fDamageStrengthBase=0.75`,
  `fDamageSkillMult=1.5`, `fHandDamageStrengthMult=0.75`, …). `stealth.rs` (487 lines,
  `detection_score` + `classify` + five input enums) is a full transcription of the
  FO3/FNV sneak-detection algorithm with the same status.
  Neither module is named in `crates/core/src/character/mod.rs`, in `charal.md` §7
  ("What stays out of scope" — which lists combat and dialogue as *concepts* but names
  no files), or in `_audit-common.md`'s layout. The result is a third un-owned
  subsystem of the kind `_audit-common.md` already warns about, but one that is not on
  its list.
- **Evidence**: `crates/core/src/lib.rs` declares `pub mod character;`,
  `pub mod combat;`, `pub mod stealth;` as siblings. `crates/core/src/combat.rs:1` —
  "Classic Oblivion combat-damage math (CHARAL-adjacent, not CHARAL itself)";
  `crates/core/src/stealth.rs` — "## Why this lives outside CHARAL". `modified_skill`,
  `oblivion_weapon_damage_multiplier`, `oblivion_hand_to_hand_damage`,
  `detection_score`, `classify` all exist; `grep` for any of them under
  `crates/core/src/character/` returns nothing.
- **Impact**: ~615 lines of per-game gameplay constants with a real capture-document
  provenance sit in an audit blind spot. A wrong coefficient there has the same
  silent-gameplay-drift profile as one inside `character/` — no crash, no failing test
  unless someone wrote it — with the added hazard that the module docstrings' own
  "CHARAL-adjacent" framing reads as *covered by the CHARAL audit* when it is the
  opposite. Both modules are consumer-less today, so the blast radius is deferred, not
  absent.
- **Related**: `CHAR-D6-01` (the `mod.rs` index that would be the natural pointer);
  `CHAR-D6-02` (§4/§7 of `charal.md`); `CHAR-D3-03` (the same hardcoded-vs-GMST
  problem, inside `character/`).
- **Suggested Fix**: Extend the CHARAL slice in `.claude/commands/audit-character/SKILL.md`
  and `.claude/commands/_audit-common.md` to name `crates/core/src/combat.rs` and
  `crates/core/src/stealth.rs` as in-scope "CHARAL-adjacent" files, add a
  Dimension-2 line item for their constants, and add a "see also" pointer from
  `character/mod.rs` and `charal.md` §7.

---

## Summary

**5 findings** — 4 MEDIUM, 1 LOW. All NEW; none duplicate an OPEN issue.
No CRITICAL or HIGH: nothing in this dimension can produce a wrong number at
runtime, only a wrong belief about what exists.

Coverage matrix published above with all seven game families.

---

