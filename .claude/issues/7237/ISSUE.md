# #7237: GAME-2026-11-22-14: Two comments still say the Oblivion level-up surface is the console until a native page exists

**Labels**: low,gameplay,doc-rot,documentation,game:oblivion

From `docs/audits/AUDIT_GAMEPLAY_2026-11-22.md` (report ID **GP-14**), audited at HEAD 2b40f76b6.

- **Severity**: LOW
- **Dimension**: 6 (player feedback)
- **Domain**: gameplay · doc-rot · **Game**: Oblivion
- **Trigger**: Reading the code.
- **Location**: `byroredux/src/commands/gameplay.rs:665` (`LevelUpCommand::execute`, the `attr` arm) and `:615` (the `LevelUpCommand` doc)
- **Status**: NEW
- **Description**: The `attr` arm says "OSL-API-005 (Q3): the level-up surface until a native page exists". The page now exists (`oblivion_rest::snapshot` / `menuxml_rest.rs`, #7086), and the console picks remain only for scripted and jail sleep (PRD `game-ui-integration` rev 16, `docs/engine/skill-use.md:181-185`). The struct doc ("the level-up menu's work, until a menu exists") is right for the Fallout reward spend but reads as covering Oblivion too.
- **Evidence**: As above.
- **Impact**: It suggests the page is still outstanding, or invites deleting the still-needed console path.
- **Related**: #7086, #7052, #7217–#7220
- **Suggested Fix**: Reword `:665` to say the console picks serve scripted and jail sleep while a bed sleep offers the page, and qualify the `:615` doc.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other procedures / other transients / other inventory paths)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
