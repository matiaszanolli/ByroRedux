# CHAR-D1-01/D2-05: pool_regen_tick_system ignores DerivedScope and applies a PlayerOnly formula to every actor

- **Issue**: [#2932](https://github.com/matiaszanolli/ByroRedux/issues/2932)
- **Finding ID**: `CHAR-D1-01`
- **Merged duplicate**: `CHAR-D2-05` (same defect, reached independently by another dimension)
- **Labels**: `medium,legacy-compat,bug`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2932 --json state`.

---

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

---

### Merged duplicate: CHAR-D2-05

A sibling dimension reached the same defect independently, from the other entry point. Filed once; the second dimension's write-up follows because it adds evidence rather than repeating it.

**`pool_regen_tick_system` evaluates `derived_value` without the `DerivedScope` gate its sibling consumer applies**

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

## Completeness Checks
- [ ] **SIBLING**: The same pattern is checked in the other per-game ruleset builders (`fallout.rs` / `tes.rs` / `skyrim.rs`), not just the one cited
- [ ] **SOURCE**: Any changed constant cites the capture document line it comes from (`docs/engine/charal-*-ruleset.md`) — never a guessed value
- [ ] **CHARAL-BOUNDARY**: The per-game seam stays *data in the tables*; no consumer gains a branch on game identity
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
