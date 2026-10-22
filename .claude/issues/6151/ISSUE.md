# #6151: TD2-2026-10-19-06: `AddReputation`, `SetReputation` and `AddReputationExact` each carry the same 12-line "get-or-insert the player's `FactionReputation` standing, branch on the fame/infamy axis" block, one per sibling file
STATE: OPEN
LABELS: bug,low,tech-debt,scripting

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/scripting/src/obscript_effects.rs:972-985`, `obscript_more/actions.rs:157-170`, `obscript_more/tail.rs:374-387`
- **Age**: 2026-10-18 (all three)
- **Effort**: trivial
- **Description**: `world.query_mut::<FactionReputation>()` -> `if get_mut(player).is_none() { insert(Default) }` -> `get_mut(..).expect("present or inserted")` -> `if axis == 1 { add_fame/set_fame } else { add_infamy/set_infamy }`. Differences: add vs set, and `AddReputation` maps the 1-5 amount through `reputation_bump_points`.
- **Proposed fix**: `fn adjust_standing(world, player, repu, fame: bool, points: u16, set: bool) -> Option<()>` in `obscript_effects.rs` (the first home; `obscript_more` already imports `form_arg`/`num` from it); the three arms keep argument validation and the effect text.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._

