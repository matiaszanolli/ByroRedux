# #3949 — SCR-D6-2026-09-06-02: `apply_effect`'s doc comment — the inventory the Dim-6 checklist delegates to — describes a lock-nesting shape that no longer exists

- **Finding ID**: SCR-D6-2026-09-06-02
- **Labels**: low,scripting,documentation,doc-rot
- **Filed**: 2026-09-06 by /audit-publish from `docs/audits/AUDIT_SCRIPTING_2026-09-06.md`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3949

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-06.md` — `/audit-scripting` pass 2026-09-06 (seventeenth). Verified against `main` at HEAD on 2026-09-06.

- **Severity**: LOW
- **Dimension**: Scripting Runtime Systems · **Untrusted-Input**: No · **Location**: `crates/scripting/src/fragment.rs:766-796` · **Status**: NEW (#3493 CLOSED re-attached this doc; drift is post-fix)
- **Description**: says the nested acquisitions run "while the caller still holds the `QuestStageFragments`/`QuestStageState`/`QuestObjectiveState` resource locks for the whole cascade loop" and counts "12 component-storage acquisitions". `QuestStageFragments` is a clone, never a guard; since the guard-free rework the two quest guards are scoped per fragment and re-acquired per provider tail; the real count is ~15 storage types across ~25 sites plus `EquipItemCatalog`, `SceneRegistry`, `PapyrusPlayerEntity` ×2, `FormIdPool`, and the `FragmentExecutionQueue` write.
- **Suggested Fix**: rewrite around `apply_fragment_guard_free`'s per-fragment scope; list acquisitions by helper rather than a hand count.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other decompiler passes / the other fragment producers / the sibling recognizer)
- [ ] **LOCK_ORDER**: If a RwLock/guard scope changes, the canonical order in `docs/engine/ecs.md` is preserved and `BYRO_LOCK_ORDER_CHECK=1` stays green
- [ ] **TESTS**: A regression test pins this specific fix
