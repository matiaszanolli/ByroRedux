# #7201 CHAR-2026-11-21-L-04: Oblivion's `LightArmorRating`/`HeavyArmorRating` derived rows can never resolve, and duplicate `ArmorRatingModel`'s coefficients

**Labels**: low,character,tech-debt,bug,game:oblivion

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Reported as**: CHAR-D1-01
- **Severity**: LOW
- **Dimension**: Ruleset Seam
- **Game**: Oblivion
- **Location**: `crates/core/src/character/tes.rs:59-76,139-162` (`ARMOR_RATING_SKILL_COEFF`, `ARMOR_RATING_SKILL_BIAS`, the two rows in `oblivion_ruleset`); `crates/core/src/character/armor_rating.rs:43` (`ArmorRatingModel::OBLIVION`); `crates/plugin/src/esm/records/oblivion_actor_values.rs:10-82` (the roster)
- **Status**: NEW
- **Source** (numeric): `docs/engine/charal-oblivion-ruleset.md:454-535` (UESP *Oblivion:The Complete Damage Formula*: `PieceArmorRating = BaseArmorRating x (0.35 + 0.0065 x ArmorSkill)`). Both copies agree with it; the finding is the second home and the unreachable rows, not a wrong value.
- **Description**: `oblivion_ruleset` pushes two `as_multiplier` rows whose *output* keys are resolved by editor id `"LightArmorRating"` / `"HeavyArmorRating"`. Since #7008 Oblivion's `actor_value_form_id` resolves only the 72-name engine roster (`index.rs:1176-1180`) and then the installed AVIF rows, which are those same 72 names. The roster contains no `*ArmorRating` name, so `resolve("LightArmorRating")` is `None`, the `if let` at `tes.rs:141`/`:152` never fires on a real load and both rows are dead. The live armor-rating path is `ArmorRatingModel` (`combat.rs:560`), which carries its own `0.35`/`0.0065`. That is the #6568 pattern (two homes for one coefficient pair) with no pin tying them together.
- **Evidence**: `OBLIVION_ACTOR_VALUES` names (index 0 to 71) end `... "Vampirism","Darkness","ResistWaterDamage"` with no armor-rating entry; the only tests that exercise the rows use a synthetic `resolve` that answers those names (`tes.rs:343-408`). The capture (`charal-oblivion-ruleset.md:521`) describes the rows as wired ("two rows in `oblivion_ruleset()`"). Nothing reads either output outside `tes.rs`.
- **Impact**: dead code and an inaccurate capture; the duplicated constants can drift (a future retune of one copy changes only one path). No wrong runtime value today.
- **Related**: #6568 (closed, the Skyrim twin); CHAR-D1-02 (why no test caught it).
- **Suggested Fix**: delete the two rows and the two consts from `tes.rs` (and the capture sentence), or keep one set of coefficients in a single home that both `ArmorRatingModel::OBLIVION` and the rows read, with an equality pin.

- **Merged**: Dim 2 reported the same defect as CHAR-D2-02 (both copies equal UESP's `0.35 + 0.0065·skill`; #6568's Skyrim shape is the precedent). Its `charal.md:350-351` prose half is in L-19.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other CHARAL rule rows / overlays / profiles)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
