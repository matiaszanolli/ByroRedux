# SCR-D4-2026-10-17-05: `check_running_quest_alias_scripts` bounds quests per frame, not scripts, so one quest can decompile an unbounded number of `.pex` in a frame

Issue: #6001
Labels: low, scripting, bug, performance

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D4-2026-10-17-05: `check_running_quest_alias_scripts` bounds quests per frame, not scripts, so one quest can decompile an unbounded number of `.pex` in a frame

- **Severity**: LOW
- **Dimension**: Attach & Reference Identity
- **Location**: `byroredux/src/asset_provider/script.rs:187-286` (`ALIAS_CHECKS_PER_FRAME` = 4 applies to quests; the inner loop at `:225` decompiles every alias script of each)
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: The comment justifies the cap as avoiding "a second of load", but the unit is the quest. A quest whose VMAD alias section lists many distinct scripts (a mod quest, or a vanilla quest with dozens) triggers one `extract_or_warn` + decompile + check per distinct name in a single frame. The verdict cache only helps repeated names.
- **Impact**: A frame hitch when such a quest starts. It is bounded by the VMAD size and has no persistent effect.
- **Suggested Fix**: Budget by script count (for example 8 distinct uncached scripts per frame) and leave the quest unchecked until the next frame when the budget is hit.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
