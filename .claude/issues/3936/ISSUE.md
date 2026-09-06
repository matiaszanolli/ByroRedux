# #3936 — SCR-D6-2026-09-06-01: `scene_package_system`'s Package `Activate` leaf inserts `ActivateEvent` after two of its four consumers have already run — the #2654 class, unpatched for the package producer

- **Finding ID**: SCR-D6-2026-09-06-01
- **Labels**: high,scripting,quests,ecs,bug
- **Filed**: 2026-09-06 by /audit-publish from `docs/audits/AUDIT_SCRIPTING_2026-09-06.md`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3936

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-06.md` — `/audit-scripting` pass 2026-09-06 (seventeenth). Verified against `main` at HEAD on 2026-09-06.

- **Severity**: HIGH (domain table: transient marker drained out of stage order)
- **Dimension**: Scripting Runtime Systems
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/package.rs:615-625` (producer); `byroredux/src/boot.rs:1009, 1025, 1060, 1065, 1144, 1904` (schedule)
- **Status**: NEW (predates the last pass — `583a349a`, present at `18a6bc94` — but no `AUDIT_SCRIPTING_*.md` adjudicated it and no open issue matches; #2654, the fragment-side twin, is CLOSED)
- **Description**: `tick_command`'s `TimedInteraction { procedure_type == "Activate" }` arm does `events.insert(target, ActivateEvent { activator: action.actor })` directly. `scene_package_system` is registered at `boot.rs:1060`; `ActivateEvent` is Pattern-A, drained by `event_cleanup_system` at `Stage::Late`. Of the four Update-stage consumers, `rumble_on_activate_dispatch` (`:1009`) and `quest_advance_dispatch` (`:1025`) run *before* the producer and never see the marker; only `two_state_activator_system` (`:1065`) and `mg07_on_activate_dispatch` (`:1144`) do. The registration comment considers only the two-state consumer. This is the exact ordering defect #2654 fixed for fragment `Activate` by introducing `PendingFragmentActivations` + the head-of-frame flush — the fix exists in the same crate and was not applied to this second producer.
- **Evidence**: orchestrator re-read `package.rs:612-628` and the `boot.rs` registration lines; `quest_advance_system` reads `ActivateEvent` once per frame (`quest_advance.rs:348-352`) and its `ActivatorGate::Any` (default) / `BaseForm(u32)` accept a scene actor as activator, so a scene-authored NPC `Activate` on a quest-advance REFR is a modelled input that is unreachable from this producer.
- **Impact**: a scene whose package `Activate` targets a REFR carrying a recognised quest-advance script silently never advances the quest; the marker is consumed by the two-state system and drained the same frame. No log, no fallback. Corpus reachability of this exact shape was not measured.
- **Disproof attempted**: `quest_advance_system` does not re-scan later in the frame; the marker does not survive to the next frame (`drain_component::<ActivateEvent>` at `cleanup.rs:93`, cleanup last); `ActivatorGate` does not reject NPC activators; no prior report adjudicated it; the producer cannot simply move before `quest_advance` because it consumes `ScenePackageEventBatch` from `scene_playback_system`, which must follow the quest-start batch.
- **Related**: #2654 (CLOSED); `boot.rs:2388-2411` order test
- **Suggested Fix**: route the package `Activate` through the existing `PendingFragmentActivations` queue (expose a `pub(crate) fn push(&mut self, target, activator)`) so it is delivered at the next frame's head flush ahead of every consumer, exactly as fragments are; extend the `boot.rs` order test's producer list.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other decompiler passes / the other fragment producers / the sibling recognizer)
- [ ] **LOCK_ORDER**: If a RwLock/guard scope changes, the canonical order in `docs/engine/ecs.md` is preserved and `BYRO_LOCK_ORDER_CHECK=1` stays green
- [ ] **TESTS**: A regression test pins this specific fix
