# TD2-2026-10-25-07: three `if try_resource::<T>().is_none() { insert_resource(T::new()/default()) }` stragglers survive #5806, and no guard stops a fourth

Labels: low, ecs, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6366
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Dimension**: 2
- **Location**: `byroredux/src/barter.rs:274` (`FormIdPool`, has `impl Default`, `form_id.rs:217`), `byroredux/src/cell_loader/reference_state.rs:487` (`ItemInstancePool`, `impl Default` `resources/mod.rs:1549`), `byroredux/src/cell_loader/object_sequences.rs:50` (`ObjectSequenceStore`, `#[derive(Default)]`, added f3380f6c84 2026-10-18); test copies `inventory.rs:3029`, `crates/scripting/src/fragment/tests.rs:3405`. (`terrain_height.rs:149` and `scene/world_setup.rs:59` insert non-Default values — legitimate.)
- **Effort**: trivial
- **Fix**: replace with `world.init_resource::<T>()` (`crates/core/src/ecs/world.rs:686`; 154 existing callers). Add a `workspace_hygiene_tests` regex scan for the three-line shape — #6136 pinned the contract but nothing pins the call sites.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD2-2026-10-25-07`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
