//! Water submersion detection.

use byroredux_core::ecs::components::water::{SubmersionState, WaterPlane, WaterVolume};
use byroredux_core::ecs::{ActiveCamera, GlobalTransform, World};

/// Submersion detection — write `SubmersionState` onto the active
/// camera entity each frame.
///
/// Tests every `WaterPlane` entity in the world against the camera's
/// world position; when the camera falls inside a [`WaterVolume`]'s
/// horizontal extent and below the plane's surface height, the
/// computed depth + selected material are written through.
///
/// MVP scope:
///
/// - Only the active camera receives `SubmersionState`. Actors are a
///   follow-up once the actor controller lands (gameplay layer
///   reads `head_submerged` to switch to swim state).
/// - Linear scan over `WaterPlane` entities. Cells ship 1–3 water
///   planes max; a broadphase would only matter once we hit
///   dozens.
/// - `head_submerged` is computed at zero offset for cameras (the
///   eye is the submerged surface). The component still carries the
///   bool for downstream uniformity with the actor path.
pub(crate) fn submersion_system(world: &World, _dt: f32) {
    let Some(active) = world.try_resource::<ActiveCamera>() else {
        return;
    };
    let cam_entity = active.0;
    drop(active);

    let Some(gq) = world.query::<GlobalTransform>() else {
        return;
    };
    let Some(cam_global) = gq.get(cam_entity).copied() else {
        return;
    };
    let cam_pos = cam_global.translation;
    drop(gq);

    // Snapshot every active water plane's volume + material. We
    // re-acquire GlobalTransform here only to confirm the plane's
    // world Y matches its volume `max.y` (defensive — `WaterVolume`
    // is authored at spawn time so the two should already agree).
    let mut best: Option<(f32, SubmersionState)> = None;
    let Some(wq) = world.query::<WaterPlane>() else {
        // No water entities at all → clear any prior state on the
        // camera so the next frame's render reads default-above-water.
        if let Some(mut sq) = world.query_mut::<SubmersionState>() {
            if let Some(state) = sq.get_mut(cam_entity) {
                *state = SubmersionState::default();
            }
        }
        return;
    };
    let Some(vq) = world.query::<WaterVolume>() else {
        return;
    };
    for (entity, plane) in wq.iter() {
        let Some(volume) = vq.get(entity) else {
            continue;
        };
        // Full 3-D AABB containment. The previous version checked
        // only the horizontal extent + a "below the surface"
        // condition, which mis-flagged cameras that sat far below
        // a water plane (e.g., outdoor cell with a tiny pond plane
        // authored high above some other piece of terrain the
        // camera happens to share an XZ column with). Requiring
        // `cam_pos.y >= volume.min.y` rejects those — to be
        // underwater you must be inside the actual water column.
        if cam_pos.x < volume.min[0]
            || cam_pos.x > volume.max[0]
            || cam_pos.y < volume.min[1]
            || cam_pos.y > volume.max[1]
            || cam_pos.z < volume.min[2]
            || cam_pos.z > volume.max[2]
        {
            continue;
        }
        // Surface is at volume.max.y; the AABB pre-test already
        // ensured cam_pos.y ≤ surface_y, so depth is always ≥ 0
        // here. No further sign check needed.
        let surface_y = volume.max[1];
        let depth = surface_y - cam_pos.y;
        // Pick the closest match (smallest depth wins — for nested
        // / overlapping water volumes, the one closest to the camera
        // controls the underwater FX).
        let candidate = (
            depth,
            SubmersionState {
                depth,
                head_submerged: depth > 0.0,
                material: Some(plane.material),
            },
        );
        match best {
            None => best = Some(candidate),
            Some((prev_depth, _)) if depth < prev_depth => best = Some(candidate),
            _ => {}
        }
    }
    drop(wq);
    drop(vq);

    let new_state = best.map(|(_, s)| s).unwrap_or_default();
    let Some(mut sq) = world.query_mut::<SubmersionState>() else {
        return;
    };
    // `SubmersionState` is inserted on the camera entity at setup
    // time (see scene.rs camera spawn). If the component is somehow
    // missing, skip silently — structural inserts mid-frame would
    // require `&mut World` and we keep this system on the pure-
    // mutation path with the rest of the per-frame systems.
    if let Some(state) = sq.get_mut(cam_entity) {
        // One-time-per-transition log. Catches the "everything
        // underwater" failure mode where a misplaced water plane
        // flags the camera as submerged on cells where the player
        // is clearly above ground. Logs at INFO so it's visible
        // without raising the global log level.
        let was = state.head_submerged;
        let now = new_state.head_submerged;
        if was != now {
            if now {
                log::info!(
                    "submersion: ENTER underwater — depth={:.1} cam=({:.1}, {:.1}, {:.1})",
                    new_state.depth,
                    cam_pos.x,
                    cam_pos.y,
                    cam_pos.z,
                );
            } else {
                log::info!(
                    "submersion: EXIT underwater — cam=({:.1}, {:.1}, {:.1})",
                    cam_pos.x,
                    cam_pos.y,
                    cam_pos.z,
                );
            }
        }
        *state = new_state;
    }
}
