# #7206 CHAR-2026-11-21-L-09: PC-offset creature scaling is incomplete and the capture misstates what the engine does with attack damage

**Labels**: low,character,bug,game:oblivion

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Reported as**: CHAR-D4-04
- **Severity**: LOW
- **Dimension**: Population Boundary
- **Game**: Oblivion
- **Location**: `crates/plugin/src/esm/records/oblivion_population.rs:222-260` (`creature_values`: skills are the group value, no level term), `byroredux/src/npc_spawn/stamps.rs:165-190` (`stamp_creature_attack`: flat `oblivion_creature_stats.damage`), `docs/engine/charal-oblivion-ruleset.md:1366-1370` ("the engine reads no attack damage from this model")
- **Status**: NEW (doc half: stale since `278886579`, 2026-11-15)
- **Source**: attack: `/mnt/data/src/reference/cs-uesp-wiki/(main)/Stats Tab - Creatures.wiki` ("For PC level offset creatures the entered value is added to 1/2 the level of the creature"); the capture's own `0x51c980` note gives `stored + level x fCreatureCalcDamage` with the exe default 1.0 and the setting not authored by `Oblivion.esm` (byte scan: absent), so the 0.5 vs 1.0 disagreement is unresolved. Skills: `/mnt/data/src/reference/uesp-wiki/Oblivion/Leveled Lists.wiki:17-19` ("CombatSkill = Combat + 2*CreatureLevel", same for Magic and Stealth); the executable's skill path was not traced for a level term (the capture decodes only the two-view remap).
- **Description**: #7040 locked the level multiply for Health, Magicka and Fatigue only. The creature skills and the attack damage of a PC-offset creature are stamped unscaled, and neither omission is listed as an open item; the capture also still says the engine reads no attack damage while `stamp_creature_attack` has stamped `CreatureAttack` from it since 2026-11-15.
- **Impact**: PC-offset creatures (284 `CREA` in the vanilla master) hit with their authored base damage and carry base skills at every player level, where the CS/UESP texts say both rise with the creature's level; magnitude depends on which of the unresolved sources is right.
- **Related**: #7040, #7022; "no guessing": do not pick 0.5 vs 1.0 or confirm the skill term from a disassembly, observe it in the original game or declare the gap.
- **Suggested Fix**: add both terms to "Known gaps against the executable" with the two source lines, correct the attack-damage sentence, and decide the coefficients only after an observation (the #7022 smoke harness is the natural place).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other CHARAL rule rows / overlays / profiles)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
