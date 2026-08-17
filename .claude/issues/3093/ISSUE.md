# CHAR-2026-08-16-D2-01: fallout4_ruleset Melee Damage row keys on an AVIF vanilla does not author

**Issue**: #3093
**Severity**: MEDIUM
**Labels**: `medium,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_CHARACTER_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_CHARACTER_2026-08-16.md` (Dimension 2 — per-game ruleset fidelity).

**Location**: `crates/core/src/character/fallout.rs`:98-104

## Description

`fallout4_ruleset` registers a Melee Damage row keyed on a `MeleeDamage` `AVIF` that **vanilla `Fallout4.esm` does not author**.

```rust
// crates/core/src/character/fallout.rs:98-104 (re-verified 2026-08-17)
// Melee Damage = ×(1 + 0.1·STR).
if let (Some(out), Some(s)) = (resolve("MeleeDamage"), strength) {
    rs.push_derived(out, DerivedStatFormula::affine(av(s), 0.1, 1.0).as_multiplier());
}
```

`resolve("MeleeDamage")` returns `None` on vanilla FO4, so the `if let` never fires and the row is never pushed.

## Impact

FO4's Melee Damage derivation is dead on real data. Combined with #3092 (nothing reads derived combat values anyway) and #2992 (FO4 weapon damage decodes to zero), the FO4 melee damage path has three independent reasons to produce nothing.

The `as_multiplier()` call also feeds `DerivedOutput::Multiplier`, which #3092 notes has no reader — so even a successful resolve would go nowhere.

## Suggested Fix

Determine what FO4 actually authors for melee damage (an `AVIF` under a different EditorID, or a non-`AVIF` mechanism) and key the row on that. **Measure against `Fallout4.esm` rather than assuming an EditorID** — this finding exists because the current name was assumed.

If FO4 genuinely has no such `AVIF`, remove the row and record why in `docs/engine/charal-fo4-ruleset.md`.

## Related

- #3092 (CHAR-D1-01 — nothing consumes derived combat values)
- #2986 (the `AV`-prefix resolution bug — a related "the EditorID we query does not exist" class)
- `docs/engine/charal-fo4-ruleset.md` (the authority for FO4 constants)

## Completeness Checks
- [ ] **NO-GUESSING**: The correct key is measured from `Fallout4.esm`, not inferred
- [ ] **SIBLING**: Every other `resolve("…")` in `fallout4_ruleset` checked against vanilla
- [ ] **SPEC-SYNC**: `charal-fo4-ruleset.md` matches whatever is decided
- [ ] **TESTS**: A real-data test asserts the FO4 ruleset's derived rows are non-empty

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3093 --json state` when live state is needed.*
