# CHAR-2026-09-11-D5-05: three CHARAL rows in `NOT_SAVED_BY_DESIGN` claim a boot installation that no site performs, under a header asserting every entry was verified against real insertion sites

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/4107
Filed: 2026-09-11 from `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`)

> Immutable snapshot of the issue as filed (TD10-001/#1156). GitHub is
> authoritative for current state: `gh issue view 4107 --json state`.

---

Reported by `/audit-character` — see `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`).

- **Severity**: LOW
- **Dimension**: 5 — Population boundary
- **Game**: all
- **Location**: `byroredux/src/save_io/registry_completeness_tests.rs:180-205`
- **Source**: `docs/engine/charal.md` §4.7 — "`PoolRegenConfig` is inserted only by unit tests, never by a live per-game path (`oblivion_pool_regen_config` builds one, nothing calls it at load)".

## Description

the table's own header (`:183-185`) says "Verified 2026-08-05 against real (non-`#[cfg(test)]`) insertion sites for every entry". Three rows contradict that:
  - `("PoolRegenConfig", "immutable game-profile regeneration tuning installed at boot")` — **no production insertion site exists at all**. `grep -rn PoolRegenConfig byroredux/src crates/` returns only the type definition, two scheduler `reads_resource` declarations, two comments, `oblivion_pool_regen_config` (uncalled), and this row.
  - `("CharacterRuleset", "… selected at boot from the source game")` and `("MeleeDamageConfig", "… selected from the game profile at boot")` — both are actually built on the first cell-reference load (`byroredux/src/cell_loader/references/mod.rs:338-359`), not at boot, and not at all for Oblivion/Skyrim/FO76/Starfield.

## Evidence

the same table already has the correct idiom for a latent resource and uses it twice — `("AfflictionStatus", "forward-latent: affliction_tick_system has no production scheduler registration…")` and `("FactionReputation", "forward-latent: no production insertion or mutation site exists yet")`. `PoolRegenConfig` is in exactly that state and does not use it.

## Impact

doc rot in a guard whose stated purpose is to make "not saved" reasons auditable. A future reader checking whether regen state survives a reload is told a boot install happens; it does not. No runtime behaviour is wrong (the resource's absence already makes `pool_regen_tick_system` a no-op).

## Related

#3848 (the unwired Oblivion/Skyrim rulesets, OPEN) is the same "built, unwired" family; charal.md §4.6/§4.7 already record the mechanism-ahead-of-wiring state correctly.

## Suggested Fix

re-word `PoolRegenConfig` to the `forward-latent:` idiom the table already uses, and change the two "at boot" phrases to "at first cell load" so they match `references/mod.rs`.

---

## Completeness Checks

- [ ] **SIBLING**: every other copy of this fact swept repo-wide (this class has repeatedly had one copy fixed and another missed — grep the symbol/number across `docs/`, `crates/`, `.claude/`, not just the named file)
- [ ] **TESTS**: where the doc states a pinned number or symbol, a test or `_audit-validate.sh` rule keeps it honest