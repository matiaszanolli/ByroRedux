//! Water submersion detection.

use byroredux_core::ecs::components::water::{
    SubmersionState, WaterMaterial, WaterPlane, WaterVolume,
};
use byroredux_core::ecs::{ActiveCamera, GlobalTransform, World};

/// Hysteresis band half-width for the `head_submerged` boolean, in
/// Bethesda world units (~1.43 cm/unit — same scale as `WaterVolume`
/// min/max). Small enough to be visually imperceptible (~5.7 cm), large
/// enough to swamp sub-frame camera/float dither when the eye is parked
/// exactly at the waterline. Tunable; the only constraint is that the
/// vertical AABB acceptance below is extended by the *same* constant so
/// the exit transition fires precisely at the band edge (#1450 / WAT-01).
const WATERLINE_HYSTERESIS: f32 = 4.0;

/// Resolve the sticky `head_submerged` flag with a hysteresis band.
///
/// `depth` is `Some(surface_y - eye_y)` when the eye is inside a water
/// column (whose acceptance extends [`WATERLINE_HYSTERESIS`] above the
/// surface, so `depth` can read slightly negative), or `None` when the
/// eye is outside every volume. Enter submerged only once the eye is a
/// full band below the surface (`depth > +eps`); once submerged, stay
/// submerged until the eye leaves the column entirely (`None`) or rises
/// a full band above it (`depth < -eps`). Outside any column is always
/// dry. This prevents the underwater-FX boolean from strobing when the
/// camera dithers across `depth == 0` at the waterline (#1450).
fn resolve_head_submerged(was: bool, depth: Option<f32>) -> bool {
    match depth {
        Some(d) if was => d > -WATERLINE_HYSTERESIS,
        Some(d) => d > WATERLINE_HYSTERESIS,
        None => false,
    }
}

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
    let mut best: Option<(f32, WaterMaterial)> = None;
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
        // The upper-Y bound is relaxed by `WATERLINE_HYSTERESIS` so a
        // camera up to one band *above* the surface still produces a
        // candidate (with slightly negative depth). That is what lets
        // the sticky `head_submerged` flag hold through the band rather
        // than dropping to the no-candidate (`None` → dry) path the
        // instant the eye crosses the surface — see `resolve_head_submerged`.
        if cam_pos.x < volume.min[0]
            || cam_pos.x > volume.max[0]
            || cam_pos.y < volume.min[1]
            || cam_pos.y > volume.max[1] + WATERLINE_HYSTERESIS
            || cam_pos.z < volume.min[2]
            || cam_pos.z > volume.max[2]
        {
            continue;
        }
        // Surface is at volume.max.y. With the band-extended upper
        // bound above, `depth` ranges in (-WATERLINE_HYSTERESIS, ...];
        // the hysteresis resolver handles the near-surface sign.
        let surface_y = volume.max[1];
        let depth = surface_y - cam_pos.y;
        // Pick the closest match (smallest depth wins — for nested
        // / overlapping water volumes, the one closest to the camera
        // controls the underwater FX).
        let candidate = (depth, plane.material);
        match best {
            None => best = Some(candidate),
            Some((prev_depth, _)) if depth < prev_depth => best = Some(candidate),
            _ => {}
        }
    }
    drop(wq);
    drop(vq);

    let best_depth = best.as_ref().map(|(d, _)| *d);
    let Some(mut sq) = world.query_mut::<SubmersionState>() else {
        return;
    };
    // `SubmersionState` is inserted on the camera entity at setup
    // time (see scene.rs camera spawn). If the component is somehow
    // missing, skip silently — structural inserts mid-frame would
    // require `&mut World` and we keep this system on the pure-
    // mutation path with the rest of the per-frame systems.
    if let Some(state) = sq.get_mut(cam_entity) {
        // Resolve the sticky `head_submerged` flag against the previous
        // frame's value so the boolean doesn't strobe at the waterline
        // (#1450). `depth` carries through unchanged for the fog path,
        // which self-fades to zero as `depth → 0` regardless.
        let was = state.head_submerged;
        let head_submerged = resolve_head_submerged(was, best_depth);
        let new_state = match best {
            Some((depth, material)) => SubmersionState {
                depth,
                head_submerged,
                material: Some(material),
            },
            None => SubmersionState::default(),
        };
        // One-time-per-transition log. Catches the "everything
        // underwater" failure mode where a misplaced water plane
        // flags the camera as submerged on cells where the player
        // is clearly above ground. Logs at INFO so it's visible
        // without raising the global log level.
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

/// Pack the active camera's submersion state into the
/// `[deep_color.rgb, depth]` vec4 the renderer consumes as
/// underwater fog parameters. Returns `[0; 4]` when the camera is
/// out of water (or no active camera). Moved out of `main.rs`
/// under TD9-NEW-01 / #1267 to keep the binary entry file below
/// the 2000-LOC ceiling.
pub fn compute_underwater_params(world: &World) -> [f32; 4] {
    let active = world.try_resource::<ActiveCamera>().map(|a| a.0);
    let Some(cam_entity) = active else {
        return [0.0; 4];
    };
    let Some(sq) = world.query::<SubmersionState>() else {
        return [0.0; 4];
    };
    let Some(state) = sq.get(cam_entity) else {
        return [0.0; 4];
    };
    if !state.head_submerged || state.depth <= 0.0 {
        return [0.0; 4];
    }
    let Some(mat) = state.material.as_ref() else {
        return [0.0; 4];
    };
    [
        mat.deep_color[0],
        mat.deep_color[1],
        mat.deep_color[2],
        state.depth,
    ]
}

#[cfg(test)]
mod tests {
    use super::{resolve_head_submerged, WATERLINE_HYSTERESIS};

    const EPS: f32 = WATERLINE_HYSTERESIS;

    #[test]
    fn outside_any_volume_is_always_dry() {
        // No candidate → dry regardless of previous state. A camera that
        // leaves the water column entirely must always surface.
        assert!(!resolve_head_submerged(true, None));
        assert!(!resolve_head_submerged(false, None));
    }

    #[test]
    fn dry_requires_full_band_below_surface_to_enter() {
        // Within the band below the surface, a dry camera stays dry.
        assert!(!resolve_head_submerged(false, Some(0.0)));
        assert!(!resolve_head_submerged(false, Some(EPS * 0.5)));
        assert!(!resolve_head_submerged(false, Some(EPS))); // strict `>`
        // Past the band, it submerges.
        assert!(resolve_head_submerged(false, Some(EPS + 0.1)));
        assert!(resolve_head_submerged(false, Some(1000.0)));
    }

    #[test]
    fn submerged_stays_submerged_across_the_waterline() {
        // The dithering-at-the-waterline case the issue describes: a
        // submerged camera holds through `depth == 0` and into the band
        // above the surface (negative depth), instead of strobing.
        assert!(resolve_head_submerged(true, Some(0.1)));
        assert!(resolve_head_submerged(true, Some(0.0)));
        assert!(resolve_head_submerged(true, Some(-EPS * 0.5)));
    }

    #[test]
    fn hysteresis_band_is_non_degenerate() {
        // The enter threshold (+eps) sits strictly above the exit
        // threshold (-eps): a depth inside the band keeps whatever state
        // it had, which is the whole point of the band.
        let mid = 0.0;
        assert!(!resolve_head_submerged(false, Some(mid))); // dry stays dry
        assert!(resolve_head_submerged(true, Some(mid))); // wet stays wet
        assert!(EPS > 0.0);
    }
}
