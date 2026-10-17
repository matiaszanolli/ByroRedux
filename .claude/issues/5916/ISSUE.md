# CHAR-2026-10-16-M-03: Skyrim's level roll is live (via `Game.AdvanceSkill`) but banks no perk and no +10 pool pick

Labels: medium,character,bug,game:skyrim
Filed from: docs/audits/AUDIT_CHARACTER_2026-10-16.md

- **Severity**: MEDIUM
- **Dimension**: Progression & Pools
- **Game**: skyrim
- **Location**: `byroredux/src/combat.rs:659-663` (`let Some((points, perk)) = leveling.skill_point_reward(..) else { return; }`), `:1175-1206` (`scripted_skill_system` → `grant_experience`); `crates/core/src/character/leveling.rs:150-170`
- **Status**: NEW (SKILL.md's "the TES models have no XP-driven roll" is stale for Skyrim)
- **Source**: `charal-skyrim-ruleset.md:715-722` ("Each level also grants a 10-point Health/Magicka/Stamina pool pick and a perk"), `:585-586` (`pool_pick_gain()=10`).
- **Description**: `scripted_skill_system` feeds `xp_from_skill_rank` into `grant_experience`. With `SKYRIM`, `xp_to_next = 25·L+75` and `level_cap() == 0`, so levels roll, the saved `CharacterLevel` advances and "Level up!" fires. `skill_point_reward` is `None` for `SkillXp`, so the function returns: no `LevelUpPoints.perk_points`, and nothing applies `pool_pick_gain`. `pool_pick_gain`, `grants_perk_at` and `SKYRIM_POOL_BASE` have no production consumer. The early `return` also discards levels already collected in `reached` for that call.
- **Evidence**: grep `pool_pick_gain|grants_perk_at|SKYRIM_POOL_BASE` over production code → only `leveling.rs`/`skyrim.rs`/tests.
- **Impact**: Each Skyrim level reached through a scripted skill advance permanently loses its perk and pool pick. Because the level is saved, a later implementation cannot recover them without a retro-grant. Latent while no UI spends them.
- **Related**: M-07 (the player has no `Perks` either).
- **Suggested Fix**: Bank a Skyrim reward (perk point into `LevelUpPoints` plus a pending pool-pick counter), or make `grant_experience` decline to roll for models whose rewards are unmodelled; at minimum log that the reward was dropped.

---
Filed from `docs/audits/AUDIT_CHARACTER_2026-10-16.md` (HEAD fa7591ca2).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' rulesets, sibling writers/consumers)
- [ ] **TESTS**: A regression test pins this specific fix
