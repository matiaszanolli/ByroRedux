# #7204 CHAR-2026-11-21-L-07: The Oblivion roster-order codes in the cast, bow and skill-use writers have no pin against `OBLIVION_ACTOR_VALUES`

**Labels**: low,character,test-gap,bug,game:oblivion

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Reported as**: CHAR-D4-07
- **Severity**: LOW
- **Dimension**: Population Boundary
- **Game**: Oblivion
- **Location**: `crates/scripting/src/player_cast.rs:28-31` (`AV_LUCK 7`, `AV_MAGICKA 9`, `AV_FATIGUE 10`, `AV_SCHOOL_FIRST 20`), `byroredux/src/skill_use_emit.rs:30-44` (`BLADE 14`, `BLUNT 16`, `HAND_TO_HAND 17`, `HEAVY_ARMOR 18`, `LIGHT_ARMOR 27`, `ATHLETICS 13`, `ACROBATICS 26`, `SNEAK 31`, `MARKSMAN 28`, `BLOCK 15`), `crates/scripting/src/oblivion_skill_use.rs:472` (`FED_SKILLS`), `crates/core/src/character/skill_use.rs:21-24` (`OBLIVION_SKILL_AV_FIRST/LAST`)
- **Status**: NEW
- **Source**: `docs/engine/charal-oblivion-ruleset.md` "The 72 records" (the one `const` roster in xEdit `wbActorValueEnum` order)
- **Description**: I checked every constant by hand against the roster (all correct today: Armorer 12 ... Speechcraft 32, Luck 7, Magicka 9, Fatigue 10, Alteration 20). They are a second copy of the roster order; the existing tests only compare table lengths (`skill_use.rs:424`) or use the same constants on both sides (`player_cast.rs:552`), so reordering or a roster correction would make the writers feed or spend the wrong value with every test still green.
- **Impact**: none today; the failure mode is silent mis-feeding.
- **Suggested Fix**: one test in the scripting or binary crate asserting `OBLIVION_ACTOR_VALUES[code]` equals the expected EditorID for each constant above and for `FED_SKILLS`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other CHARAL rule rows / overlays / profiles)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
