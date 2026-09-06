# #3937 — SCR-D8-2026-09-06-01: #3838 hoisted the `SceneRegistry` read guard to function scope, so `scene_trigger_actor_approach_system_inner` now inverts the canonical scene/quest lock order and closes a cycle against `actor_quest_trigger_is_in_sequence`

- **Finding ID**: SCR-D8-2026-09-06-01
- **Labels**: high,scripting,concurrency,ecs,bug
- **Filed**: 2026-09-06 by /audit-publish from `docs/audits/AUDIT_SCRIPTING_2026-09-06.md`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3937

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-06.md` — `/audit-scripting` pass 2026-09-06 (seventeenth). Verified against `main` at HEAD on 2026-09-06.

- **Severity**: HIGH (latent — see Impact; `_audit-severity.md` "ECS deadlock potential" and the domain table's "ECS lock held across a second resource/component mutation")
- **Dimension**: Havok Idle / Cinematic Slice
- **Untrusted-Input**: No
- **Location**: `byroredux/src/systems/cinematic.rs:460-681` (introduced by `a3980338`, Fix #3838, 2026-09-05); counter-edge at `crates/scripting/src/trigger.rs:341-369`
- **Status**: NEW
- **Description**: Before #3838 the `SceneRegistry` guard lived inside the block that computed `(awaited, between_scenes)` and was dropped when that block ended (`18a6bc94:cinematic.rs:415`, inside `let (awaited, between_scenes) = { … }`). The scratch rework replaced the block with straight-line `clear()`/`extend()` calls and bound the guard at function scope: `let Some(registry) = world.try_resource::<SceneRegistry>() else { return; };` (`:460`). Rust drops a guard at end of scope, not last use, so `registry` is now alive to `:681` — across `world.query::<QuestAdvanceOnActivate>()` (`:500`), `TriggerVolume` (`:501`), `QuestStageState` (`:502`), `QuestTriggerApproachRegistry` (`:503-505`), `evaluate_condition_list` (`:557`, which re-reads `SceneRegistry`/`ScenePlayer` in its `IsSceneActionComplete` arm), `SceneAliasCandidate`/`RemoteSceneActorStub`/`Transform` reads (`:601-616`), a `Transform` **write** (`:651`), `set_kinematic_translation` → `PhysicsWorld` write (`:660`), and the `OnTriggerEnterEvent` write (`:664`). Every one records a `SceneRegistry → X` edge in the lock tracker. `docs/engine/ecs.md:658` pins the canonical order as `QuestAdvanceOnActivate → ScenePlayer → QuestStageState → SceneRegistry`, and `:668-675` explains that #3580 specifically required the registry guard to be *dropped* before `QuestStageState` in the sibling gate. That gate, `actor_quest_trigger_is_in_sequence`, still holds `QuestAdvanceOnActivate` (`trigger.rs:344`) and `ScenePlayer` (`:358`) while acquiring `SceneRegistry` (`:369`) — unconditionally, on every BaseForm-gated trigger entry. Together: `QuestAdvanceOnActivate → SceneRegistry → QuestAdvanceOnActivate`.
- **Evidence**: orchestrator confirmed `:460` at function scope, `grep drop(registry)` → none, function closes at `:681`, `QuestAdvanceOnActivate` acquired at `:500`; `trigger.rs:344/369` counter-edge; `ecs.md:658` canonical order; pre-#3838 block scoping at `18a6bc94:415`.
- **Impact**: No deadlock today — both systems are `add_exclusive` (`boot.rs:1017`, `:1023`), the "circumstantial" safety `ecs.md:693-696` warns about, one `add_to_with_access` promotion away from a real ABBA. What does fire: with `BYRO_LOCK_ORDER_CHECK=1` in a debug build, the detector panics the process (`lock_tracker.rs:272-290`) the first time the MQ101 cart sequence both runs the approach system and has the horse enter a resident BaseForm trigger. **Not CI-red at HEAD**: `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux` → 1923 passed, because the gate lives in `crates/scripting`'s test binary and no `byroredux`-bin test drives both in one process. The eight new `SceneRegistry → X` edges (incl. two writes and `PhysicsWorld`) each widen the surface for further cycles.
- **Disproof attempted**: no `drop(registry)` or re-scoping block; `try_resource` is lock-tracked (`world.rs:718`); the gate's `advances` guard is used again at `trigger.rs:420` so it is alive at `:369`; ran the full binary suite under the detector (green — and explained why); confirmed pre-#3838 code recorded no edge out of `SceneRegistry`; `a3980338`'s message discusses only the `ScenePlayer`-before-`SceneRegistry` clone rationale — the widening was unintentional.
- **Related**: #3838 (CLOSED — introducing fix), #3580 (CLOSED — same pair fixed in the sibling gate), #3651 (canonical-order doc), #3446 (CLOSED — source-scan guard pattern reusable here). Cross-reference Dim 6 (`trigger.rs`), `/audit-concurrency` Dim 3.
- **Suggested Fix**: end the guard where the old block did — wrap `:460-497` in a block that owns `registry`, or `drop(registry);` right after `between_scenes.extend(…)` at `:497`. Add a `byroredux`-side test that enables the lock tracker, installs a BaseForm trigger + running `ScenePlayer` on one `World`, and runs `trigger_detection_system` then the approach closure — must not panic; or a source-scan test asserting the registry guard's scope closes before the `QuestAdvanceOnActivate` query. Update the `SceneTriggerApproachScratch` doc (`:415-418`), which records only the `ScenePlayer` half of the rationale.

---

### MEDIUM

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other decompiler passes / the other fragment producers / the sibling recognizer)
- [ ] **LOCK_ORDER**: If a RwLock/guard scope changes, the canonical order in `docs/engine/ecs.md` is preserved and `BYRO_LOCK_ORDER_CHECK=1` stays green
- [ ] **TESTS**: A regression test pins this specific fix
