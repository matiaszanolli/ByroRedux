# PHYS-D5-03

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2869

---

Found by `/audit-physics` Dimension 5 (Character Controller). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: MEDIUM · **Status**: NEW (defect in the landed #1874 fix)
**Location**: `byroredux/src/app_step.rs:709-721`, `byroredux/src/cell_loader/transition.rs:337-348` (`reposition_camera`) + `:411-423`, `byroredux/src/systems/character.rs:531-595` (`snap_character_body_to_camera`)

## Trigger Conditions
Any *runtime* door walk (`step_cell_transition`) — i.e. every door the player actually uses after boot. Does **not** affect the cold-start `setup_scene` path.

## Description
`reposition_camera` places the **camera** at the raw Y-up-converted XTEL destination position (`transition.rs:343`). `snap_character_body_to_camera` then places the capsule at `cam_pos - Vec3::Y * eye_height` (`character.rs:567`), i.e. centre at `dest.y - 52`, **feet at `dest.y - 116`**.

Door/XTEL destinations are at floor level — that is the premise the cold-start ladder is built on (`byroredux/src/scene.rs:1006-1009`, `byroredux/src/cell_loader/references/mod.rs:298`) — and the cold-start path consequently places the capsule *centre* at `floor_y + half_height + radius + kcc_offset = floor_y + 68` (`scene.rs:137-156`).

**The two paths disagree by 120 BU for the same door**, and the transition path runs **no** ground probe, no walkable-normal check, and no `is_grounded` verification — it sets `is_grounded = false` and hands the capsule to gravity from inside the floor.

`snap_character_body_to_camera` is correct for its original caller (`toggle_player_mode`, where the camera genuinely *is* at eye height); #1874 reused it for the transition path, where the camera had just been set to a floor-level door pose, and inherited the eye-height subtraction with it.

## Evidence
- `transition.rs:343` writes `transform.translation = dest_pos` (no eye offset added)
- `app_step.rs:710` derives `dest_pos` from `pending.destination_position_zup` via `position_zup_to_yup` (raw XTEL)
- `character.rs:564-567` subtracts `eye_height` (52.0, `crates/physics/src/components.rs:127`)
- capsule half-extent is 64 BU (`components.rs:124-126`)
- contrast `capsule_center_y_on_surface` (`scene.rs:137-144`), which *adds* `half_height + radius + kcc_offset_bu`

## Impact
After every door walk the capsule starts deeply embedded in the destination floor. Rapier's `check_and_fix_penetrations` is a stub and the body is kinematic, so nothing pushes it out: the character either sticks blocked-and-ungrounded (the #2193 failure mode) or, given PHYS-D5-02, falls through. The camera is pinned to the body by `camera_follow_system`, so the symptom is the view sinking into the floor on arrival.

## Suggested Fix
Route the transition arrival through the same grounding code as cold start — probe the destination XZ with `cast_capsule_down_onto_walkable_surface` (with PHYS-D5-01's corrected origin) and place the body via `character_spawn_center_y`, then let `camera_follow_system` derive the camera from the body, rather than deriving the body from a floor-level camera pose.

## Related
- PHYS-D5-01, PHYS-D5-02 (the other two halves of the door-threshold gap)
- #1874 (the transition-snap fix this sits inside), #2193 (CLOSED)

## Not established
Whether real Bethesda XTEL destination Y is *exactly* floor level per game. The 116 BU figure assumes the same "doors sit at floor level" premise the cold-start ladder itself asserts; the **120 BU inconsistency between the two engine paths holds regardless** of what the authored Y means.
