# #6156: TD2-2026-10-19-11: The Skyrim `.fuz` voice-path builder copies the truncation block of the FO3/FNV one, written one day after #5521 changed it
STATE: OPEN
LABELS: bug,low,tech-debt,dialogue

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `byroredux/src/systems/dialogue_voice.rs:97-122` (`voice_path_candidates`) vs `:136-165` (`fuz_voice_path_candidates`); third sibling `:172` (`oblivion_voice_path_candidates`)
- **Age**: 4d6294bb1 (#5521, 2026-10-11) rewrote the rule; acd577a79 (#5912, 2026-10-19) copied it
- **Effort**: trivial
- **Description**: lowercase both EDIDs, `if quest.chars().count() + topic.chars().count() > VOICE_NAME_CHARS { cut quest to 10, topic to the rest }`, then `format!("sound\\voice\\{plugin}\\{voice_type}\\{quest}_{topic}_{:08x}_{response}.<ext>")` — 16 identical lines. The two differ in the empty-guard (`||` vs `&&`) and the extension; the doc comment of the fuz copy re-measures the same 25-char boundary, i.e. the rule is shared knowledge held in two places.
- **Proposed fix**: `fn voice_stem(quest_edid, topic_edid) -> (String, String)` (lowercase + truncate) in `dialogue_voice.rs`; both builders keep their guard and extension.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._

