# #5305: ECS-2026-10-05-D1-01: `footstep_system` takes `CharacterController` and `WaterContact` while holding the `GlobalTransform` / `FootstepEmitter` guards, closing three production lock-order cycles — the ABBA lane is red (5 panics)

**Labels**: high,ecs,concurrency,audio,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5305

**Source**: `docs/audits/AUDIT_ECS_2026-10-05.md` — `ECS-2026-10-05-D1-01` (HEAD `a2c24b16e`)

- **Severity**: HIGH. ECS deadlock potential has a floor of HIGH, and the lock-order CI job is red at HEAD (the same grading as #3819, #4982 and #5025).
- **Dimension**: 1 — Lock Ordering
- **Location**: `byroredux/src/systems/audio.rs:175-226` (`footstep_system`, Phase 1)
- **Status**: NEW (introduced by `ccc743160`, #5146)
- **Description**: Phase 1 already held three guards: the `FootstepScratch` resource write, the `GlobalTransform` read query and the `FootstepEmitter` write query. #5146 added two per-entity gets inside that same scope, for the player body: `world.get::<CharacterController>(entity)` and `world.get::<WaterContact>(entity)`. That records the new edges `GlobalTransform → CharacterController` and `GlobalTransform → WaterContact`, plus the same edges from `FootstepEmitter` and `FootstepScratch`.

  The reverse edges were already in production:

  | Edge | Source (production) |
  |---|---|
  | `CharacterController → Transform` | `character_controller_system` (`systems/character.rs:246-258`, the `cq` guard held across the `Transform` read) |
  | `Transform → GlobalTransform` | transform propagation |
  | `WaterContact → GlobalTransform` | `make_water_interaction_system` (`systems/water.rs:419-423`) |
  | `WaterContact → ActorVitals → ActorValues` | `water_damage_system` (`systems/water.rs:25-49`) |

  These close three cycles:
  - **A**: `CharacterController → Transform → GlobalTransform → CharacterController`
  - **B**: `GlobalTransform → WaterContact → GlobalTransform`
  - **C**: `ActorVitals → ActorValues → GlobalTransform → WaterContact → ActorVitals`

  A dedicated guard already existed for cycle A. `camera_follow_does_not_close_character_lock_cycle` (`systems/character.rs:1706-1740`) records the two production edges and states the rule: *"A subsequent GlobalTransform -> CharacterController edge would now close the three-lock cycle."* That edge is exactly what landed.

  The source comment reasons that *"the system is a Late exclusive, so nothing runs concurrently against these locks"*. That is the #4616 trap: the detector records process-wide edges, and it cannot tell an exclusive caller apart from a parallel one.
- **Evidence**:
  - Panics in `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux`, identical in CI run 37341506292:
    - `camera_follow_does_not_close_character_lock_cycle`: *attempted acquisition of `Transform` while holding `CharacterController` … `Transform → GlobalTransform → CharacterController → Transform`*
    - `a_weaker_body_disturbance_does_not_clobber_a_stronger_camera_one`, `both_splash_producers_host_the_marker_on_the_water_plane`, `dynamic_water_contact_emits_entry_once_and_ripples_while_wet`: *`GlobalTransform → WaterContact → GlobalTransform`*
    - `water_damage_system_applies_contact_hazard_and_marks_death`: *`ActorVitals → ActorValues → GlobalTransform → WaterContact → ActorVitals`*
  - Attribution: `-- --skip footstep_tests` gives 2636 passed and 0 failed. `--test-threads=1 footstep_tests systems::water::tests systems::character::tests::camera_follow` reproduces 4 of the 5.
  - The new nest:
    ```rust
    let Some(gt_q) = world.query::<GlobalTransform>() else { return; };
    let Some(mut fs_q) = world.query_mut::<FootstepEmitter>() else { return; };
    for (entity, fs) in fs_q.iter_mut() {
        ...
        let controller = world.get::<byroredux_physics::CharacterController>(entity);
        let swimming = match (&controller, world.get::<WaterContact>(entity)) { ... };
    ```
- **Impact**:
  - There is no runtime deadlock today. Every participant runs serially: `footstep_system`, `water_damage_system` and `make_water_interaction_system` are Late exclusives, and `character_controller_system` and propagation run in other stages.
  - The cycles are entirely production code, so promoting any one participant to parallel, or adding a parallel reader with one of these orders, makes it real.
  - The lane cannot catch any other new cycle while it is red. This is the sixth red streak, after #3580, #3819, #4603, #4982 and #5025.
- **Related**: #5025 (the previous red streak), #4616 (the hoisting trap), #2135 (the `character_controller_system` ordering note).
- **Suggested Fix**: Before taking the `GlobalTransform` / `FootstepEmitter` guards, resolve the one body's gate as an owned value:
  1. Copy `is_grounded`, `half_height` and `radius` from `world.get::<CharacterController>(body)`.
  2. Copy `depth` from `world.get::<WaterContact>(body)`.
  3. Compute `body_gated: bool`.

  The Phase 1 loop then only compares `entity == body`. Hoist it above the `FootstepScratch` acquisition as well, so no edge leaves the scratch resource.

## Publisher note

Independently reproduced by `AUDIT_CONCURRENCY_2026-10-05.md` (cross-referenced there, not re-filed): exactly the same five detector panics; the scripting graph is green. The footstep body also holds the `CharacterController` guard across the `WaterContact` get (`CharacterController → WaterContact`), but `character_controller_system` records the same edge in the same direction, so that one closes nothing.

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **TESTS**: A regression test pins this specific fix
