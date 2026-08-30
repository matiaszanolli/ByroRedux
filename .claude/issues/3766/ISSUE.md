# #3766 — CHAR-2026-08-30-D2-01: DerivedStatFormula::cap's field docstring still names the pre-#2936 fractional Critical Chance cap, and a second cap that has never shipped

**Repo**: matiaszanolli/ByroRedux · **Filed**: 2026-08-30 · **HEAD**: `64f64480`
**Labels**: low, character, doc-rot, documentation, game:fnv, game:fo3

---

**Audit**: `/audit-character` — `docs/audits/AUDIT_CHARACTER_2026-08-30.md` (Dimension 2 — Derived-Stat Formulas), HEAD `64f64480`
**Finding ID**: `CHAR-2026-08-30-D2-01`

- **Severity**: LOW
- **Status**: NEW

## Location

`crates/core/src/character/derived.rs:167-168` — the `DerivedStatFormula::cap` field docstring

## Description

The `cap` field doc reads:

> *"Upper clamp (`f32::INFINITY` = uncapped). FO3 AP 85, FNV AP 95, **Critical Chance 0.10**, FO4 VATS 0.95."*

Two of those four are wrong against the shipped tables.

1. **Critical Chance** has shipped `capped(10.0)` since #2936 moved every percentage row onto the 0–100 convention. `0.10` is precisely the fractional value that fix removed.
2. **`FO4 VATS 0.95`** names a cap that has never existed in any builder: `fallout4_ruleset` registers exactly three rows (Health / AP / Carry Weight), pinned by `ROSTER_CASES`' `derived_rows: Some(3)`.

## Evidence

`derived.rs:168`:
```
/// Critical Chance 0.10, FO4 VATS 0.95.
```
versus `fallout.rs:63-66`:
```rust
DerivedStatFormula::affine(av(l), 1.0, 0.0).capped(10.0)
```
and the module docstring 100 lines above it: *"`Luck·1` capped `10`, **not** `Luck·0.01` capped `0.10`"*.

`grep -rn 'capped(' crates/core/src/character/` returns no `0.95` and no `0.10`. Re-verified at HEAD.

## Source

- `docs/engine/charal-fnv-fo3-ruleset.md:96` — `Luck × 1%` cap **10 %**
- `docs/engine/charal-fo4-ruleset.md` § derived table — V.A.T.S. accuracy is routed as a *gameplay-system input*, never a `derived` row

## Impact

The module docstring's percentage-convention paragraph explicitly says there is **no type-level enforcement** and that a new percentage row "must be written on the 0–100 scale by hand". The `cap` field doc is the nearest reference an implementer of such a row reads, and it demonstrates the wrong convention with a concrete number — the exact 100x-off failure mode #2936 was filed for.

`AUDIT_CHARACTER_2026-08-15.md` row 22 already quoted this line approvingly ("Crit 0.10") while marking it PASS, so the stale text has survived one audit that read it.

## Related

- #2936 (the fix that changed the value)
- #3485 (the sibling 32 B → 36 B pin rot in the same struct — OPEN)

## Suggested Fix

Replace the examples with the shipped caps: FO3 AP `85`, FNV AP `95`, Critical Chance `10`, Radiation Resistance `85`. Drop the FO4 VATS example entirely — it names no shipped row.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other field docstrings on `DerivedStatFormula`, and the module docstring's own examples)
- [ ] **TESTS**: A regression test pins this specific fix (or the values are asserted by an existing builder test the doc can cite)
