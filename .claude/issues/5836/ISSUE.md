# TD3-2026-10-15-12: Remaining M42 procedure-count contradictions after #5471 (carried, narrowed; `feature-matrix.md` itself is now consistent)

Labels: low,gameplay,ai,tech-debt,documentation,doc-rot

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD3-2026-10-15-12`).

- **Severity**: LOW
- **Dimension**: 3 - Stale Documentation & Comments
- **Location**: `docs/engine/npc-spawn-ai-packages.md:9` (vs `:522`); `ROADMAP.md:61`, `:95`; `docs/engine/game-loop.md:155`
- **Status**: Regression of #5471 (CLOSED); carries TD3-2026-10-12-03
- **Age**: Eat/Sleep `00f580e09` (2026-10-08), Dialogue `14cff35ae` (2026-10-07); `graph_locomotion_system` / `console_walk_system` registered at `boot/schedule/post_update.rs:139-140`
- **Effort**: trivial
- **Description**: `npc-spawn-ai-packages.md:9` "exactly seven procedures execute" vs `:522` "Ten procedures of ~17 execute"; `ROADMAP.md:61` "Seven AI package procedures run by default" vs `feature-matrix.md:89` (10); `ROADMAP.md:95` "10 of ~17 package procedures need subsystems that do not exist (... dialogue)" (should be 7 missing, dialogue exists); `game-loop.md:155` PostUpdate row lists sandbox/wander/travel/follow/escort/guard/patrol only (no `make_eat_sleep_system`, `graph_locomotion_system`, `console_walk_system`).
- **Evidence**: `grep -n "exactly seven\|Seven AI package\|10 of ~17" docs/engine/npc-spawn-ai-packages.md ROADMAP.md`.
- **Impact**: `/audit-gameplay` treats `npc-spawn-ai-packages.md` as authoritative and is told `eat_sleep_system` should not exist.
- **Related**: #5471, TD3-2026-10-15-07.
- **Suggested Fix**: "exactly seven" -> "ten", ROADMAP:61 "Seven" -> "Ten", ROADMAP:95 -> "7 of ~17 (item use, package combat, magic)", add the three systems to the game-loop row.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
