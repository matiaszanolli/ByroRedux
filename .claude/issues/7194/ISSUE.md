# #7194 CHAR-2026-11-21-M-02: `fMaxArmorRating` is unbounded in the Oblivion armor overlay: a negative value panics, a value over 100 makes armor heal

**Labels**: medium,character,combat,bug,game:oblivion

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Reported as**: CHAR-D2-04
- **Severity**: MEDIUM / **Dimension**: Derived Formulas / **Game**: oblivion / **Location**: `crates/core/src/character/armor_rating.rs:48-50` (`with_gmst`), `:62-67` (`armor_factor`) / **Status**: NEW
- **Source**: the sibling overlay `armor.rs:60-73` (`value >= 0.0` filter, `.min(100.0)` "so a retune cannot make armor heal"); Rust `f32::clamp` panics when `min > max` (verified with a 5-line probe on the pinned toolchain: `7.0f32.clamp(0.0, -5.0)` panics).
- **Description**: `with_gmst` takes any finite `fMaxArmorRating`. `armor_factor` computes `combined.clamp(0.0, self.max_rating)`: a negative authored cap makes `min > max` and panics in the first weapon hit that reaches `weapon_defense`. A cap above 100 gives `(100 - combined) / 100 < 0`, a negative damage multiplier, so `apply_health_damage` receives negative damage.
- **Evidence**: `ArmorRatingModel::with_gmst(|n| (n == "fMaxArmorRating").then_some(-5.0)).armor_factor(10.0, false)` panics; with `150.0` and `combined = 120.0` it returns `-0.2`.
- **Impact**: a plugin (or a typo) in one game setting crashes the combat system or turns armor into a heal. Skyrim's equivalent was hardened in #6567.
- **Related**: #6567 (the Skyrim hardening this row missed).
- **Suggested Fix**: filter `value >= 0.0` and cap at 100 exactly as `ArmorModel::with_gmst` does, and floor the factor at 0.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other CHARAL rule rows / overlays / profiles)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
