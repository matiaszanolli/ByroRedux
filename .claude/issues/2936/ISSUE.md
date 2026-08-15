# CHAR-D2-02: the FO3/FNV derived table mixes fraction and percentage unit conventions with nothing recording which is which

- **Issue**: [#2936](https://github.com/matiaszanolli/ByroRedux/issues/2936)
- **Finding ID**: `CHAR-D2-02`
- **Labels**: `medium,legacy-compat,bug`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2936 --json state`.

---

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

## Completeness Checks
- [ ] **SIBLING**: The same pattern is checked in the other per-game ruleset builders (`fallout.rs` / `tes.rs` / `skyrim.rs`), not just the one cited
- [ ] **SOURCE**: Any changed constant cites the capture document line it comes from (`docs/engine/charal-*-ruleset.md`) — never a guessed value
- [ ] **CHARAL-BOUNDARY**: The per-game seam stays *data in the tables*; no consumer gains a branch on game identity
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
