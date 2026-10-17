# CHAR-2026-10-16-L-08: The captures lag the code: Skyrim NPC derivation, level-up rewards and kill-XP/bleedout/encumbrance constants are in no `charal-*` document

Labels: low,character,documentation,doc-rot,game:skyrim,game:fo3,game:fnv
Filed from: docs/audits/AUDIT_CHARACTER_2026-10-16.md

- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: skyrim, fo3, fnv
- **Location**: `docs/engine/charal-skyrim-ruleset.md` (no section for `SKYRIM_BASE_SKILL` 15, `SKYRIM_SKILL_POINTS_PER_LEVEL` 8, the allocator); `docs/engine/charal-fnv-fo3-ruleset.md:459-461` (perk cadence still "mark for a citing pass") and no kill-XP section; `docs/engine/scripting.md:279-295`
- **Status**: NEW
- **Source**: SKILL: "a coefficient no capture supports is a finding". UESP *CLAS* supports the Skyrim numbers; the GECK *Experience Settings* page supports the kill-XP defaults 0/10/20/30/50, the 40 % health share and the `level <= highest` band rule (verified by Dim 3: FalloutNV.esm/Fallout3.esm author no `iXPRewardKillNPC*`); the exe tables support bleedout 10/30 s + 0.3 and `fMoveEncumEffect` 0.4. Every value is correct; the captures are missing.
- **Description**: Also `scripting.md:287` says the authored `iLevelUpSkillPointsBase` 11 / `Interval` 1 "give FO3's rule". Both masters author exactly those values, identical in FO3 and FNV, so they cannot be the FO3/FNV discriminator and nothing derives `10 + INT` from them.
- **Impact**: The next audit cannot verify those numbers document-first; M-01 and L-01 are exactly the drift a capture would have caught.
- **Suggested Fix**: Add a "Skyrim NPC derivation (CLAS)" section (skill and attribute formulas, allocator order as resolved by the master, the DNAM oracle and the saturation caveat) and a "Level-up rewards & kill experience" section to the FNV/FO3 capture; reword `scripting.md:287`.

---
Filed from `docs/audits/AUDIT_CHARACTER_2026-10-16.md` (HEAD fa7591ca2).
