# #3947 — SCR-D5-2026-09-06-06: `CanonicalEvent::from_papyrus` has no production caller — `tables.rs` claims it is "the *only* place Papyrus event names are interpreted" while two live sites match names inline

- **Finding ID**: SCR-D5-2026-09-06-06
- **Labels**: low,scripting,tech-debt,bug
- **Filed**: 2026-09-06 by /audit-publish from `docs/audits/AUDIT_SCRIPTING_2026-09-06.md`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3947

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-06.md` — `/audit-scripting` pass 2026-09-06 (seventeenth). Verified against `main` at HEAD on 2026-09-06.

- **Severity**: LOW
- **Dimension**: Recognizer-Chain Soundness · **Untrusted-Input**: No · **Location**: `crates/scripting/src/translate/tables.rs:28-31, 65-79`; inline interpreters `quest_stage_gate.rs:215-237`, `papyrus_provider/lower_program.rs:63-79` · **Status**: NEW
- **Suggested Fix**: route `find_advance_event`/`lower_event_into` through it, or delete it and fix the module doc.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other decompiler passes / the other fragment producers / the sibling recognizer)
- [ ] **LOCK_ORDER**: If a RwLock/guard scope changes, the canonical order in `docs/engine/ecs.md` is preserved and `BYRO_LOCK_ORDER_CHECK=1` stays green
- [ ] **TESTS**: A regression test pins this specific fix
