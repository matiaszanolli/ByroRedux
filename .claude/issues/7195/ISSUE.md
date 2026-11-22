# #7195 CHAR-2026-11-21-M-03: Resist Normal Weapons ignores three documented rules: Weakness to Normal Weapons, creature attacks, and the fist exception; one is mislabelled as an open question

**Labels**: medium,character,combat,bug,game:oblivion

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Reported as**: CHAR-D2-01
- **Severity**: MEDIUM / **Dimension**: Derived Formulas / **Game**: oblivion / **Location**: `crates/core/src/character/armor_rating.rs:79-84` (`weapon_resistance_factor`), test `the_documented_resistance` (`-20.0 -> 1.0`), `byroredux/src/combat.rs:526-560` (`weapon_defense`), `:574-581` (`weapon_ignores_resistance`), `docs/engine/charal-oblivion-ruleset.md:533-535` / **Status**: NEW
- **Source**: UESP `Oblivion/Weakness to Normal Weapons.wiki` ("50% Weakness to Normal Weapons ... a weapon normally dealing 20 points ... your health would go down by 30"); UESP `Oblivion/Resist Normal Weapons.wiki` ("creature attacks, however, are unaffected by resist normal weapons"; "Normal weapons can be melee and ranged non-enchanted weapons or bare fists (only if below level 50 in Hand to Hand)"); UESP *The Complete Damage Formula* `OpponentWeaponResistance = (100 - NormalWeaponResistance) / 100` with no clamp. `crates/plugin/src/esm/records/oblivion_actor_values.rs:72-79` has `ResistNormalWeapons` and no separate weakness actor value.
- **Description**: (a) The roster has one actor value for the pair, so a Weakness to Normal Weapons effect (scrolls, Shivering Isles cursed gear) can only arrive as a negative `ResistNormalWeapons`; the documented factor is then above 1 (50% weakness -> 1.5). `weapon_resistance_factor` clamps to `[0, 1]` and a test pins `-20.0 -> 1.0` as "not modelled by this term". The ruleset doc's "Not modelled" list (`:532-535`) does not mention weakness. (b) UESP states creature attacks are unaffected by Resist Normal Weapons; the doc records it as an open question and says "(they do here)"; `weapon_defense` applies resistance to any aggressor with `ActorVitals`, creatures included. (c) The two UESP pages disagree on fists at Hand to Hand 50+ (Resist page: fists are normal only below 50; Weakness page: fists "do regular damage provided ... at least Journeyman"); the code treats every unarmed blow as normal (`weapon_ignores_resistance` returns `false` with no weapon). That conflict is also undeclared.
- **Evidence**: the quoted wiki lines above; `armor_rating.rs:83` `.clamp(0.0, 1.0)`; the doc sentence "Not modelled: armor condition (read as full), Shield-like boosting effects, and whether natural creature attacks count as normal weapons (they do here)".
- **Impact**: a player or NPC under a Weakness to Normal Weapons effect takes unmodified weapon damage instead of the documented increase; a ghost, wraith or vampire is also harder to hurt from creature attacks than the wiki says; the declared-gap list under-reports what is built.
- **Related**: #7056/#7054 siblings (same composition site); the ghost smoke `oblivion-ghost.sh` exercises only the 100% case.
- **Suggested Fix**: let the factor exceed 1 for a negative resistance (drop the lower bound of the clamp, keep `max(0)` on the result), add `aggressor is a creature` as a reason to ignore the term where the aggressor record is `CREA`, and either build or declare, in `charal-oblivion-ruleset.md:532-535`, the fist rule with its source conflict stated.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other CHARAL rule rows / overlays / profiles)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
