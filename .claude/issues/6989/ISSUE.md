# #6989: TD8-2026-11-15-05: Two `_`-prefixed parameters survived refactors: `player_detection_events(_player)` and `Menu::texture(_zoom)`

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6989
- **Labels**: low,ui,gameplay,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

**Partly deferred**: the `player_detection_events(_player)` half sits in M65 detection code that another session is still editing (#6908 tracker); land it with or after M65 Phase 2. The `Menu::texture(_zoom)` half is independent.

- **Severity**: LOW
- **Dimension**: 8
- **Location**: `byroredux/src/detection/tick.rs:257` (called at `:119`); `crates/menuxml/src/menu.rs:463` (called at `:418` as `self.texture(assets, filename, *zoom)`)
- **Age**: `995ed089b` 2026-11-15 (M65 Phase 2); `67d9b3b27` 2026-09-18 (the menuxml crate's introduction)
- **Effort**: trivial
- **Labels**: tech-debt
- **Status**: NEW
- **Description**: both are private/crate-private inherent functions (not trait impls), so the underscore is not forced by a signature. `player_detection_events` takes `world` and `_player` but only uses `world` (it filters on `PLAYER_REFERENCE`). `Menu::texture` is a private method of `Menu` that never reads its `zoom`. Both callers pass an argument the callee ignores.
- **Impact**: noise; `_zoom` hints at a zoom-aware texture lookup that does not exist.
- **Suggested Fix**: delete the parameter and the argument at the call site (the skill says delete, do not rename). `detection/` is M65 work another session is still touching; check `git diff HEAD` on that file before editing.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
