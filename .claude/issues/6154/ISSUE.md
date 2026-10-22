# #6154: TD2-2026-10-19-09: Three fragment dispatchers still hand-roll `flush_advances`, and `Effect::Interpret { .., resume: None }` is built by hand at 11 sites (prior-partial: TD2-2026-10-15-06 tail)
STATE: OPEN
LABELS: bug,low,tech-debt,scripting

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: epilogue copies `crates/scripting/src/fragment/systems.rs:161-166` (pending-fragment system), `:259-264` (scene dispatcher, with the #3580 lock-order comment), `crates/scripting/src/quest_stages.rs:1134-1139`; constructors at `fragment/{distance,events,hits,package,perk,populate,reference,terminal,updates}.rs`
- **Age**: epilogues 2026-09/10; the dispatch module landed 2026-10-15
- **Effort**: trivial
- **Description**: `advances.is_empty()` -> `PapyrusPlayerEntity::get` -> `push_quest_stage_advances` is `dispatch::flush_advances` verbatim (it is `pub(crate)` and re-exported at `fragment.rs:82`). The two in `systems.rs` additionally carry a copy of the #3580 comment that now lives once in `dispatch.rs:41-46`.
- **Proposed fix**: call `flush_advances` at the three sites; add `Effect::interpret(script, function, args)` beside `Effect::Interpret` in `translate/effects/model.rs` and use it at the 11 literals.

Related: #5797 (closed; fixed the dispatch tails, three epilogues remain).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._

