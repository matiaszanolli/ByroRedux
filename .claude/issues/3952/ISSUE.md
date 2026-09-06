# #3952 — SCR-D6-2026-09-06-06: the load-bearing scene→quest→continuation→cleanup ordering has no regression pin; only the flush/quest_advance half is tested

- **Finding ID**: SCR-D6-2026-09-06-06
- **Labels**: low,scripting,ecs,test-gap,bug
- **Filed**: 2026-09-06 by /audit-publish from `docs/audits/AUDIT_SCRIPTING_2026-09-06.md`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3952

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-06.md` — `/audit-scripting` pass 2026-09-06 (seventeenth). Verified against `main` at HEAD on 2026-09-06.

- **Severity**: LOW
- **Dimension**: Scripting Runtime Systems · **Untrusted-Input**: No · **Location**: `byroredux/src/boot.rs:2388-2419` (existing test) vs `:1045-1090, 1904` · **Status**: NEW
- **Description**: `activation_flush_is_scheduled_before_every_activate_event_consumer` pins flush < {rumble, quest_advance, two_state} and quest_advance < quest_fragment only; `scene_playback → scene_fragment_dispatch → quest_fragment_dispatch → fragment_continuation` and cleanup-last are unpinned, and #3739's 750-line move is exactly the edit class that reorders them unnoticed.
- **Suggested Fix**: extend the static-source test with those relations and `rfind(event_cleanup_system)` > every other Late `add_exclusive`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other decompiler passes / the other fragment producers / the sibling recognizer)
- [ ] **LOCK_ORDER**: If a RwLock/guard scope changes, the canonical order in `docs/engine/ecs.md` is preserved and `BYRO_LOCK_ORDER_CHECK=1` stays green
- [ ] **TESTS**: A regression test pins this specific fix
