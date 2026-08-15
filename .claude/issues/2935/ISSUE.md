# CHAR-D1-04: derived_len is documented as a stat count but returns a row count

- **Issue**: [#2935](https://github.com/matiaszanolli/ByroRedux/issues/2935)
- **Finding ID**: `CHAR-D1-04`
- **Labels**: `low,legacy-compat,documentation`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2935 --json state`.

---

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

## Completeness Checks
- [ ] **SIBLING**: The same drift class is swept across the other capture documents / docstrings, not just the row cited
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
