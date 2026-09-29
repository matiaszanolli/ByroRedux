# PHYS-D4-2026-09-29-01: the third-person boom moves every camera-ray gameplay cast 180 BU behind the head — melee reach ends at the player's own head, activation keeps 12 BU of 192

**Labels**: medium,bug,physics,gameplay,combat

**Source**: `docs/audits/AUDIT_PHYSICS_2026-09-29.md`
**Severity**: MEDIUM
**Dimension**: Character & NPC Controller (consumers owned by `/audit-gameplay`: interaction/combat)
**Location**:
- `byroredux/src/systems/character.rs`: `THIRD_PERSON_BOOM_BU = 180.0` and the `PlayerCameraView::ThirdPerson` arm of `camera_follow_system` (`head_pos - (cam_rot * -Vec3::Z) * THIRD_PERSON_BOOM_BU`)
- `byroredux/src/interaction.rs`: `INTERACTION_REACH_BU = 192.0`, `camera_ray` (reads the `ActiveCamera` Transform), activation target selection and occlusion casts
- `byroredux/src/combat.rs`: `MELEE_REACH_BU = 180.0`, player swing ray via `camera_ray`

**Trigger Conditions**: Character mode, `player.view third` / V key, then attack or activate anything. Worse with a wall behind the player.

## Description
`a070baaad` added `THIRD_PERSON_BOOM_BU = 180.0`. In third person, `camera_follow_system` sets `cam_pos = head_pos - forward * 180`. Every player gameplay ray takes its origin from the camera (`interaction::camera_ray` reads the `ActiveCamera` Transform):
- activation target selection, with `INTERACTION_REACH_BU = 192`;
- the interaction occlusion casts;
- the player swing (`combat.rs`), with `attack_reach_bu` = `MELEE_REACH_BU` (180) × weapon reach;
- the studio host and `commands/view.rs` rays.

None compensates for the boom. The camera-collision probe is documented as missing, so the ray origin can also sit behind or inside the wall at the player's back.

## Evidence
```rust
// character.rs
crate::player_body::PlayerCameraView::ThirdPerson => {
    head_pos - (cam_rot * -Vec3::Z) * THIRD_PERSON_BOOM_BU
}
// interaction.rs — pub(crate) fn camera_ray(world) -> (camera Transform.translation, forward)
// combat.rs   — pub(crate) const MELEE_REACH_BU: f32 = 180.0;
// interaction.rs — pub(crate) const INTERACTION_REACH_BU: f32 = 192.0;
```

## Impact
In third person:
- A reach-1.0 or unarmed swing's ray stops at the player's head and hits nothing in front of the player.
- Activation reaches only 12 BU past the head, so doors, containers, NPCs, loot and dialogue are effectively out of reach.
- With a wall behind the player, the first solid the ray meets is that wall (convex collider → toi = 0 with `solid = true`; TriMesh → back face), which occludes every target.

Only the player capsule is excluded. Switching back to first person is the workaround. The `p3-player-body.sh` gate asserts "no self-targeting in `interaction.status`" and so cannot see lost reach.

## Related
CONC-D4-2026-09-28-02 / #4995 (same boom, body-yaw lag, closed); the slice doc's "no wall collision yet" follow-up; AUD-2026-09-29-D5-01 (same boom root cause, footstep consumer).

## Suggested Fix
In third person, cast gameplay rays from the eye (capsule position + `eye_height`) along the camera forward, or advance the origin by the boom length and keep the camera pose for rendering only. Extend the p3 gate to activate a target ~100 BU away in third person.

Validated at HEAD 9fcfdc3fc: `THIRD_PERSON_BOOM_BU = 180.0` and the boom arm exist in `character.rs`; `camera_ray` returns the `ActiveCamera` Transform translation; `combat.rs` swing and interaction selection both call `camera_ray` with no third-person compensation.

## Completeness Checks
- [ ] **SIBLING**: every `camera_ray` caller (interaction, combat, studio host, `commands/view.rs`) routed through the same eye-origin helper
- [ ] **LOCK_ORDER**: if the helper reads `PlayerEntity`/`CharacterController`, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins third-person activation/melee reach to a target in front of the player
