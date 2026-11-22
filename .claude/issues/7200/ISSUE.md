# #7200 CHAR-2026-11-21-L-03: the #6756 GMST overlay helper accepts non-finite authored values; every sibling overlay rejects them

**Labels**: low,character,bug,game:fo3,game:fnv,game:fo4

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Reported as**: CHAR-D2-03
- **Severity**: LOW / **Dimension**: Derived Formulas / **Game**: fo3, fnv, fo4 / **Location**: `crates/core/src/character/fallout.rs:58-60` (`setting`), consumed at `:72-75,104-117,143-144,234-247,282-283,340-341`; caller `byroredux/src/npc_spawn/stamps.rs:262,282` (`index.game_setting_float`, which returns the raw `f32`) / **Status**: NEW (gap in closed #6756)
- **Source**: the family convention: `spell_cast.rs:157`, `bow.rs:167`, `fall.rs:49`, `armor_rating.rs:49`, `disposition.rs:57`, `limb_effects.rs:63` all use `.filter(|v| v.is_finite())`; `armor.rs:71` also drops negatives; `LevelingModel::with_gmst` (`leveling.rs:154`) filters too.
- **Description**: `setting(gmst, name, default)` is `gmst(name).unwrap_or(default)`. A load order authoring NaN or infinity for `fAVDHealthEnduranceMult`, `fAVDActionPointsMult`, `fAVDCarryWeightMult` and similar flows into the bilinear/affine rows.
- **Evidence**: `DerivedStatFormula::eval` ends `rounded.max(self.floor).min(self.cap)`; a NaN raw value becomes the floor (verified: `NaN.max(0.0).min(INF) = 0`), so a player's Health or AP reads 0, while `+inf` with an uncapped row (Health, Carry Weight) reads infinity.
- **Impact**: hostile or corrupt plugin data zeroes or unbounds a derived pool. Low likelihood, and the same data would be rejected by every other CHARAL overlay.
- **Related**: closed #6756 (M-01).
- **Suggested Fix**: `gmst(name).filter(|v| v.is_finite()).unwrap_or(default)` in `setting`, with a one-line test beside `retuned_favd_gmsts_reach_the_derived_rows`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other CHARAL rule rows / overlays / profiles)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
