# TD1-2026-10-25-05: `register_update_systems` grew 854 -> 981 lines (+127, 14 commits) and `register_late_systems` is 849; neither is in #6143's budget list

Labels: low, ecs, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6359
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW · **Dimension**: 1 · **Effort**: small
- **Age**: 14 commits on `byroredux/src/boot/schedule/update.rs` since 4a3e89d56 (AI package systems `fcd1cce4a` Find / `9aaf5e1cf` Ambush / `10f8cbb91` UseWeapon / `b540b104a` Flee, crime `4cfcee4b9` `99051a4eb` `98d6eedfe` `1a67c9f30` `0c6cd46dc` `39ffea9a8` `e63fa457d`, ObScript result scripts `6db47b0e4`). It will cross 1000 lines at the next two systems. Same `late.rs`: 836 -> 849.
- **Location**: `byroredux/src/boot/schedule/update.rs:17` (the file is essentially the one function, 997 prod lines), `late.rs:15`
- **Structure**: flat sequence of `scheduler.add_exclusive[_with_access](Stage::Update, ..)` with a 5-40 line ordering-contract comment each (producer-before-consumer: activation -> flush -> loot -> jail -> combat -> hostility -> guards -> quest/story/scene -> packages). Registration order is the contract, so the axis is **contiguous order segments**, not alphabetical: `register_interaction_and_crime`, `register_combat`, `register_quest_story_scene`, `register_ai_packages`, called in the same order from `register_update_systems`; the dt-dropping `fn x_dispatch(world,_dt)` shims (L81-145) move into the segment that registers them. No behaviour change, order pinned by calling in sequence.
- **Related**: #6143 (open: "no regrowth budget" for 11 driver fns). Add these two rows to its list rather than a new budget scheme; the same pin pattern (`draw.rs:1433` `DRAW_FRAME_LINE_BUDGET`) applies.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD1-2026-10-25-05`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
