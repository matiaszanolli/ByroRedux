//! Water submersion detection.

use byroredux_core::ecs::components::water::{
    SubmersionState, WaterMaterial, WaterPlane, WaterVolume,
};
use byroredux_core::ecs::components::ParticleEmitter;
use byroredux_core::ecs::{ActiveCamera, GlobalTransform, World};
use byroredux_scripting::{RippleEvent, SplashEvent};

/// Hysteresis band half-width for the `head_submerged` boolean, in
/// Bethesda world units (~1.43 cm/unit — same scale as `WaterVolume`
/// min/max). Small enough to be visually imperceptible (~5.7 cm), large
/// enough to swamp sub-frame camera/float dither when the eye is parked
/// exactly at the waterline. Tunable; the only constraint is that the
/// vertical AABB acceptance below is extended by the *same* constant so
/// the exit transition fires precisely at the band edge (#1450 / WAT-01).
const WATERLINE_HYSTERESIS: f32 = 4.0;
const DISTURBANCE_BAND: f32 = 24.0;
const DISTURBANCE_RADIUS: f32 = 18.0;

fn disturbance_rate(cam_pos: byroredux_core::math::Vec3, volume: &WaterVolume) -> f32 {
    let surface_y = volume.max[1];
    let vertical = (surface_y - cam_pos.y).abs();
    if vertical > DISTURBANCE_BAND {
        return 0.0;
    }
    let clamped_x = cam_pos.x.clamp(volume.min[0], volume.max[0]);
    let clamped_z = cam_pos.z.clamp(volume.min[2], volume.max[2]);
    let dx = cam_pos.x - clamped_x;
    let dz = cam_pos.z - clamped_z;
    if dx * dx + dz * dz > DISTURBANCE_RADIUS * DISTURBANCE_RADIUS {
        0.0
    } else {
        14.0 * (1.0 - vertical / DISTURBANCE_BAND)
    }
}

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
        // #2792 (REN-D15-09) — mirror the WaterPlane-absent branch above:
        // no `WaterVolume` component anywhere means no water can submerge
        // the camera this frame, so any stale `head_submerged` / `material`
        // from a prior frame must be cleared, not left to feed
        // `compute_underwater_params` indefinitely. Today `WaterPlane`
        // always ships with a matching `WaterVolume` except transiently via
        // `spawn_lod_water_plane` (#2449), a state the camera can't already
        // be submerged in — but that's an invariant of the current spawn
        // path, not something this function should assume holds forever.
        if let Some(mut sq) = world.query_mut::<SubmersionState>() {
            if let Some(state) = sq.get_mut(cam_entity) {
                *state = SubmersionState::default();
            }
        }
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

    // Drive the resident water-spray emitters from the camera footprint.
    // Water planes are spawned with a dormant emitter, so this remains a
    // component-only mutation and cannot invalidate the plane query above.
    let mut disturbance_events = Vec::new();
    if let Some((volume_q, mut emitter_q)) = world.query_2_mut::<WaterVolume, ParticleEmitter>() {
        for (entity, volume) in volume_q.iter() {
            if let Some(emitter) = emitter_q.get_mut(entity) {
                let previous = emitter.rate;
                let rate = disturbance_rate(cam_pos, volume);
                emitter.rate = rate;
                if rate > 0.0 {
                    disturbance_events.push((
                        entity,
                        previous <= 0.0,
                        (rate / 14.0).clamp(0.0, 1.0),
                        [cam_pos.x, volume.max[1], cam_pos.z],
                    ));
                }
            }
        }
    }
    // Publish the same interaction to gameplay/audio consumers as transient
    // ECS markers. The particle emitter remains the presentation path; these
    // markers make the disturbance useful to systems that should not inspect
    // renderer state directly.
    if !disturbance_events.is_empty() {
        if let Some(mut q) = world.query_mut::<RippleEvent>() {
            for &(entity, _, intensity, position) in &disturbance_events {
                q.insert(
                    entity,
                    RippleEvent {
                        actor: cam_entity,
                        intensity,
                        position,
                    },
                );
            }
        }
        if let Some(mut q) = world.query_mut::<SplashEvent>() {
            for &(entity, entering, intensity, position) in &disturbance_events {
                if entering {
                    q.insert(
                        entity,
                        SplashEvent {
                            actor: cam_entity,
                            intensity,
                            position,
                        },
                    );
                }
            }
        }
    }

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
/// `[deep_color.rgb, extinction]` vec4 the renderer consumes as
/// underwater fog parameters. Extinction is evaluated from the authored
/// water fog range, so each game's WATR material controls its underwater
/// transition rather than a presentation-time fixed distance. Returns `[0; 4]` when the camera is
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
    let (fog_near, fog_far) = if mat.underwater_fog_far > mat.underwater_fog_near {
        (mat.underwater_fog_near, mat.underwater_fog_far)
    } else {
        (mat.fog_near, mat.fog_far)
    };
    let extinction = underwater_extinction(state.depth, fog_near, fog_far);
    [
        mat.deep_color[0],
        mat.deep_color[1],
        mat.deep_color[2],
        extinction,
    ]
}

#[inline]
fn underwater_extinction(depth: f32, fog_near: f32, fog_far: f32) -> f32 {
    let span = (fog_far - fog_near).max(1.0);
    let ramp = ((depth - fog_near) / span).clamp(0.0, 1.0);
    1.0 - (-ramp * 2.0).exp()
}

#[cfg(test)]
mod tests {
    use super::{
        disturbance_rate, resolve_head_submerged, submersion_system, underwater_extinction,
        DISTURBANCE_BAND, DISTURBANCE_RADIUS, WATERLINE_HYSTERESIS,
    };
    use byroredux_core::ecs::components::water::{
        SubmersionState, WaterKind, WaterMaterial, WaterPlane, WaterVolume,
    };
    use byroredux_core::ecs::components::ParticleEmitter;
    use byroredux_core::ecs::{ActiveCamera, GlobalTransform, World};
    use byroredux_core::math::Vec3;
    use byroredux_scripting::{RippleEvent, SplashEvent};

    const EPS: f32 = WATERLINE_HYSTERESIS;

    #[test]
    fn underwater_extinction_respects_authored_fog_range() {
        assert_eq!(underwater_extinction(20.0, 80.0, 600.0), 0.0);
        let near = underwater_extinction(180.0, 80.0, 600.0);
        let wide = underwater_extinction(180.0, 0.0, 1200.0);
        assert!(
            near > wide,
            "shorter authored fog range should absorb faster"
        );
        assert!(underwater_extinction(600.0, 80.0, 600.0) > 0.8);
    }

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
        assert!(std::hint::black_box(EPS) > 0.0);
    }

    #[test]
    fn disturbance_rate_is_localized_to_surface_footprint() {
        let volume = WaterVolume {
            min: [-100.0, -200.0, -100.0],
            max: [100.0, 0.0, 100.0],
        };
        assert!(disturbance_rate(Vec3::new(0.0, 0.0, 0.0), &volume) > 0.0);
        assert_eq!(
            disturbance_rate(Vec3::new(0.0, DISTURBANCE_BAND + 1.0, 0.0), &volume),
            0.0
        );
        assert_eq!(
            disturbance_rate(
                Vec3::new(volume.max[0] + DISTURBANCE_RADIUS + 1.0, 0.0, 0.0),
                &volume
            ),
            0.0
        );
    }

    #[test]
    fn surface_interaction_emits_transient_splash_and_ripple_markers() {
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        let camera = world.spawn();
        world.insert_resource(ActiveCamera(camera));
        world.insert(
            camera,
            GlobalTransform::new(
                Vec3::new(0.0, 0.0, 0.0),
                byroredux_core::math::Quat::IDENTITY,
                1.0,
            ),
        );
        world.insert(camera, SubmersionState::default());
        let water = world.spawn();
        world.insert(
            water,
            WaterPlane {
                kind: WaterKind::Calm,
                material: WaterMaterial::default(),
            },
        );
        world.insert(
            water,
            WaterVolume {
                min: [-50.0, -100.0, -50.0],
                max: [50.0, 0.0, 50.0],
            },
        );
        world.insert(water, GlobalTransform::IDENTITY);
        world.insert(water, ParticleEmitter::water_splash());

        submersion_system(&world, 0.016);

        let ripples = world.query::<RippleEvent>().expect("ripple storage");
        let splashes = world.query::<SplashEvent>().expect("splash storage");
        assert!(ripples.get(water).is_some());
        assert!(splashes.get(water).is_some());
    }

    /// Regression for #2792 (REN-D15-09): a `WaterPlane` entity with no
    /// matching `WaterVolume` component must clear stale submersion state
    /// exactly like the "no `WaterPlane` at all" branch above it does —
    /// not bare-`return` and leave a prior frame's `head_submerged: true`
    /// / `material` feeding `compute_underwater_params` indefinitely.
    #[test]
    fn water_volume_absent_clears_stale_submersion_state() {
        let mut world = World::new();

        let camera = world.spawn();
        world.insert_resource(ActiveCamera(camera));
        world.insert(camera, GlobalTransform::IDENTITY);
        // Stale state from a prior frame — must be cleared, not preserved.
        world.insert(
            camera,
            SubmersionState {
                depth: 5.0,
                head_submerged: true,
                material: Some(WaterMaterial::default()),
            },
        );

        // A WaterPlane entity exists (so the WaterPlane-absent early
        // return above does NOT fire) but carries no WaterVolume — the
        // transient state `spawn_lod_water_plane` (#2449) produces.
        let water = world.spawn();
        world.insert(
            water,
            WaterPlane {
                kind: WaterKind::River,
                material: WaterMaterial::default(),
            },
        );

        submersion_system(&world, 0.0);

        let sq = world
            .query::<SubmersionState>()
            .expect("SubmersionState query");
        let state = sq.get(camera).copied().expect("camera state");
        assert_eq!(state.depth, 0.0, "depth must reset to default");
        assert!(
            !state.head_submerged,
            "head_submerged must reset to default, not stay stuck true"
        );
        assert!(
            state.material.is_none(),
            "material must reset to default, not keep feeding \
             compute_underwater_params a stale value"
        );
    }
}
