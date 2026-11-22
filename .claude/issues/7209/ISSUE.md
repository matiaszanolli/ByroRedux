# #7209 CHAR-2026-11-21-L-12: Level-up flavour text is missing for 10 of 19 levels and level 21+ reuses level 20's: `Oblivion.esm` authors only nine `sLevelUp` strings, not "2 to 20"

**Labels**: low,character,bug,game:oblivion

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Reported as**: CHAR-D3-03
- **Severity**: LOW
- **Dimension**: Progression & Pools
- **Game**: oblivion
- **Location**: `byroredux/src/skill_use_emit.rs:266-277` (`level_message`: `game_setting_string(&format!("sLevelUp{}", level.clamp(2, 20)))`); `docs/engine/skill-use.md:189` ("`Oblivion.esm` authors 2 to 20")
- **Status**: NEW
- **Source**: UESP `Oblivion/Leveling.wiki:127-170` (local dump) lists a message for every level 2..20 and a distinct one for "21+" ("The results of hard work and dedication always look like luck..."); **Oblivion.esm GMST census**: the only `sLevelUp*` records are 4, 6, 11, 12, 15, 17, 18, 19, 20 (their text equals UESP's rows for those levels).
- **Description**: The code uses the load order's string and falls back to the bare "You are now level N." when none is authored. Levels 2, 3, 5, 7, 8, 9, 10, 13, 14 and 16 therefore get no flavour, and every level above 20 reuses level 20's text where UESP documents a different 21+ message. The doc's claim is the premise of the design and is false for the shipped master (the other strings are executable defaults, which the repo reads from no file). The data to close the gap is in a documented source, so no executable read is needed.
- **Evidence**: scan results above; `level.clamp(2, 20)` in `level_message`.
- **Impact**: Cosmetic (a notification line), but it is a documented player-facing message that a vanilla Oblivion player sees on 10 of the first 19 level-ups, and a wrong one beyond level 20.
- **Related**: PRD OSL-API-004, `skill-use.md:189`, CHAR-D3-05.
- **Suggested Fix**: Hold UESP's 2..20 and 21+ table as a documented fallback keyed by level (load order first, as now), use the 21+ text above level 20, and correct `skill-use.md:189` to say which nine the master authors.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other CHARAL rule rows / overlays / profiles)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
