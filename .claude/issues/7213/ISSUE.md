# #7213 CHAR-2026-11-21-L-16: the FO3 sneak row drops the armor-noise term with no document line

**Labels**: low,character,bug,game:fo3

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Reported as**: CHAR-D2-09
- **Severity**: LOW / **Dimension**: Derived Formulas / **Game**: fo3 / **Location**: `crates/core/src/stealth/fallout.rs:41-47` (`level_terms` gate covers both the level and the armor terms), test `the_fo3_row_drops_the_level_and_armor_terms` / **Status**: NEW
- **Source**: `docs/engine/sneak-detection.md` 4.2 ("New Vegas is the FO3 shape plus the level terms in the target skill") and `charal-fnv-fo3-ruleset.md:257-258,271-273` (the armor penalty belongs to `TargetSkill` on the FNV page; the page calls the level reading "FNV's addition over FO3" and says nothing about the armor term on FO3).
- **Description**: The FO3 row removes `5 (TL - DL)`, `max(50 - 10 TL, 0)` and the armor penalty together. The prose documents only the first two as FNV's addition; the armor half rests on the code and test alone.
- **Evidence**: the gate `if level_terms { ... - i.target_armor.penalty() }`.
- **Impact**: if FO3 does carry the armor penalty, a heavily armored FO3 player is detected more easily than the engine computes. Unknowable from the repository; a declared gap is cheap.
- **Suggested Fix**: state in `sneak-detection.md` 4.2 which source the FO3 armor omission comes from, or label it a declared choice in the same way the Silent Running visual term is (`charal-fnv-fo3-ruleset.md:326-336`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other CHARAL rule rows / overlays / profiles)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
