# NIFAL-D9-2026-09-21-01: assert_pbr_override_ceiling covers only the two Skyrim lanes — the other six games cannot see upward drift

**Labels**: low, nifal, test-gap, bug

**Severity**: LOW · **Dimension**: Completeness (translation-completeness signal) · **Tier Violated**: harness-gap · **Game Affected**: Oblivion, FO3, FNV, FO4, FO76, Starfield
**Location**: `crates/nif/tests/translation_completeness.rs:578` and `:617` (the only two `assert_pbr_override_ceiling` call sites)
**Source**: `docs/audits/AUDIT_NIFAL_2026-09-21.md` (CONFIRMED at `4dca737e2`)

### Description
#4393's lesson, recorded in `assert_pbr_override_ceiling`'s own doc (:236-240), is that a floor alone cannot see upward drift — a parser placeholder counted as a classifier signal. The ceiling was added only where #4393 happened to fire (SkyrimLE/SkyrimSE, both 97.0). Every other game asserts floors only, so the exact regression class the guard exists for would keep all six remaining games green. The low-fill BGSM-era rows are the most exposed: FO76's floor is 8.0% and Starfield's 1.0% (measured 5.1% on 2026-09-21) — a placeholder that drifted Starfield's metO to 50% would pass. FO4 measured 99.4% with no ceiling (legitimate per the union-of-signals classifier analysis, but nothing pins that).

### Evidence
Ceiling call sites at `:578`/`:617` only (third grep hit is the fn definition).

### Impact
A repeat of #4393 on any non-Skyrim game is invisible to the harness until re-measured by hand.

### Related
#4393, #4250, #2707

### Suggested Fix
Add a ceiling per game ~10pp above each current measured value (Oblivion/FO3/FNV ≈ 99-100 or a documented skip, FO4 ≈ 100, FO76 ≈ 20, Starfield ≈ 10), mirroring the floor margins' philosophy.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers, other gates)
- [ ] **TESTS**: A regression test pins this specific fix
