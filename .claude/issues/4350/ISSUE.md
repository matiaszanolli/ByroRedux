# #4350 — TD2-008: scripting's `snapshot<T>` / `drain<T>` / `append_scene_completions` are verbatim copies in `dialogue.rs` and `package.rs` (a third `drain` in `scene/playback.rs`)

**Labels**: low, scripting, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4350

- **Severity**: LOW · **Dimension**: 2
- **Location**: `crates/scripting/src/dialogue.rs:170-189`, `:225-243`; `crates/scripting/src/package.rs:299-339`; `crates/scripting/src/scene/playback.rs:199-207` · **Status**: NEW · **Age**: `022cf421d` (08-01) · **Effort**: trivial · **Kind**: tech-debt
- **Finding**: The completion-batch merge rule decides when a scene action counts as done; a fix to one copy would leave the other runtime on the old rule.
- **Suggested Fix**: Make all three `pub(crate)` in `scene/playback.rs` beside `SceneActionCompletionBatch` and delete the copies.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved (and every lock `apply_effect` acquires stays named in its doc block, #3949)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
