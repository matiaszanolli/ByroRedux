# CHAR-2026-10-26-L-02: `VatsModel::FALLOUT_NV` copies the FO3 values for six `fVATS*` settings that FalloutNV.esm authors differently; the doc says they are identical

- **Severity**: LOW
- **Dimension**: Derived Formulas
- **Game**: fnv
- **Location**: `crates/core/src/character/vats.rs` (`FALLOUT_NV`'s `..Self::FALLOUT3` spread; docs "identical in both games"/"FO3 and FNV share them"; test `vanilla_defaults_match_the_geck_wiki_page` pins `FALLOUT3` only)
- **Status**: NEW
- **Source**: FalloutNV.esm via `probe_gmst_vats`. Each pair is the value FalloutNV.esm authors (FO3 default in parentheses):
  - `fVATSPlayerDamageMult` 0.75 (0.1)
  - `fVATSCriticalChanceBonus` 5 (15; Fallout3.esm does not author it, the 15 is the GECK wiki default)
  - `fVATSDamageToWeaponMult` 1 (5)
  - `fVATSSpreadMult` 1.1 (0.5)
  - `fVATSRangeSpreadMax` 100 (3)
  - `fVATSRangeSpreadUncertainty` 0.01 (0.7)

  FalloutNV.esm also authors `fVATSFortuneOdds` 0.1, which the model lacks.
- **Description**: The FNV model is "FO3 plus the AP restore-rate dialect". A real FNV load overwrites all six through `with_gmst`, so the shipped runtime is correct. The compiled-in fallback and the "identical" claim are wrong, and the model's contract is "GMST, then the game's own vanilla default".
- **Impact**: None on a real FNV load. Wrong for any no-overlay consumer (tests, a debug command, a stripped load order). Phase 3c already routes `fVATSPlayerDamageMult` into the HitEvent funnel.
- **Suggested Fix**: Give `FALLOUT_NV` its own six values (plus `fortune_odds` if a field is added), fix the two "identical" sentences, and add a per-game pin next to `ap_standards_pin_the_per_game_corpus_values`.

---
Source: `docs/audits/AUDIT_CHARACTER_2026-10-26.md` (HEAD bf304851f), finding `CHAR-2026-10-26-L-02`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / profiles / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)

