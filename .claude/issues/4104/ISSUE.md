# CHAR-2026-09-11-D4-02: `pool_regen_tick_system` evaluates the max-pool formula at a hardcoded `level = 1`, discarding the `CharacterLevel` it has the entity id to read

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/4104
Filed: 2026-09-11 from `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`)

> Immutable snapshot of the issue as filed (TD10-001/#1156). GitHub is
> authoritative for current state: `gh issue view 4104 --json state`.

---

Reported by `/audit-character` — see `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`).

- **Severity**: LOW
- **Dimension**: Pools, Afflictions, Resistances & Reputation
- **Game**: all (latent; Oblivion is the only game that would reach it today)
- **Location**: `crates/core/src/character/regen.rs:205` (`for (_entity, avs)`), `:230` (`ruleset.derived_value(config.magicka_avif, avs, 1)`)
- **Source**: UNSOURCED-in-code. No capture document states that any max-pool formula is level-invariant in general; `charal-oblivion-ruleset.md` §Magicka gives `Magicka = 2×Intelligence` (level-independent), which is why the wrong argument is currently harmless — but the call site is game-agnostic and the `1` carries no comment saying so.

## Description

`CharacterRuleset::derived_value(output_avif, avs, level: u16)` (`ruleset.rs:110`) takes the level as a caller-supplied parameter, and `DerivedInput::LEVEL` is a real, shipped input — FNV/FO3 Health is `bilinear(END, LEVEL, …)` (`ruleset.rs:176`, `derived.rs:381-385`). The regen tick passes a literal `1`. Every **other** production `derived_value` call site resolves the real level: `byroredux/src/combat.rs:459-461` does `world.get::<CharacterLevel>(aggressor).map_or(1, |l| l.level)`, and `crates/scripting/src/condition.rs:524` passes a resolved `level`. The regen tick is the sole outlier, and it is not a capability gap — it holds the entity id and discards it as `_entity` at `:205`, while `CharacterLevel` lives in `crates/core` (`character/components.rs:15-27`) and is reachable from there.

## Evidence

`regen.rs:205,226-231`:
  ```rust
  for (_entity, avs) in avs_q.iter_mut() {
      ...
      let scoped_max = ruleset.as_ref().and_then(|ruleset| {
          ruleset
              .derived_formula(config.magicka_avif)
              .filter(|f| f.scope == DerivedScope::ActorGeneral)
              .and_then(|_| ruleset.derived_value(config.magicka_avif, avs, 1))
      });
  ```
  versus `byroredux/src/combat.rs:459-462`:
  ```rust
  let level = world
      .get::<CharacterLevel>(aggressor)
      .map_or(1, |level| level.level);
  ruleset.derived_value(melee_damage_avif, &avs, level)
  ```

## Impact

Zero today — no shipped max-Magicka row uses `DerivedInput::LEVEL` (Oblivion's is `2×INT`; Skyrim's pools advance by level-up pick, not a LEVEL formula, and the Skyrim ruleset is unreachable anyway per #3848), and the whole system is inert pending `PoolRegenConfig`. It becomes live the first time any game expresses a regenerating pool's maximum as a function of level: every actor's regen rate would then be computed off a level-1 pool, i.e. progressively too slow as the character levels, with no error and no test to catch it. The neighbouring `DerivedScope` filter shows the site is meant to honour the formula's full contract (#2932 went to real lengths for `scope`); the `level` argument next to it is a silent placeholder.

## Related

#2932 (the `DerivedScope` half of this same call), #3483 / #3444 (the delta commits that rewrote the surrounding lines without touching the `1`), `charal.md` §7.1 (the caller-supplied `level` parameter is load-bearing there too — scale-to-leader companions pass the *player's* level)

## Suggested Fix

Join `CharacterLevel` in the loop — `let level = level_q.get(entity).map_or(1, |l| l.level);` — matching `combat.rs`'s existing `map_or(1, …)` fallback, and add the read to the scheduler access declaration in `boot/schedule/update.rs`. If a hardcoded `1` is deliberate (e.g. "no regenerating pool is level-scaled in any supported game"), state that in a comment and cite it, rather than leaving a bare literal.

---

## Completeness Checks

- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix