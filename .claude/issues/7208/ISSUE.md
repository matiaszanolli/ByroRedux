# #7208 CHAR-2026-11-21-L-11: `SpellCastModel` reads and applies two settings the CS wiki documents as unused, with no capture row

**Labels**: low,character,bug,game:oblivion

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Reported as**: CHAR-D2-05
- **Severity**: LOW / **Dimension**: Derived Formulas / **Game**: oblivion / **Location**: `crates/core/src/character/spell_cast.rs:109,143,167-168` (`level_cost_mult` 1.0, `level_cost_base` 0.0), `:257-266` (`spell_tier`) / **Status**: NEW
- **Source**: CS wiki `FMagicSpellLevelCostMult.wiki` and `FMagicSpellLevelCostBase.wiki`: "Not used by the game." (category Deprecated Settings); `Oblivion.esm` authors neither (probe); neither appears in `charal-oblivion-ruleset.md` "Spell casting" (the tier rule there is the bare base cost against `fMagicSpellLevel*Min`).
- **Description**: `spell_tier` evaluates `base_cost x level_cost_mult + level_cost_base` before the threshold test, and `with_gmst` lets a load order retune the two. The identity defaults hide it, but a plugin authoring either setting would change tiers in a way the documented rule does not, and the coefficients have no source line. By the no-guessing rule they are UNSOURCED coefficients.
- **Evidence**: the wiki text above; `rg SpellLevelCost docs` finds nothing outside `spell_cast_census.rs`.
- **Impact**: negligible today; dead flexibility that contradicts the source it cites.
- **Related**: #7077 (tracking), #7078 (the sourcing pass).
- **Suggested Fix**: delete both fields, or add a capture line that says the executable reads them (not by disassembly: owner rule) before keeping them.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other CHARAL rule rows / overlays / profiles)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
