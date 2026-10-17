# CHAR-2026-10-16-L-07: The Skyrim capture states the barter sell cap as 1.00× item value; the code (and the exe) use 0.95×

Labels: low,character,documentation,doc-rot,game:skyrim
Filed from: docs/audits/AUDIT_CHARACTER_2026-10-16.md

- **Severity**: LOW (doc rot; the code value is exe-sourced)
- **Dimension**: Derived Formulas
- **Game**: skyrim (and the FO4 fallback pair)
- **Location**: `docs/engine/charal-skyrim-ruleset.md:107`; code `byroredux/src/barter.rs` `BarterRules::from_index`
- **Status**: NEW (introduced by `61ad95cf3`)
- **Source**: capture `:107` "sellPrice ≤ itemValue × 1.00, buyPrice ≥ itemValue × 1.05" (UESP *Speech* agrees). `SkyrimSE.exe` static table: `fBarterSellMax` 0.95, `fBarterBuyMin` 1.05; `Skyrim.esm` authors neither (only `fBarterMax` 3.3 / `fBarterMin` 2.0).
- **Impact**: None today; a future audit reads a false mismatch or "fixes" the code to 1.00.
- **Suggested Fix**: Amend `:107` to say the cap is `fBarterSellMax` (exe default 0.95), and add the exe defaults to the FO4 Barter Prices block (`charal-fo4-ruleset.md:229-240`).

---
Filed from `docs/audits/AUDIT_CHARACTER_2026-10-16.md` (HEAD fa7591ca2).
