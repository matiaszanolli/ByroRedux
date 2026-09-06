# #3955 — SCR-D8-2026-09-06-03: #3838 left the approach system's doc comment attached to the scratch struct, and the system function itself is now undocumented

- **Finding ID**: SCR-D8-2026-09-06-03
- **Labels**: low,scripting,documentation,doc-rot
- **Filed**: 2026-09-06 by /audit-publish from `docs/audits/AUDIT_SCRIPTING_2026-09-06.md`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3955

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-06.md` — `/audit-scripting` pass 2026-09-06 (seventeenth). Verified against `main` at HEAD on 2026-09-06.

- **Severity**: LOW
- **Dimension**: Havok Idle / Cinematic Slice · **Untrusted-Input**: No · **Location**: `byroredux/src/systems/cinematic.rs:405-421, 436` · **Status**: NEW
- **Description**: the four-line "Bridge offscreen cinematic locomotion…" doc (`:405-408`) now runs straight into the scratch struct's doc (`:409-420`), so both attach to `SceneTriggerApproachScratch`; `fn scene_trigger_actor_approach_system_inner` (`:436`) has no doc.
- **Suggested Fix**: move `:405-408` above `:436`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other decompiler passes / the other fragment producers / the sibling recognizer)
- [ ] **LOCK_ORDER**: If a RwLock/guard scope changes, the canonical order in `docs/engine/ecs.md` is preserved and `BYRO_LOCK_ORDER_CHECK=1` stays green
- [ ] **TESTS**: A regression test pins this specific fix
