# TD1-2026-10-19-06: The remainder of closed #5803 is live: `apply_effect` (578, grew from 562), `evaluate_guard` (313), `check_call` (372), `guard_reading` (234), `call_statement` (227) (regression of #5803)

Labels: low,scripting,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Location**: `crates/scripting/src/fragment/effects.rs:152` `apply_effect` (578 lines, ~35 inline arms for the older families); `fragment/effects/evaluate.rs:228` `evaluate_guard` (313 / 30 arms); `fragment/vm/checker.rs:722` `check_call` (372); `translate/effects/guards.rs:357` `guard_reading` (234); `fragment/vm/interp.rs:1154` `call_statement` (227)
- **Status**: Existing: #5803 (closed 10-17 by `20e14944a`, whose subject covers only `apply_at_depth`, now 127 lines and fixed; the issue title also named `apply_effect`/`evaluate_guard`). Closed issue + live problem for those two: regression of its stated scope, reopen or file new.
- **Effort**: medium
- **Description**: the family-file precedent (`effects/{inventory,lock,scene,ai_combat,player_control,placement,vehicle_cinematic,quest_scoped}.rs`) covers the newest arms only. The inline arms group into clear families: quest/local/reference variables (`SetQuestVariable`, `CopyQuestVariable`, `ModQuestVariable`, `SetLocal`, `SetReferenceVariable`, L294-312, L620-667), actor values (`SetActorValue`/`Named`/`DamageActorValue`, L496-575), perk/faction/spell/race (L576-619, L679-720), location + alias (L444-495), player stats/economy (L315-335, L368-397), messages/UI (L252-283).
- **Proposed fix**: move each group to `effects/{quest_vars,actor_values,actor_lists,location_alias,player_stats,messages}.rs`, so `apply_effect` becomes a ~120-line dispatcher; mirror the same families for `evaluate_guard` (guard kinds by family) and `guard_reading`. Keep `fragment.rs::SOURCES` valid (add the files to its `concat!` and its `sources_completeness` test).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
