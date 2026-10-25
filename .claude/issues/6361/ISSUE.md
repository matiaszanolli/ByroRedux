# TD2-2026-10-25-02: the ambient behavior-component set is spelled three times; `insert_at_spawn` and `insert_at_runtime` are ~165-line line-for-line twins

Labels: low, gameplay, ai, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6361
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Dimension**: 2
- **Location**: `byroredux/src/npc_spawn/ai_package.rs:556-717` (`insert_at_spawn(&mut World)`), `:720-855` (`insert_at_runtime(&World)`), `:885-980` (`clear_ambient_behavior`, 30 `remove_component` lines)
- **Age**: this window added Find, UseWeapon, Flee, Ambush (b540b104a, 10f8cbb91, 9aaf5e1cf, fcd1cce4a, 7fded9423) — each edited both inserters and the remove list. The remove list has already needed a hand fix for a forgotten component (#3255 NavPath comment at `:968`).
- **Effort**: small
- **Fix**: write the variant match once. Option A: a private `trait ComponentSink { fn put<T: Component>(&mut self, actor: EntityId, c: T); }` implemented for `&mut World` (`world.insert`) and `&World` (`insert_component`), and one `fn install<S: ComponentSink>(self, sink: &mut S, actor)`; both current methods become 3-line wrappers. Option B: have `AmbientBehavior` produce a small `BehaviorBundle` enum and let the two callers insert it. Then pair the remove list with a `const AMBIENT_COMPONENT_REMOVERS: &[fn(&World, EntityId)]` so a new procedure edits one match arm and one row.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD2-2026-10-25-02`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
