# AUD-2026-09-29-D5-01: footsteps follow the camera, so third-person camera turns and the view toggle play footsteps; swimming and airborne motion also play dirt footsteps

**Labels**: medium,bug,audio

**Source**: `docs/audits/AUDIT_AUDIO_2026-09-29.md`
**Severity**: MEDIUM
**Dimension**: Engine Consumers
**Location**:
- `byroredux/src/scene.rs`: `AudioListener` and the only `FootstepEmitter` are both inserted on the camera entity (`world.insert(cam, crate::components::FootstepEmitter::new())`).
- `byroredux/src/systems/character.rs`: `THIRD_PERSON_BOOM_BU = 180.0` and `cam_pos = head_pos - forward * 180` in `camera_follow_system`.
- `byroredux/src/systems/audio.rs`: `footstep_system`.
- `byroredux/src/app_events.rs`: the V key calls `toggle_third_person`.

## Description
`footstep_system` accumulates the XZ change of each `FootstepEmitter` entity's `GlobalTransform` and fires a footstep every 52.5 BU (`DEFAULT_STRIDE_THRESHOLD_BU`). The only emitter is the active camera — the right pose source while the camera was always at the player's eyes.
- **Third person** (`a070baaad`): the camera sits 180 BU behind the head along the look vector, so its XZ position moves when the player only looks around.
  - A full yaw turn with the body standing still moves the camera ~1131 BU along an arc → ~21 footsteps. A 90° mouse flick → ~5.
  - Pitch changes the arc radius, so looking up/down also counts.
  - Toggling the view moves the camera 180 BU at once → one footstep (same shape as the `single_large_jump_fires_one_footstep_only` test).
  - While walking in third person, steps play at the boom, 2.6 m behind the body.
- **Either view**: the system never reads locomotion state (`CharacterController.is_grounded`, swimming). Swimming (the WATAL W1 route) and horizontal motion in the air (running jumps, falls) play the dirt-walk footstep at walking cadence.

## Evidence
- `git grep FootstepEmitter -- byroredux/src ':!*test*'` finds a single production insert, on the camera in `scene.rs`.
- `camera_look_rotation(yaw, pitch) = Ry(yaw)·Rx(pitch)`, forward = rot·−Z (`systems/camera.rs`).
- `footstep_system`'s Access (`boot/schedule/late.rs`) lists only `FootstepConfig`, `FootstepScratch`, `AudioWorld`, `GlobalTransform` and `FootstepEmitter` — no `CharacterController`, `PlayerMode` or `PlayerCameraView` read.

## Impact
Audible and reachable in normal play on the character route with one key: every mouse turn in third person produces footsteps, and all swimming does too. Workaround: stay in first person and avoid water.

## Related
PHYS-D4-2026-09-29-01 / #5124 (same boom root cause, different consumer: camera-origin gameplay rays). #848, #3652, #4185 (footstep stage placement, closed); FOOT phase 3.5b.

## Suggested Fix
- In `PlayerMode::Character`, drive footsteps from the player body (the `PlayerEntity` capsule's `GlobalTransform`) instead of the camera; keep the listener on the camera.
- Accumulate stride only while grounded and not swimming; otherwise re-seed `last_position` and zero the accumulator.
- Add a unit test: an orbiting camera with a stationary body fires nothing.

Validated at HEAD 9fcfdc3fc: the only production `FootstepEmitter` insert is on the camera in `scene.rs`; `footstep_system` reads no `CharacterController` / `PlayerCameraView` / swim state; `THIRD_PERSON_BOOM_BU = 180.0` boom arm present in `character.rs`.

## Completeness Checks
- [ ] **LOCK_ORDER**: new reads (`PlayerEntity`, `CharacterController`) added to `footstep_system`'s Access row and acquired in TypeId order
- [ ] **SIBLING**: water splash/ripple emitters checked for the same camera-as-body assumption
- [ ] **TESTS**: orbiting-camera / stationary-body and swimming fixtures fire no footsteps
