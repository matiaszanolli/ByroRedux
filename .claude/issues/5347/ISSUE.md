# #5347 — AUD-2026-10-05-D5-01: The footstep swim gate duplicates is_grounded, so its test cannot detect its removal — the redundant WaterContact read is the edge that closes cycles B and C of ECS D1-01 (#5305)

- **Labels**: low,audio,test-gap,bug
- **Filed from**: `docs/audits/AUDIT_AUDIO_2026-10-05.md` (2026-10-05 comprehensive suite)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5347

- **Severity**: LOW. There is no audible defect. The lock-order consequence is graded HIGH under ECS-2026-10-05-D1-01 and is not double-counted here.
- **Dimension**: Engine Consumers
- **Location**:
  - `byroredux/src/systems/audio.rs:205-216`: the gate.
  - `byroredux/src/systems/character.rs:1676-1684`: `resolve_ground_contact`.
  - `character.rs:297-303`: the swim verdict.
  - `character.rs:1328-1358` and `:620`: `sync_player_water_contact`.
  - `systems/audio.rs:857-917`: the test.
- **Status**: NEW (introduced by `ccc743160`, #5146)
- **Description**: The #5146 body gate is `if !grounded || swimming`, where `swimming = depth_reaches_swimlevel(WaterContact.depth, half_height + radius)`. In production that second operand cannot change the result:
  1. `character_controller_system` computes `swim` from `player_water_state` at the **pre-move** pose (`:297-303`). It passes `swim.is_some()` to `resolve_ground_contact`, which returns `(false, …)` whenever it is swimming (`:1682-1684`). So `is_grounded` is false on every swimming frame.
  2. The same tick publishes the retained `WaterContact` with `depth = surface_y - current_pos.y`, measured at the same pre-move pose (`sync_player_water_contact`; its doc says so explicitly). `depth_reaches_swimlevel(contact.depth, …)` therefore equals that frame's `swim.is_some()`. This is the same round-trip that `was_swimming` relies on.

  So `!grounded || swimming` reduces to `!grounded`. The regression test `swimming_body_is_silent_and_exit_does_not_replay_the_swim_distance` sets `is_grounded = false` **and** `depth = 30` for the whole swim leg (`:860-872`). It would still pass with the `WaterContact` get deleted, or with the predicate replaced by `false`. It only catches an always-true break, through the exit step.

  The redundant read is not free. ECS-2026-10-05-D1-01 attributes cycle B (`GlobalTransform → WaterContact → GlobalTransform`) and cycle C (`ActorVitals → ActorValues → GlobalTransform → WaterContact → ActorVitals`) to exactly this `world.get::<WaterContact>` under the `GlobalTransform` / `FootstepEmitter` guards. Cycle A (`… → CharacterController`) comes from the `CharacterController` get, which is needed.
- **Evidence**:
  ```rust
  // character.rs:1682
  if swimming { return (false, vertical_velocity); }
  // systems/audio.rs:210-216
  let swimming = match (&controller, world.get::<WaterContact>(entity)) {
      (Some(c), Some(contact)) => depth_reaches_swimlevel(contact.depth, c.half_height + c.radius),
      _ => false,
  };
  if !grounded || swimming { … }
  ```
  - Reproduction of the D1-01 edges: `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux -- --test-threads=1 footstep_tests systems::water::tests systems::character::tests::camera_follow` gives 28 passed and 4 failed.
  - The four failures are `camera_follow_does_not_close_character_lock_cycle` (cycle A) and three `systems::water::tests` (cycle B).
- **Impact**:
  - Today there is none audibly. The extra read is a dead operand that holds a lock edge.
  - The test gives false assurance: if someone later decouples swimming from `is_grounded` (for example, a swim state that keeps "grounded" for wading on the lake floor), the swim gate's silence will rest on a predicate no test exercises.
- **Related**: ECS-2026-10-05-D1-01 (HIGH, the fix owner), cross-referenced in `AUDIT_CONCURRENCY_2026-10-05.md`. Also #5146 (closed) and PHYS-D5 sampler-parity in `AUDIT_PHYSICS_2026-10-05.md`, which notes swim and camera submersion can disagree near the waterline. That is a different sampler, not this gate.
- **Suggested Fix**:
  - Before taking any query guard, resolve the body's gate once: `let body_grounded = player_body.and_then(|b| world.get::<CharacterController>(b).map(|c| c.is_grounded));`. Then drop the `WaterContact` read, or derive it in the same pre-pass if it is kept as defence in depth. This closes all three D1-01 cycles.
  - Split the swim test so that one leg holds `is_grounded = true` with a swimlevel depth, if the gate stays. Otherwise document that swimming is covered through `resolve_ground_contact`.

Lock-order owner: ECS-2026-10-05-D1-01 is filed as #5305 (HIGH). This issue covers only the redundant swim operand and the test that cannot detect its removal; dropping the `WaterContact` read is one way to close cycles B and C there.

_Source: `AUDIT_AUDIO_2026-10-05.md` (AUD-2026-10-05-D5-01), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
