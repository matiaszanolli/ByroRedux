# #4995: CONC-D4-2026-09-28-02: `player_body_facing_system` writes in Late for a PostUpdate consumer; the one-frame body-yaw lag is avoidable, its justification is wrong, and nothing pins it

- **Repo**: matiaszanolli/ByroRedux
- **Labels**: low,ecs,gameplay,bug
- **Filed from**: docs/audits/AUDIT_CONCURRENCY_2026-09-28.md
- **Filed**: 2026-09-28

- **Severity**: LOW
- **Dimension**: Scheduler Access Declarations (cross-stage sequencing)
- **Location**: `byroredux/src/boot/schedule/late.rs:64-83`; `byroredux/src/player_body.rs:388-422` (doc + body)
- **Status**: NEW (a070baaad, today)
- **Description**:
  - **What it does.** The system writes the body root's `Transform.rotation` from `InputState.yaw`. The root is a child of the capsule (`player_body.rs:240-242`). Its only consumer is PostUpdate `transform_propagation`, which runs before Late. So the body is drawn with frame N-1's yaw, while `camera_follow_system` (Late, parallel) orbits the third-person boom with frame N's yaw (`character.rs:697-712`). On a fast turn, the body visibly trails the camera by one frame.
  - **Why the comment's reason fails.** The registration comment accepts this as "the same one-frame staleness every Late pose consumer here already accepts". That doesn't hold here. The other Late consumers sit in Late because their input, the post-Physics pose, only exists there (#3180/#3652). This system's only input is `InputState.yaw`, which is written between frames (`ui_input.rs:80`) and is final before the scheduler runs. No Physics-stage output is involved.
  - **Doc drift.** The fn doc (`player_body.rs:390`) says it "Runs beside `camera_follow_system` in the Late batch". It is registered exclusive, and would trip the analyzer (Transform WriteWrite) if someone "restored" it to the batch as the doc describes.
  - **No pin.** No test pins its stage or exclusivity. The four precedents (#3652, #3653, #4185, #4186) each have a pin test.
- **Evidence**: `late.rs:75-83` registers it as `add_exclusive_with_access(Stage::Late, …, .writes::<Transform>())`. `post_update.rs:11` is the propagation in the earlier stage. `grep player_body_facing` finds only its unit tests (`player_body.rs:672/686/702`) and the registration.
- **Trigger Conditions**: Third-person view (V key / `player.view third`) in Character mode, with mouse yaw changing between frames.
- **Impact**: The third-person body rotates one frame behind the camera (≈3° at 180°/s at 60 fps). This is cosmetic, but it is structural and permanent, and the registration comment wrongly calls it unavoidable.
- **Verification Path**: In third person, sweep the mouse and compare the body root's `GlobalTransform` rotation with `InputState.yaw` in the same frame via byro-dbg. Today they differ by one frame's delta.
- **Related**: #3652 (billboard, MEDIUM), #3653 (particle rate lag, pinned by `particle_emitter_rate_lag_is_structural`).
- **Suggested Fix**:
  - Register it as an Update exclusive, which also runs after the animation parallel batch, so the overwrite semantics are unchanged. PostUpdate propagation then composes it in the same frame.
  - Fix the fn doc.
  - Pin the placement (Update exclusive, before PostUpdate propagation) the way `billboard_runs_after_camera_follow_in_late` does.

Source: `docs/audits/AUDIT_CONCURRENCY_2026-09-28.md` (CONC-D4-2026-09-28-02) · HEAD `9e6f08870`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related systems / CI steps
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition and the `docs/engine/ecs.md` canonical order are preserved
- [ ] **TESTS**: A regression test pins this specific fix
