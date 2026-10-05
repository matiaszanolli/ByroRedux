# #5298 — SCR-D1-2026-10-05-01: The "player receiver declines" rule in SetUnconscious / StartCombat / StopCombat is syntactic — a PlayerRef property lowers, and since #4694 resolves to the player at runtime

- **Labels**: medium,scripting,bug
- **Filed from**: `docs/audits/AUDIT_SCRIPTING_2026-10-05.md` (2026-10-05 comprehensive suite)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5298

- **Severity**: MEDIUM
- **Dimension**: Recognizer Chain
- **Untrusted-Input**: No
- **Location**:
  - `crates/scripting/src/translate/effects.rs:1392-1405`: `prim_set_unconscious`.
  - `crates/scripting/src/translate/effects.rs:1353-1370`: `prim_start_combat`.
  - `crates/scripting/src/translate/effects.rs:1376-1384`: `prim_stop_combat`.
  - `crates/scripting/src/translate/effects.rs:1682-1688`: `player_expr_ref`.
  - `crates/scripting/src/translate/effects.rs:1739-1745`: `receiver_actor`.
  - `crates/scripting/src/fragment/effects.rs:88-118`: `resolve_object`, whose `Object{alias:-1}` arm calls
    `resolve_entity_by_global_form_id`.
  - `crates/scripting/src/condition.rs:529-534`: the `0x14` case resolves to `PapyrusPlayerEntity`.
- **Status**: NEW. This is a sibling gap of #4323 (closed; its SIBLING checkbox was left unchecked). #4694 (closed, `a70b54f14`)
  opened it: before #4694 a `PlayerRef` property resolved to nothing and the effect declined at runtime.
- **Description**: `actor == ActorRef::Player` is true only for `Game.GetPlayer()` or a local bound from it. Skyrim's
  ubiquitous `Actor Property PlayerRef Auto` (VMAD `Object { form_id: 0x14, alias: -1 }`) lowers as
  `ActorRef::Object(Property("PlayerRef"))`. A player-filled quest alias lowers the same way. At apply time both resolve
  to the player entity. The primitive's own doc says the player is unmodeled ("an unconscious player would need the
  controls, camera and HUD handling …").
- **Evidence**: `PlayerRef.SetUnconscious(true)` passes the `ActorRef::Player` check. At runtime the
  `Effect::SetUnconscious` arm (`fragment/effects.rs:1559`) calls `set_unconscious(true)` on the player's
  `ActorControlState`, which clears `restrained` (`player_control.rs:138-143`). `player_accepts_movement_input` reads
  `restrained` (`byroredux/src/systems/character.rs:142-150`). `PlayerRef.StartCombat(X)` reproduces #4323's impact
  through the same route.
- **Impact**:
  - **`SetUnconscious`**: a scripted restraint is lifted mid-cinematic, so the player can move again. A saved
    `unconscious` flag is left on the player, and nothing ever reads or clears it.
  - **`StartCombat`**: `npc_combat_ai_system` steers the player's `Transform` and auto-strikes, which is #4323 again.
  - **Reachability**: how often vanilla uses `PlayerRef` with these three calls was not measured. That would need game
    data and the decompiler examples, which are out of scope for a static run.
- **Related**: #4323, #4694, #5017.
- **Suggested Fix**: at apply time, decline when the resolved actor is the `PapyrusPlayerEntity`, in all three arms. A
  shared `resolve_npc_actor` would do it. Lowering cannot see the VMAD value, so the static check alone cannot close
  this. Add a test that drives a `PlayerRef` property through `apply_effect`.

_Source: `AUDIT_SCRIPTING_2026-10-05.md` (SCR-D1-2026-10-05-01), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
