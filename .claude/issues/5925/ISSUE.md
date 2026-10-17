# CHAR-2026-10-16-L-04: The level-cap off-by-one duty has no consumer-level pin, and `skill_points` / `grants_perk_at` duplicate `skill_point_reward`

Labels: low,character,bug,test-gap,game:fo3,game:fnv
Filed from: docs/audits/AUDIT_CHARACTER_2026-10-16.md

- **Severity**: LOW
- **Dimension**: Progression & Pools
- **Game**: fo3, fnv, fo4
- **Location**: `byroredux/src/combat.rs:619-652` (the only bin tests are at level 1 and L1→L3); `crates/core/src/character/leveling.rs:150-215`
- **Status**: NEW
- **Description**: (a) The behaviour is correct by reading (`capped = cap > 0 && level >= cap` breaks before rolling) but nothing pins it; `level_caps_per_game` asserts only the constants. (b) `skill_point_reward` (the only production caller, from `grant_experience`) re-derives `base + int_mult·INT` and `level % perk_cadence`, duplicating `skill_points(u8)` and `grants_perk_at(u16)`, which now have no production callers and disagree for FO4 (`skill_points(FO4) = None`, `skill_point_reward(FO4) = Some((0.0, true))`). Per CLAUDE.md ("improve existing code rather than duplicating logic") one should win.
- **Suggested Fix**: Add a `grant_experience` test (FNV level 30 + 10 000 XP stays 30; level 29 + enough XP ends at exactly 30). Implement `skill_points`/`grants_perk_at` in terms of `skill_point_reward`, or delete them and port their tests.

---
Filed from `docs/audits/AUDIT_CHARACTER_2026-10-16.md` (HEAD fa7591ca2).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' rulesets, sibling writers/consumers)
- [ ] **TESTS**: A regression test pins this specific fix
