# FNV-2026-10-18-09: `m-exteriors.sh fnv soak` hard-fails `groundcover_blades` monotonic growth

- **Issue**: #6058 (https://github.com/matiaszanolli/ByroRedux/issues/6058)
- **Labels**: low, terrain-exterior, test-gap, bug, game:fnv, legacy-compat
- **Source**: docs/audits/AUDIT_FNV_2026-10-18.md (snapshot as filed; GitHub is authoritative for state)

Found by the FNV compatibility audit of 2026-10-18 (`docs/audits/AUDIT_FNV_2026-10-18.md`, HEAD `af5063597`).

- **Origin**: D1-04. **Owner**: `/audit-exterior`. **Status**: NEW; also fails identically on the baseline build.
- **Evidence**: HEAD `[245811, 245851, 245862, 245873, 245904]` (+0.04% over 5 cycles); baseline `[74425, 74481, 74529, 74577, 74595]` (+0.23%). Blade count is 3.3× the baseline's, which matches the intentional GRAS density change in `592f31b87`. Static-mode frame p50 8.5 / 8.2 ms (HEAD) against 7.9 / 7.5 ms (baseline), two runs each on a shared machine: suggestive of +0.7 ms, not attributed.
- **Suggested Fix**: make the soak's monotonic test tolerance-aware, or fix the sampling phase; re-run to decide whether the 0.04% drift is real.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the other FO3 / Oblivion / Skyrim paths that share this code
- [ ] **TESTS**: A regression test (or a repaired gate) pins this specific fix

