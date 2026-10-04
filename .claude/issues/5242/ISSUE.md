# #5242: CHAR-2026-10-03-D5-01: `feature-matrix.md`'s CHARAL prose still says "FO4/FO76/Starfield share one 'stored' mechanism" and that FO76/Starfield "inherit the same decoder by lineage", contradicting the table row above it (✗, #4453)

- **Repo**: matiaszanolli/ByroRedux
- **Labels**: low,character,documentation,doc-rot,game:fo76,game:starfield
- **Source report**: docs/audits/AUDIT_CHARACTER_2026-10-03.md

- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: fo76, starfield
- **Location**: `docs/feature-matrix.md`, § Character / Progression (CHARAL), the paragraph beginning "Skyrim's NPC population derives Health, Magicka and Stamina…".
- **Status**: NEW. The paragraph was last edited by `8175cb706` (2026-08-31). It went stale with #4453 (2026-09-23), and the sibling #5051 fix (`797a5b227`) edited only the player-seed row.
- **Description**: Since #4453, `CharacterRulesProfile::FO76` / `STARFIELD` carry `NpcStatModel::None`, pinned by `fo76_and_starfield_claim_no_npc_stat_model_until_captured`, and the table's "NPC actor-value population" row says ✗ for both. The prose below still describes them as sharing FO4's PRPS + DNAM population. The parser's PRPS decoder still runs for them, but nothing populates actors from it.
- **Impact**: A reader takes FO76/Starfield NPC stats as wired.
- **Suggested Fix**: Re-scope the sentence to FO4. Say that FO76/Starfield parse PRPS/DNAM but populate nothing until a capture lands (#4453).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (`charal.md`, ROADMAP CHARAL bullets)

---
*Filed from `docs/audits/AUDIT_CHARACTER_2026-10-03.md` (finding CHAR-2026-10-03-D5-01, /audit-character 2026-10-03, HEAD `2c36c29d8`).*
