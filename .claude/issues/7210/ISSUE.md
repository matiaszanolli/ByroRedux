# #7210 CHAR-2026-11-21-L-13: The "a skill at 100 banks nothing" gate reads the base value where the recorded rule reads `GetAV`

**Labels**: low,character,bug,game:oblivion

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Reported as**: CHAR-D3-04
- **Severity**: LOW
- **Dimension**: Progression & Pools
- **Game**: oblivion
- **Location**: `crates/scripting/src/oblivion_skill_use.rs:172` (`let before = entry.base.max(0.0) as u16;`) feeding `SkillUseCosts::bank(value, ...)` (`skill_use.rs`, `value >= SKILL_USE_CAP`)
- **Status**: NEW (needs an owner decision; may be intended)
- **Source**: `skill-use.md:16` ("**a skill at 100 or more banks nothing** (`GetAV(skill) >= 100 -> return`); otherwise `exp += amount` ..., the rank-up writes the skill's **base** + 1"); PRD V2 / OSL-XP-003. The same row separates the two reads: the gate is on the current value, the write on the base.
- **Description**: `apply()` passes the base layer to `bank()`, so a player at base 95 wearing Fortify Blade 10 (current 105) still banks and ranks; the recorded rule would bank nothing. Whether the cost level `L` is also the current value is not recorded, so only the gate is flagged. The base-layer write itself (OSL-LVL-001) is correct and tested.
- **Evidence**: `values.get_base(skill.av)` -> `entry.base`; `ActorValue::current()` is not consulted anywhere in the file.
- **Impact**: Negligible on a vanilla load with no skill-fortifying gear; a divergence from the recorded rule that no test pins either way.
- **Related**: OSL-XP-003, V2.
- **Suggested Fix**: Either gate on `ActorValues::current` of the skill (keeping the base write) and pin it, or record in `skill-use.md:16` that the engine gates on the base layer by decision.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other CHARAL rule rows / overlays / profiles)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
