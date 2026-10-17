//! Player pose capture/restore + the deferred player save-action
//! queue (#5585, split from `save_io.rs`).
//!
//! [`capture_player_pose`] refreshes the [`PlayerPose`] resource each
//! frame so a save records where the player was; [`apply_player_pose`]
//! re-places body or camera after a live load. Input adapters enqueue
//! [`PlayerSaveAction`]s instead of entering the registry lock surface.

use super::*;

/// The player's standing position + look direction at save time, so a
/// live `load` can put the player back where they were rather than at the
/// reloaded cell's default door spawn.
///
/// `position` is the **engine Y-up world position** of the body in
/// Character mode (the camera is re-pinned to body + eye-height the next
/// frame by `camera_follow_system`) or of the camera itself in FlyCam
/// mode. `yaw`/`pitch` are the [`InputState`](crate::components::InputState)
/// look angles — the source of truth in *both* modes, since both camera
/// systems rebuild the camera rotation from them every frame, so a saved
/// `Transform.rotation` alone wouldn't survive a tick.
///
/// Refreshed every frame by [`capture_player_pose`]; registered as a save
/// resource so it rides along in the snapshot; re-applied on load by
/// [`apply_player_pose`].
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
pub struct PlayerPose {
    pub position: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    /// `true` when captured in Character mode (body-driven), `false` for
    /// FlyCam — tells the restore which entity the `position` refers to.
    pub character_mode: bool,
}

/// A player-facing save/load request that must execute only after the frame's
/// scheduler has joined all parallel systems.
///
/// Input adapters (keyboard, native menus, and future SDK hosts) enqueue this
/// small value instead of entering the save registry's wide lock surface
/// directly. [`execute_pending_player_save_actions`] is the single executor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerSaveAction {
    Quicksave,
    Quickload,
}

impl PlayerSaveAction {
    pub(crate) fn context(self) -> &'static str {
        match self {
            Self::Quicksave => "player quicksave",
            Self::Quickload => "player quickload",
        }
    }
}

/// FIFO ingress for player save/load requests.
///
/// This is intentionally transient process state, not part of a save file.
#[derive(Default)]
pub struct PendingPlayerSaveActions(pub(crate) Vec<PlayerSaveAction>);

impl Resource for PendingPlayerSaveActions {}

impl PendingPlayerSaveActions {
    /// Read-only view of the still-deferred actions, so callers (and tests)
    /// can observe the queue without joining the executor's take semantics.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn queued(&self) -> &[PlayerSaveAction] {
        &self.0
    }
}

/// Defer a player save/load request to the post-scheduler frame boundary.
///
/// Returning an error when boot has not installed the queue makes this usable
/// by embedders without hiding a partially initialized SDK/runtime.
pub fn queue_player_save_action(
    world: &World,
    action: PlayerSaveAction,
) -> Result<(), &'static str> {
    let Some(mut pending) = world.try_resource_mut::<PendingPlayerSaveActions>() else {
        return Err("player save-action queue not installed");
    };
    pending.0.push(action);
    Ok(())
}

impl Resource for PlayerPose {}

/// Pull the saved cell context out of a decoded snapshot, if present.
///
/// Returns `None` for saves taken outside an interior cell (loose-NIF /
/// exterior modes never set `CurrentCellContext`).
pub fn snapshot_cell_context(
    snap: &byroredux_save::Snapshot,
) -> Option<crate::cell_loader::CurrentCellContext> {
    snap.resources
        .get("CurrentCellContext")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

/// Pull the saved exterior-streaming context out of a decoded snapshot, if
/// present. Returns `None` for saves taken outside exterior streaming
/// (interior / loose-NIF modes never set `CurrentExteriorContext`) — the
/// exterior counterpart of [`snapshot_cell_context`], EX-09/17 item 4.
pub fn snapshot_exterior_context(
    snap: &byroredux_save::Snapshot,
) -> Option<crate::cell_loader::CurrentExteriorContext> {
    snap.resources
        .get("CurrentExteriorContext")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

/// Pull the saved [`PlayerPose`] out of a decoded snapshot, if present.
///
/// Absent for pre-refinement saves (schema-fingerprint drift would reject
/// those before this is reached anyway) and for any snapshot taken with no
/// `PlayerPose` resource installed.
pub fn snapshot_player_pose(snap: &Snapshot) -> Option<PlayerPose> {
    snap.resources
        .get("PlayerPose")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

/// Refresh the [`PlayerPose`] resource from the live player each frame.
///
/// Called App-side in the post-scheduler phase (after the camera systems
/// have published this frame's pose), so it reads `&World` and writes the
/// resource through interior mutability — no scheduler-access declaration
/// needed. No-op until the [`PlayerPose`] resource is installed; leaves the
/// last good pose untouched if the position source can't be resolved (e.g.
/// the camera entity has no `Transform` yet).
pub fn capture_player_pose(world: &World) {
    use byroredux_core::ecs::{ActiveCamera, Transform};

    let Some(mut pose) = world.try_resource_mut::<PlayerPose>() else {
        return;
    };
    let character_mode = world
        .try_resource::<crate::systems::PlayerMode>()
        .map(|m| *m == crate::systems::PlayerMode::Character)
        .unwrap_or(false);
    let (yaw, pitch) = world
        .try_resource::<crate::components::InputState>()
        .map(|i| (i.yaw, i.pitch))
        .unwrap_or((0.0, 0.0));

    // Position source: the body in Character mode, the camera in FlyCam.
    let target = if character_mode {
        world
            .try_resource::<crate::systems::PlayerEntity>()
            .and_then(|r| r.0)
    } else {
        world.try_resource::<ActiveCamera>().map(|a| a.0)
    };
    let pos = target.and_then(|e| {
        world
            .query::<Transform>()
            .and_then(|q| q.get(e).map(|t| t.translation))
    });

    if let Some(pos) = pos {
        pose.position = pos.to_array();
        pose.yaw = yaw;
        pose.pitch = pitch;
        pose.character_mode = character_mode;
    }
}

/// Re-place the player at a saved [`PlayerPose`] after a live load.
///
/// `yaw`/`pitch` go onto [`InputState`](crate::components::InputState) in
/// both modes — that's what the camera systems read to rebuild the camera
/// rotation. The position is applied to whichever entity the active mode
/// drives: the persisted body (Character — `camera_follow_system` re-pins
/// the camera next frame) or the camera directly (FlyCam). Falls back to
/// the camera when Character mode was saved but no player body is live
/// (e.g. a `--fly` reload), so the look direction is at least honoured.
///
/// #5530 — the pose comes from a save file, so it is validated first: a
/// non-finite or world-bound-exceeding position (the physics sanity bound,
/// [`byroredux_physics::PhysicsWorld::translation_is_sane`]) or a non-finite
/// look angle is refused whole, logged, and the player stays where the
/// reloaded cell spawned them. Returns whether the pose was applied.
pub fn apply_player_pose(world: &mut World, pose: &PlayerPose) -> bool {
    let position = Vec3::from_array(pose.position);
    if !byroredux_physics::PhysicsWorld::translation_is_sane(position)
        || !pose.yaw.is_finite()
        || !pose.pitch.is_finite()
    {
        log::error!(
            "save load: refusing a corrupt player pose (position {:?}, yaw {}, pitch {}); \
             the player keeps the cell's spawn point (#5530)",
            pose.position,
            pose.yaw,
            pose.pitch,
        );
        return false;
    }
    if let Some(mut input) = world.try_resource_mut::<crate::components::InputState>() {
        input.yaw = pose.yaw;
        input.pitch = pose.pitch;
    }

    let pos = position;
    let body = world
        .try_resource::<crate::systems::PlayerEntity>()
        .and_then(|r| r.0);
    let character_now = world
        .try_resource::<crate::systems::PlayerMode>()
        .map(|m| *m == crate::systems::PlayerMode::Character)
        .unwrap_or(false);

    // #2018 / SAVE-D6-03 — drive the body whenever the LIVE session is in
    // Character mode, regardless of which mode the pose was SAVED in.
    // Pre-fix this gated on `pose.character_mode && character_now`, so a
    // FlyCam-saved pose reloaded into a live Character session fell to the
    // camera-only branch below, leaving the body untouched;
    // `camera_follow_system` (Stage::Late, every frame while
    // `PlayerMode::Character`) unconditionally re-derives the camera
    // position from the body's `GlobalTransform` + eye height with no
    // awareness a pose was just restored, so the camera-only fix was
    // visible for exactly one frame before being silently overwritten —
    // same mechanism as the door-transition case `#1874` fixed.
    if character_now {
        if let Some(body) = body {
            // `pos` is the saved *camera* position when the pose was
            // captured in FlyCam mode (see `capture_player_pose`'s
            // `target` selection) — convert it to the body's feet position
            // the same way `snap_character_body_to_camera` does
            // (`cam_pos - eye_height` on Y), so `camera_follow_system`
            // re-derives the identical restored vantage every subsequent
            // frame instead of `body_pos.y + eye_height` landing one
            // `eye_height` too high. A Character-saved pose already IS the
            // body position, so it's used verbatim.
            let body_pos = if pose.character_mode {
                pos
            } else {
                let eye_height = world
                    .query::<byroredux_physics::CharacterController>()
                    .and_then(|q| q.get(body).map(|c| c.eye_height))
                    .unwrap_or(52.0);
                pos - Vec3::Y * eye_height
            };
            // Momentum is cleared inside, so the body carries no stale free-fall
            // velocity into the reloaded cell; the kinematic Rapier body is synced
            // so the KCC's next collide-and-slide starts from the restored spot.
            crate::systems::place_character_body(world, body, body_pos, None);
            return true;
        }
    }

    // FlyCam, or Character-saved with no live body: drop the camera at the
    // saved spot with a yaw/pitch-derived rotation.
    let rot = crate::systems::camera_look_rotation(pose.yaw, pose.pitch);
    crate::cell_loader::reposition_camera(world, pos, rot);
    true
}
