# #5349 — AUD-2026-10-05-D5-03: #5146 left three stale or wrong doc sites about which entity carries the footstep emitter

- **Labels**: low,audio,doc-rot,documentation
- **Filed from**: `docs/audits/AUDIT_AUDIO_2026-10-05.md` (2026-10-05 comprehensive suite)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5349

- **Severity**: LOW
- **Dimension**: Engine Consumers / Manager & Schedule (doc)
- **Location**:
  - `byroredux/src/scene.rs:1227-1229`: "The camera's emitter (inserted in `setup_camera_and_lights`)". No such function exists (`git grep` has 0 definitions; the insert is in `spawn_initial_camera`, `scene.rs:903`/`:982`).
  - `scene.rs:977-981`: "character mode **moves** it onto the player body". Nothing moves: the body gets a second emitter and the camera keeps its own (see D5-02).
  - `byroredux/src/components.rs:1717-1719`: the `FootstepEmitter` doc says "today the fly-camera entity; an `M28.5` character controller will own this in future". The body has owned it since #5146.
  - `byroredux/src/boot/schedule/late.rs:83-99`: the footstep pin rationale still says that in player/third-person mode the system reads "`camera_follow_system`'s pose, not last frame's". In character mode the only accumulating emitter is now the body, whose `GlobalTransform` comes from Early `character_controller_system` plus PostUpdate propagation. `camera_follow_system` no longer feeds any audible footstep, and the camera emitter is only re-seeded. `footstep_runs_after_camera_follow_in_late` still pins an order that is now harmless but unmotivated.
- **Status**: NEW (introduced by `ccc743160`)
- **Description**: see Location. The dead function name is the kind of path/symbol reference that `_audit-validate.sh`-style checks flag. The `components.rs` "in future" line is directly falsified by the commit.
- **Impact**: documentation only. The next reader of the Late pin will look for a camera-pose dependency that no longer exists.
- **Related**: D5-02, #4146 (the earlier comment drift class in the same file).
- **Suggested Fix**:
  - Name `spawn_initial_camera` at `scene.rs:1228`.
  - Say "adds a body emitter" at `:979`.
  - Rewrite the `FootstepEmitter` doc to say the body carries it in character mode and the camera in FlyCam boots.
  - Re-word the `late.rs` rationale. The Late pin now matters only for FlyCam-boot camera emitters, or it could move to PostUpdate after propagation if it is kept for the body alone. Keep the pin test or retire it accordingly.

_Source: `AUDIT_AUDIO_2026-10-05.md` (AUD-2026-10-05-D5-03), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
