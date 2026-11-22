# #7199 CHAR-2026-11-21-L-02: the Oblivion weapon-damage family and its Luck term restate authored settings as literals while the spell-cast row reads the same settings

**Labels**: low,character,combat,tech-debt,bug,game:oblivion

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Reported as**: CHAR-D2-06
- **Severity**: LOW / **Dimension**: Derived Formulas / **Game**: oblivion / **Location**: `crates/core/src/combat.rs:41-84` (`modified_skill` `0.4 x (luck - 50)`, `oblivion_weapon_damage_multiplier` `0.5`, `0.75`, `0.005`, `0.2`, `0.015`, `oblivion_hand_to_hand_damage` `10.5`, `1.0`, `0.5`), `crates/core/src/character/bow.rs:218` (the `(F/M + 1)/2` constants) versus `spell_cast.rs` (`luck_skill_mult`, `luck_skill_base` read from `fActorLuckSkillMult` / `iActorLuckSkillBase`) / **Status**: NEW
- **Source**: `charal-oblivion-ruleset.md:554-564` ("GMST names now captured ... AUTHORED, not hardcoded ... All future formula rows built for Oblivion should read these by name once GMST parsing lands"); UESP *Complete Damage Formula* "Modifiable Settings" (`fDamageWeaponMult` 0.5, `fDamageStrengthBase` 0.75, `StrengthMult` 0.5, `SkillBase` 0.2, `SkillMult` 1.5, `fHandHealthMin/Max`, `fHandDamageStrengthMult`, `fHandDamageSkillMult`, `fHandFatigueDamageBase/Mult`); CS wiki *Fatigue Game Settings* (`FatigueFactor = fFatigueBase - fFatigueMult x (1 - Fatigue/MaxFatigue)`, 1.0 / 0.5). `Oblivion.esm` authors `fDamageWeaponMult`, `StrengthBase`, `StrengthMult`, `SkillMult`, the condition pair, `fHandHealthMax`, `fHandFatigueDamageBase/Mult`, `fFatigueBase`, `fFatigueMult` (probe), so a load order can retune every one of them.
- **Description**: GMST parsing landed long ago and four Oblivion rows (`spell_cast`, `bow`, `fall`, `armor_rating`) overlay it, but the weapon and Hand-to-Hand damage functions and the Luck term are pure literals. The same quantity (`-20 + 0.4 x Luck`) has two homes, one retunable and one not: retuning `fActorLuckSkillMult` changes spell costs but not weapon damage.
- **Evidence**: `rg "0\.4 \* \(luck" crates/core/src` finds `combat.rs:60`; `spell_cast.rs` carries `0.4` / `-20.0` as GMST-backed fields.
- **Impact**: a rebalance plugin that retunes the documented settings is honoured by magic and ignored by melee/bow damage (mod-compat doctrine, CLAUDE.md and *embedded_community_fixes*). Values are correct for vanilla.
- **Related**: CHAR-D2-02 (same duplication pattern), CHAR-D2-07.
- **Suggested Fix**: an `OblivionDamageModel` row (or fields on `BowModel`'s `DamageRule::Rating`) holding the nine settings with a `with_gmst`, installed like the other rows; route `modified_skill` through the spell-cast Luck fields.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other CHARAL rule rows / overlays / profiles)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
