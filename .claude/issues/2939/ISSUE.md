# CHAR-D2-06: eval clamps only the upper bound — negative-bias resistances return negative values for an absent input

- **Issue**: [#2939](https://github.com/matiaszanolli/ByroRedux/issues/2939)
- **Finding ID**: `CHAR-D2-06`
- **Labels**: `low,legacy-compat,bug`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2939 --json state`.

---

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

## Completeness Checks
- [ ] **SIBLING**: The same pattern is checked in the other per-game ruleset builders (`fallout.rs` / `tes.rs` / `skyrim.rs`), not just the one cited
- [ ] **SOURCE**: Any changed constant cites the capture document line it comes from (`docs/engine/charal-*-ruleset.md`) — never a guessed value
- [ ] **CHARAL-BOUNDARY**: The per-game seam stays *data in the tables*; no consumer gains a branch on game identity
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
