# #7307: SCR-D4-03: The skill points at the wrong source pin, and the XCT stub spawn has no pin of its own

- **Severity**: LOW
- **Dimension**: Attach & Reference Identity (comment and guard rot)
- **Untrusted-Input**: No
- **Location**: `.claude/commands/audit-scripting/SKILL.md` Dim 4 ("the source pin in `cell_loader/references/source_pin_tests.rs` rejects the bare spelling"); `byroredux/src/cell_loader/relocated.rs:890`
- **Status**: NEW
- **Description**: `references/source_pin_tests.rs` holds three tests (`npc_spawn_jobs_are_resumable_and_wall_clock_timed`, `actor_spawn_branch_attaches_vmad_scripts`, `scol_expansion_is_cached_across_a_budget_yield`). None greps for the bare `spawn_logical_quest_reference(` spelling. The real pins are:
  - `scene/scene_runtime.rs:704-724`
  - `cell_loader/exterior/logical_stub_source_pin_tests.rs`
  - `references/synth_child_tests.rs:472` and `:522` (the helper behaviour test is `logical_quest_reference_spawns_identity_and_attaches_scripts` at `:538`)

  XCT's `reconcile_logical_identities` (`relocated.rs:890`) calls `spawn_logical_quest_reference_with_scripts` correctly today but has no pin. A future edit of that loop to the bare spawner would not fail any test.
- **Evidence**: `rg "spawn_logical_quest_reference_with_scripts" byroredux/src/cell_loader/relocated.rs` gives only the call at `:890`. There is no `production_text` scan of that file.
- **Impact**: This is the recurrence the #4112/#4331/#5650 pins exist to stop, and one of the four callers has none.
- **Related**: #4112, #4331, #5650.
- **Suggested Fix**: Add a `production_text` pin for `relocated.rs`. Correct the skill sentence to name the three real pins.

**Found by**: `docs/audits/AUDIT_SCRIPTING_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other latent / suspending paths, the other drain owners, the other ObScript hosts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the `apply_effect` lock inventory is updated
- [ ] **TESTS**: A regression test pins this specific fix
