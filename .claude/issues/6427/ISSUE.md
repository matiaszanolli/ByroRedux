# CHAR-2026-10-26-L-06: Trespass and pickpocket fines are fixed constants while the other crime settings are GMST-overridable

- **Severity**: LOW
- **Dimension**: Progression & Pools (mod-compat)
- **Game**: oblivion
- **Location**: `crates/core/src/character/crime.rs:134-135` (`TRESPASS_GOLD`, `PICKPOCKET_GOLD`), `with_gmst`; consumer `crates/scripting/src/crime_faction.rs:118-121`
- **Status**: NEW
- **Source**: CS wiki `ICrimeGoldXX` names `iCrimeGoldPickpocket` (25) and `iCrimeGoldTresspass` (5, sic) as Game Settings. Oblivion.esm authors neither (byte scan), so only a mod that adds them is affected.
- **Description**: The module promises "engine-supplied defaults that the load order's GMSTs override by name". Two of the five `CrimeKind` fines are the exception. A mod retuning them is silently ignored on the faction-fine path.
- **Impact**: Mod incompatibility only. Neither kind has a live reporting consumer yet.
- **Suggested Fix**: Promote both to `CrimeModel` fields read in `with_gmst` (sic spelling included), and have `faction_fine` read the model.

---
Source: `docs/audits/AUDIT_CHARACTER_2026-10-26.md` (HEAD bf304851f), finding `CHAR-2026-10-26-L-06`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / profiles / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)

