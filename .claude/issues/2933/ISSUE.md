# CHAR-D1-02/D2-01: DerivedOutput::Multiplier has no reader — FO4 Melee Damage surfaces a raw multiplier

- **Issue**: [#2933](https://github.com/matiaszanolli/ByroRedux/issues/2933)
- **Finding ID**: `CHAR-D1-02`
- **Merged duplicate**: `CHAR-D2-01` (same defect, reached independently by another dimension)
- **Labels**: `medium,legacy-compat,bug`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2933 --json state`.

---

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

---

### Merged duplicate: CHAR-D2-01

A sibling dimension reached the same defect independently, from the other entry point. Filed once; the second dimension's write-up follows because it adds evidence rather than repeating it.

**`DerivedOutput::Multiplier` is ignored by the live `GetActorValue` consumer — FO4 Melee Damage leaks a ×1.0–2.0 multiplier as if it were an actor value**

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

## Completeness Checks
- [ ] **SIBLING**: The same pattern is checked in the other per-game ruleset builders (`fallout.rs` / `tes.rs` / `skyrim.rs`), not just the one cited
- [ ] **SOURCE**: Any changed constant cites the capture document line it comes from (`docs/engine/charal-*-ruleset.md`) — never a guessed value
- [ ] **CHARAL-BOUNDARY**: The per-game seam stays *data in the tables*; no consumer gains a branch on game identity
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
