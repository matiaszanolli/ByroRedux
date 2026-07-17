# SAVE-D6-03: apply_player_pose silently reverts a FlyCam-saved pose within one frame when the live session is in Character mode

**Labels**: medium, tech-debt, bug

**Severity**: MEDIUM
**Dimension**: M45.1 Live Load-Apply
**Source**: `docs/audits/AUDIT_SAVE_2026-07-16.md`

## Location
`byroredux/src/save_io.rs:288-338` (`apply_player_pose`); interacts with `byroredux/src/systems/character.rs:358-454` (`camera_follow_system`)

## Description
The branch selection is gated on `pose.character_mode && character_now` — it only drives the player body when *both* the save-time and live mode are Character. When the save was captured in FlyCam mode but the live session is currently in Character mode, the fallback repositions only the `ActiveCamera` entity's `Transform` — it never touches the body. `camera_follow_system` (Stage::Late, every frame while `PlayerMode::Character`) unconditionally re-derives the camera position from the body's `GlobalTransform` + eye height with no awareness a pose was just restored, so the restored vantage is visible for exactly one frame and then silently overwritten. Same mechanism as the closed `#1874` (door-transition camera reversion), never patched for this path.

Verified current: `apply_player_pose` still gates the body-driving branch on `pose.character_mode && character_now`, falling back to camera-only repositioning (`crate::cell_loader::reposition_camera`) otherwise.

## Evidence
```rust
// save_io.rs — only drives the body when BOTH modes match
if pose.character_mode && character_now {
    if let Some(body) = body { /* ... */ return; }
}
// otherwise: camera-only fallback, body untouched
crate::cell_loader::reposition_camera(world, pos, rot);
```

## Impact
A FlyCam-mode save reloaded into a live Character-mode session restores look direction (`InputState` yaw/pitch) but not position — the camera snaps back to wherever the untouched body sits, one frame later. No test exercises this saved/live mode mismatch (`player_pose_round_trips_flycam` and `player_pose_character_tracks_body` both keep modes matched).

## Related
Closed `#1874` (same mechanism, different trigger site — cell transition, not load).

## Suggested Fix
In the fallback branch, when `character_now` is true and a body exists, also relocate the body (mirroring `snap_character_body_to_camera`, camera→body direction instead of body→camera) so `camera_follow_system` re-derives the same restored position every subsequent frame. Simplest form: branch on `character_now` alone.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
