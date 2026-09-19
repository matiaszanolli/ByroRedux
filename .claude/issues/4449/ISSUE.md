# D1-04: D1-04: tes.rs armor-rating comment quotes "OpponentArmorSkill" — perspective-relative naming contradicts the capture's resolved reading the code implements

- **Labels**: low,character,documentation,doc-rot,game:oblivion
- **Filed from**: docs/audits/AUDIT_CHARACTER_2026-09-19.md (/audit-character 2026-09-19)
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4449

---

**Source**: docs/engine/charal-oblivion-ruleset.md §"The Complete Damage Formula" item 3: `PieceArmorRating = BaseArmorRating × (0.35 + 0.0065 × ArmorSkill) × …` with ArmorSkill resolved as the wearer's own governing armor skill.

**Description**

The doc comment for `ARMOR_RATING_SKILL_COEFF` (`crates/core/src/character/tes.rs:50-57`) first states the formula with `ArmorSkill`, then quotes the source as `0.35 + 0.0065 × OpponentArmorSkill`. The capture — the audit's ground truth — resolves the term to the *wearer's own* skill, and the shipped code reads exactly that (input = the actor's own LightArmor/HeavyArmor AV, actor-general scope, tes.rs:126-149; worked values 0.35+0.0065·50 = 0.675 and 0.35+0.0065·20 = 0.48 verified against the capture).

**Evidence**

Behavior matches the capture; only the inline formula quote carries UESP's attacker-perspective "Opponent" naming, which reads out of context as the *other* actor's skill — the one misreading a future consumer implementer could make.

**Impact**

None at runtime. Documentation-confusion risk next to a module whose whole point is that the input identity is unambiguous.

**Related**

#855d10cd doc-rot precedent.

**Suggested Fix**

Quote as `× (0.35 + 0.0065 × wearer's ArmorSkill)` with a parenthetical "(the UESP page writes this from the attacker's perspective as 'Opponent')".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_CHARACTER_2026-09-19.md` (finding D1-04, /audit-character 2026-09-19, HEAD `479163836`).*