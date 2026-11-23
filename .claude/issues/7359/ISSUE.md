# #7359: FO3-D5-001: the new-game chargen fallback is keyed on `GameKind::Fallout3NV`, so FO3 falls back to New Vegas's VCG00

Labels: see GitHub (severity, domain, type and game labels)

Filed from `docs/audits/AUDIT_FO3_2026-11-23.md` (finding `FO3-D5-001`).

- **Severity**: LOW
- **Dimension**: FO3-vs-FNV discrimination; owner `/audit-scripting`
- **Location**: `byroredux/src/scene/scene_runtime.rs`, `chargen_quest_default` and its call site
- **Status**: NEW (came in with `8abbd3679`)
- **Description**: A new game reads `sCharGenQuest` from the install's `Fallout_default.ini`. When no INI can be read, it falls back to `chargen_quest_default(index.game)`, which returns `GameKind::Fallout3NV => Some(0x0010_2037)`, FNV's VCG00, for both games. `GameKind` cannot tell FO3 from FNV; `CharacterRulesProfile` is the only discriminator. The function's own comment admits it: "Fallout 3 and New Vegas share a game kind; Fallout 3's INI always sets it."
- **Evidence**: the call site does `ini.and_then(chargen_quest).unwrap_or(default_quest)` then `index.quests.get(&chargen)` and starts nothing on a miss. `Fallout3.esm` has 0 record headers with form 0x00102037. The shipped `Fallout_default.ini` sets `SCharGenQuest=0001f388` (CG00).
- **Impact**: bites only when the INI is not found next to the `--esm`'s `Data` folder (relocated or mod-manager root, name or case mismatch). An FO3 `--new-game` then silently starts no chargen quest: CG00 "Birth" never runs and the only trace is an absent log line. FNV and a vanilla FO3 install are unaffected.
- **Related**: `b972009d7`; there is still no FO3 new-game gate.
- **Suggested Fix**: take the compiled-in default from a profile row (FO3 0x0001F388, FNV 0x00102037, Oblivion 0x0002466E), or decline with a logged warning for FO3. Pin it with a test that an FO3-profile index with no INI picks CG00.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
