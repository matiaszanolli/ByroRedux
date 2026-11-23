# #7291: SCR-D3-01: An explicit EvaluatePackage raised between `ambient_ai_package_system` and `scene_package_system` is invisible to the ambient pass, and its Must Complete release is lost

- **Severity**: MEDIUM
- **Dimension**: Event Runtime
- **Location**: `crates/scripting/src/package.rs:685-688` (drain), `byroredux/src/npc_spawn/ai_package.rs:1381-1397,1507` (read), `byroredux/src/boot/schedule/update/quest_scene/package_fragments.rs:12` and `scene_actions.rs` (order)
- **Status**: NEW. Related closed issues: #7243 (the release marker), #2652 (the "observe before the SCEN drain" ordering).
- **Untrusted-Input**: No
- **Description**: `EvaluatePackageRequest` has two consumers. `ambient_ai_package_system` only reads it (`requested`), and `scene_package_system` snapshots and drains it. The cleanup.rs table lists only the second, as a Pattern B single owner. The ambient pass sees the request only if the request was raised before it in the same Update, or in a later stage of the previous frame. Update order is `ambient_ai_package_system` (package_fragments), then `package_fragment_system`, `perk_*`, `terminal_fragment_system`, `scripted_conversation_system`, `forcegreet`, `obscript_quest_tick_system`, `obscript_quest_menu_mode_system`, `obscript_stage_result_system`, and then `scene_package_system` (scene_actions, first row). Any `EvaluatePackage` raised in that window is drained the same frame by `scene_package_system`. The next ambient pass has `requested` empty.
  - **Which producers are in the window**: Quest and stage-result ObScript (`obscript_actions::evaluate_package`, reachable through `obscript_quests.rs:703` and the command table). Skyrim PF_/PRKF_/TERM_ fragments (`Effect::EvaluatePackage`, `fragment/effects/ai_combat.rs:184`).
  - **What is lost**: The actor is not forced due, so the re-evaluation waits for the next in-game-minute gate. The `ReleasePackageHold` marker (#7243) is taken by the next ambient pass and discarded, because `release_hold.contains(&actor)` is consulted only for actors in `due`. A Must Complete package therefore keeps its hold, which is the exact #7243 symptom on this path.
- **Evidence**:
  - `crates/scripting/src/package.rs:685-688`:
    ```rust
    let reevaluate: HashSet<EntityId> = snapshot::<EvaluatePackageRequest>(world).into_keys().collect();
    drain::<EvaluatePackageRequest>(world);
    ```
  - `ai_package.rs:1381-1397`: `requested` is a read-only collect. `release_hold` is collected, then every marker is removed immediately, before the `due` gate.
  - `package_fragments.rs:12`: `scheduler.add_exclusive(Stage::Update, crate::npc_spawn::ambient_ai_package_system);`
  - `quest_scene.rs` calls the segments in the order `package_fragments`, `forcegreet`, `obscript_quests`, `scene_actions`.
  - The pin `an_explicit_evaluate_package_ends_a_must_complete_hold` (`ai_package.rs:3128-3153`) calls `request_package_evaluation` and then `ambient_ai_package_system` back to back. It never runs `scene_package_system` between them, and `ambient_ai_package_system_takes_its_release_marker` only checks the drain.
- **Impact**: An `EVP` from a quest or stage-result script, or from a package/perk/terminal fragment, drops the explicit-request semantics. For ordinary packages the effect is a delay of up to one in-game minute (about 2 s at the default time scale). For a Must Complete package the release is permanently lost, so the actor stays on the held package. That is exactly #7243's symptom, which is therefore only fixed for producers outside the window.
- **Related**: #7243, #7264, the `EvaluatePackageRequest` row in the cleanup.rs table (it is not single-owner).
- **Suggested Fix**: Make the marker single-owner for real. Either schedule `ambient_ai_package_system` after `scene_package_system` and let it take both `EvaluatePackageRequest` and `ReleasePackageHold`, with the scene pass only snapshotting. Or stamp a "seen by ambient" side set that gates the scene pass's drain. Add a schedule-order test (`BuiltSchedule::position`) plus a world test that runs both systems in schedule order.

**Found by**: `docs/audits/AUDIT_SCRIPTING_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other latent / suspending paths, the other drain owners, the other ObScript hosts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the `apply_effect` lock inventory is updated
- [ ] **TESTS**: A regression test pins this specific fix
