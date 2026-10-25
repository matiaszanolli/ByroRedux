# TD2-2026-10-25-05: four actor movers re-spell "apply the walk step to `Transform`", and two re-spell the "busy actor" gate; the #5580 guard does not see them

Labels: low, gameplay, ai, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6364
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Dimension**: 2
- **Location**: `byroredux/src/systems/flee.rs:138-148`, `systems/flee_package.rs:157-168`, `systems/find_package.rs:252-262`, `systems/eat_sleep.rs:371-381` (each: `query_mut::<Transform>()` → `translation = step.position` → `if let Some(r) = rotation { rotation = r }`); busy gate `has::<Dead>||AiCombatState||AiSuspended||FleeState` at `flee_package.rs:87-93` and `find_package.rs:188-196`. (`use_weapon_package.rs` and `forcegreet.rs` apply through their own `Step::Walk` enum.)
- **Age**: flee.rs 07b… (2026-10-18), flee_package.rs b540b104a and find_package.rs fcd1cce4a (2026-10-24). #5580 (badc45309, 2026-10-12) moved the six oscillating walkers onto `navmesh_path::write_back_walk` and pinned it with `walk_movers_share_one_pass_two_write_back` (`navmesh_path.rs:995`), whose file list is exactly those six, so the next four movers passed.
- **Effort**: small
- **Fix**: add `fn apply_walk_step(world: &World, moves: impl IntoIterator<Item = (EntityId, Vec3, Option<Quat>)>)` beside `write_back_walk` (`navmesh_path.rs:821`, which can call it for its Transform half); use it in the four. Add `fn ambient_package_may_run(world, actor) -> bool` (Dead/combat/suspended/flee) next to `FleeState` in `flee.rs`; both package systems call it. Extend the guard's file list with the four files.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD2-2026-10-25-05`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
