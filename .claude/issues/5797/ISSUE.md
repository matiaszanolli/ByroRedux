# TD2-2026-10-15-06: Ten Papyrus-call dispatch tails repeat `Effect::Interpret` -> `apply_fragment_guard_free(from_ref)` -> player lookup -> `push_quest_stage_advances`; the #5763 frame-budget gate reached only the ones touched that day

Labels: medium,scripting,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD2-2026-10-15-06`).

- **Severity**: MEDIUM
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/scripting/src/fragment/hits.rs:246-281`, `distance.rs:272-295`, `package.rs:256-274`, `perk.rs:68-86`, `terminal.rs:67-85`, `events.rs:257-264`, `updates.rs:415-451`, `reference.rs:472-504` (plus the console twins at `reference.rs:377,410`); budget helper `budget.rs:146,164`
- **Status**: NEW
- **Age**: M47.2 dispatchers 2026-10-12..13; the budget `195e3b4ef` (#5763, 2026-10-13)
- **Effort**: small
- **Description**: `package_fragment_system`, `perk_fragment_system`, `terminal_fragment_system` are the same 25-line scaffold (take the pending resource -> clone the fragment table -> look up the player -> loop building an `Effect::Interpret { script, function, args, resume: None }` -> `apply_fragment_guard_free(world, std::slice::from_ref(&effect), owner, vmad)` -> `let Some(player) = player else return` -> `push_quest_stage_advances`); `hits.rs`, `distance.rs` and `updates.rs` carry the same 12-line epilogue (`advances.is_empty()` -> `try_resource::<PapyrusPlayerEntity>()` -> push). Clone scan: 7 identical windows across `hits`/`distance`/`updates`.
- **Evidence**: the budget is a hand-copied `if super::budget::exhausted(world) { match super::budget::defer(...) { Ok(()) => continue, Err(mut e) => effects.remove(0) } }` at `updates.rs:427` and `reference.rs:480` (plus `events.rs:263` which always defers, and the two in `systems.rs`). `budget.rs:1-40` states "Every interpreter system ... is an exclusive Update system" and lists four; `hits.rs`, `distance.rs`, `package.rs`, `perk.rs` and `terminal.rs` still call `apply_fragment_guard_free` unconditionally, so an `OnHit` / `OnDistanceLessThan` / package / perk / terminal burst runs to completion in the frame it is raised, which is exactly the cost #5763 set out to remove.
- **Impact**: the per-frame interpreter bound is not a property of the system, it is a property of each dispatcher that remembered to copy the gate; the next dispatcher (the M47.2 list is still growing, 6 commits in 2 days) defaults to unbudgeted.
- **Related**: #5763, #5592 (the same consolidation, for the extension host side), `fragment/effects.rs:758` (budget gate inside a continuation).
- **Suggested Fix**: `fragment/dispatch.rs` with `fn run_papyrus_call(world, owner: QuestFormId, vmad: Option<&ScriptInstanceData>, script, function, args) -> Vec<QuestStageAdvanced>` (budget check + `defer` + `apply_fragment_guard_free`) and `fn flush_advances(world, advances)` (player lookup + `push_quest_stage_advances`); the ten sites become one call each and the budget policy has one home.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
