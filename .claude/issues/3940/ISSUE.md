# #3940 — SCR-D5-2026-09-06-04: `recognize_specific_actor_trigger` collapses a present-but-wrong-typed VMAD `prereqStageOPT`/`disableWhenDone`/`onlyOnce` into the `.psc` default — the two-case collapse #2669 fixed in its sibling — so a mistyped prerequisite...

- **Finding ID**: SCR-D5-2026-09-06-04
- **Labels**: medium,scripting,quests,bug
- **Filed**: 2026-09-06 by /audit-publish from `docs/audits/AUDIT_SCRIPTING_2026-09-06.md`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3940

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-06.md` — `/audit-scripting` pass 2026-09-06 (seventeenth). Verified against `main` at HEAD on 2026-09-06.

- **Severity**: MEDIUM (exposure limited to corrupt / hand-edited plugins — the CK always writes matching type tags)
- **Dimension**: Recognizer-Chain Soundness
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/translate/recognizers/quest_stage_gate.rs:148-169, 191-192`; landed `7473a387` (2026-08-24), never in the checklist
- **Status**: NEW
- **Description**: `int_property("stage")?`/`object_property(..)?` decline correctly, but `int_property("prereqStageOPT").unwrap_or(-1)` maps "present but not `Int32`" onto "no prerequisite" and `bool_property(name, false)` maps "present but not `Bool`" onto `false` — the collapse the crate's own three-case contract (`two_state_activator.rs:71-89`, `effects.rs:1353-1368`) forbids. The wrong state is a missing `GetStageDone(prereq) == 1` condition on a `QuestAdvanceOnActivate`: the trigger fires without its prerequisite / re-fires every entry.
- **Disproof attempted**: CK type discipline bounds exposure but does not remove the contract violation; sibling fixes (#2669, #2023, #1909) were filed for the same shape and exposure.
- **Related**: #2669, #2023, #1909 (all CLOSED)
- **Suggested Fix**: three-case `Option<Option<T>>` closures with `?` on the outer `None`, mirroring `vmad_bool`; add `declines_specific_actor_trigger_on_mistyped_prereq`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other decompiler passes / the other fragment producers / the sibling recognizer)
- [ ] **LOCK_ORDER**: If a RwLock/guard scope changes, the canonical order in `docs/engine/ecs.md` is preserved and `BYRO_LOCK_ORDER_CHECK=1` stays green
- [ ] **TESTS**: A regression test pins this specific fix
