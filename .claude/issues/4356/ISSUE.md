# #4356 — TD3-008: NPC combat AI made two "the player is the only HitEvent producer" claims false

**Labels**: low, combat, gameplay, tech-debt, documentation, doc-rot
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4356

- **Severity**: LOW · **Dimension**: 3
- **Location**: `crates/core/src/ecs/components/creature_attack.rs:14-19`, `docs/feature-matrix.md:222,272-277` · **Status**: NEW (supersedes #4105's wording) · **Age**: `f61ea0447` (09-13) · **Effort**: trivial · **Kind**: doc-rot
- **Finding**: `byroredux/src/systems/combat_ai.rs:122,170` computes `attack_damage` for NPC aggressors and inserts `HitEvent` (verified).
- **Suggested Fix**: Name `npc_combat_ai_system` as a second producer and consumer of `CreatureAttack`; add an NPC-combat row to the feature matrix.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: If practical, a hygiene/gate check prevents this doc-rot class recurring
