//! Water submersion detection.

use byroredux_core::ecs::components::water::{
    SubmersionState, WaterContact, WaterPlane, WaterVolume, WATERLINE_HYSTERESIS,
};
use byroredux_core::ecs::components::{ActorValues, ActorVitals, Dead, ParticleEmitter};
use byroredux_core::ecs::{ActiveCamera, EntityId, GlobalTransform, World};
use byroredux_core::math::Vec3;
use byroredux_scripting::{RippleEvent, SplashEvent};
use rustc_hash::{FxHashMap, FxHashSet};

const DISTURBANCE_BAND: f32 = 24.0;
const DISTURBANCE_RADIUS: f32 = 18.0;

/// Apply authored FO3/FNV harmful-water damage to actor bodies after the
/// physics phase has refreshed their [`WaterContact`]. The kinematic player
/// has its own controller path; this covers dynamic NPC/creature bodies
/// without coupling the physics crate to actor gameplay components.
pub(crate) fn water_damage_system(world: &World, dt: f32) {
    if dt <= 0.0 {
        return;
    }
    let Some(contact_q) = world.query::<WaterContact>() else {
        return;
    };
    let Some(vitals_q) = world.query::<ActorVitals>() else {
        return;
    };
    let dead_q = world.query::<Dead>();
    let targets: Vec<(EntityId, u32, f32)> = contact_q
        .iter()
        .filter_map(|(entity, contact)| {
            let damage = contact.damage_per_second.max(0.0)
                * contact.submerged_fraction.clamp(0.0, 1.0)
                * dt;
            if damage <= 0.0 || dead_q.as_ref().is_some_and(|q| q.get(entity).is_some()) {
                return None;
            }
            Some((entity, vitals_q.get(entity)?.health, damage))
        })
        .collect();
    drop(contact_q);
    drop(vitals_q);
    drop(dead_q);

    for (entity, health, damage) in targets {
        let Some(mut values_q) = world.query_mut::<ActorValues>() else {
            continue;
        };
        let Some(values) = values_q.get_mut(entity) else {
            continue;
        };
        values.apply_damage(health, damage);
        let killed = values.current(health) <= 0.0;
        drop(values_q);
        if killed {
            if let Some(mut dead_q) = world.query_mut::<Dead>() {
                dead_q.insert(entity, Dead);
            }
            crate::combat::queue_dead_actor_reconciliation(world, entity);
        }
    }
}

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
///
/// # Scheduling requirement
///
/// Must run **after** `camera_follow_system`, which authors the camera pose in
/// `Stage::Late` in player / third-person mode (writing both `Transform` and
/// `GlobalTransform` directly, bypassing the missing late-stage propagation
/// pass). Registered as a `Stage::Late` exclusive so that holds in every
/// camera mode; a `Stage::PostUpdate` registration reads the *previous*
/// frame's pose outside fly-cam mode, which lags the underwater low-pass and
/// composite tint by a frame (#3180). It must also stay ahead of
/// `water_audio_system`, which consumes the `SubmersionState` and the
/// Splash/Ripple markers written here.
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
    let mut best: Option<(f32, f32, EntityId)> = None;
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
    let wave_adjustment = world
        .try_resource::<byroredux_core::ecs::resources::TotalTime>()
        .map(|time| {
            (
                time.0,
                byroredux_physics::weather_wave_adjustment(world, time.0),
            )
        });
    for (entity, plane) in wq.iter() {
        let Some(volume) = vq.get(entity) else {
            continue;
        };
        let surface_y = volume.max[1]
            + wave_adjustment
                .map(|(time, (weather_scroll, wind_wave_scale))| {
                    byroredux_physics::authored_wave_height_with_weather(
                        &plane.material,
                        cam_pos,
                        time,
                        weather_scroll,
                        wind_wave_scale,
                    )
                })
                .unwrap_or(0.0);
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
            || cam_pos.y > surface_y + WATERLINE_HYSTERESIS
            || cam_pos.z < volume.min[2]
            || cam_pos.z > volume.max[2]
        {
            continue;
        }
        // Surface is the authored volume height plus the bounded rendered
        // wave. With the band-extended upper bound above, `depth` ranges in
        // (-WATERLINE_HYSTERESIS, ...]; the hysteresis resolver handles the
        // near-surface sign.
        let depth = surface_y - cam_pos.y;
        let distance = byroredux_physics::nearest_surface_distance(surface_y, cam_pos.y);
        // Pick the closest match by absolute vertical distance. During cell
        // transitions an upper and lower water volume can overlap in XZ; a
        // signed-depth comparison would incorrectly prefer the lowest
        // surface whenever the camera is above both.
        let candidate = (depth, distance, entity);
        match best {
            None => best = Some(candidate),
            Some((_, prev_distance, _)) if distance < prev_distance => best = Some(candidate),
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

    let best_depth = best.as_ref().map(|(depth, _, _)| *depth);
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
            Some((depth, _, surface_entity)) => SubmersionState {
                depth,
                head_submerged,
                surface_entity: Some(surface_entity),
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

/// Emit surface interaction markers for dynamic physics bodies.
///
/// The physics crate deliberately owns only canonical `WaterContact` state;
/// it does not depend on scripting or presentation. This bridge runs after
/// the physics step and turns dry→wet transitions into `SplashEvent`s while
/// keeping a one-frame `RippleEvent` alive for every wet body. The active
/// camera already has its own volume-based path above, so this covers thrown
/// clutter, ragdolls, creatures, and any other dynamic body uniformly.
pub(crate) fn make_water_interaction_system() -> impl FnMut(&World, f32) + Send + Sync {
    let mut wet_last_frame = FxHashSet::<EntityId>::default();
    let mut wet_now = FxHashSet::<EntityId>::default();
    let mut entries = Vec::new();
    let mut ripple_by_surface = FxHashMap::<EntityId, (EntityId, Vec3, f32)>::default();
    let mut splash_by_surface = FxHashMap::<EntityId, (EntityId, Vec3, f32)>::default();

    move |world: &World, _dt: f32| {
        let Some(contact_q) = world.query::<WaterContact>() else {
            wet_last_frame.clear();
            return;
        };
        let Some(global_q) = world.query::<GlobalTransform>() else {
            return;
        };

        wet_now.clear();
        entries.clear();
        ripple_by_surface.clear();
        splash_by_surface.clear();
        for (entity, contact) in contact_q.iter() {
            if contact.submerged_fraction <= 0.0 {
                continue;
            }
            let Some(global) = global_q.get(entity) else {
                continue;
            };
            // `GlobalTransform` is the body's centre; place the surface event
            // at the waterline so a deeply submerged body does not sound as
            // though the splash came from below the surface.
            let mut position = global.translation;
            position.y += contact.depth;
            let intensity = (0.35 + contact.submerged_fraction * 0.65).clamp(0.0, 1.0);
            let Some(surface_entity) = contact.surface_entity else {
                continue;
            };
            entries.push((
                entity,
                surface_entity,
                position,
                intensity,
                !wet_last_frame.contains(&entity),
            ));
            if contact.submerged_fraction < 0.98 {
                ripple_by_surface
                    .entry(surface_entity)
                    .and_modify(|current| {
                        if intensity > current.2 {
                            *current = (entity, position, intensity);
                        }
                    })
                    .or_insert((entity, position, intensity));
            }
            wet_now.insert(entity);
        }
        drop(global_q);
        drop(contact_q);

        if !entries.is_empty() {
            // #3115 — `SplashEvent`'s documented contract is that the marker
            // lands on the WATER-PLANE entity (`crates/scripting/src/events.rs`),
            // which `submersion_system` honours and this producer did not: it
            // hosted the splash on the BODY while hosting the sibling
            // `RippleEvent` on `surface_entity`, so one function used two host
            // classes and one of them contradicted the doc. Nothing is visibly
            // wrong today only because the sole consumer (`water_audio_system`)
            // reads the payload's own `position` and ignores the host — a
            // scripting/quest consumer attaching to a plane would silently see
            // half the splashes.
            //
            // Deduped strongest-wins per surface, like the ripple path above:
            // two bodies entering one plane in the same frame must not collapse
            // to whichever iterated last.
            for &(entity, surface_entity, position, intensity, entering) in &entries {
                if entering {
                    splash_by_surface
                        .entry(surface_entity)
                        .and_modify(|current| {
                            if intensity > current.2 {
                                *current = (entity, position, intensity);
                            }
                        })
                        .or_insert((entity, position, intensity));
                }
            }

            if let Some(mut q) = world.query_mut::<RippleEvent>() {
                for (&surface_entity, &(entity, position, intensity)) in &ripple_by_surface {
                    // #3115 — `submersion_system` runs earlier in Stage::Late
                    // and may already have written the camera's disturbance on
                    // this same plane. `insert` overwrites, so a dynamic body
                    // sharing the plane used to drop the camera's event without
                    // trace. Keep whichever is stronger.
                    if q.get(surface_entity)
                        .is_some_and(|existing| existing.intensity >= intensity)
                    {
                        continue;
                    }
                    q.insert(
                        surface_entity,
                        RippleEvent {
                            actor: entity,
                            intensity,
                            position: [position.x, position.y, position.z],
                        },
                    );
                }
            }
            if let Some(mut q) = world.query_mut::<SplashEvent>() {
                for (&surface_entity, &(entity, position, intensity)) in &splash_by_surface {
                    if q.get(surface_entity)
                        .is_some_and(|existing| existing.intensity >= intensity)
                    {
                        continue;
                    }
                    q.insert(
                        surface_entity,
                        SplashEvent {
                            actor: entity,
                            intensity,
                            position: [position.x, position.y, position.z],
                        },
                    );
                }
            }
        }
        std::mem::swap(&mut wet_last_frame, &mut wet_now);
        wet_now.clear();
    }
}

/// Pack the active camera's submersion state into the
/// `[underwater_color.rgb, extinction]` vec4 the renderer consumes as
/// underwater fog parameters. Extinction is evaluated from the authored
/// water fog range, so each game's WATR material controls its underwater
/// transition rather than a presentation-time fixed distance. Starfield's
/// non-zero per-channel absorption coefficients additionally attenuate the target
/// tint with Beer–Lambert transmission; the zero triplet preserves the
/// legacy path exactly. Returns `[0; 4]` when the camera is out of water (or
/// no active camera). Moved out of `main.rs`
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
    let Some(state) = sq.get(cam_entity).copied() else {
        return [0.0; 4];
    };
    drop(sq);
    if !state.head_submerged || state.depth <= 0.0 {
        return [0.0; 4];
    }
    let Some(surface_entity) = state.surface_entity else {
        return [0.0; 4];
    };
    let Some(plane_q) = world.query::<WaterPlane>() else {
        return [0.0; 4];
    };
    let Some(plane) = plane_q.get(surface_entity) else {
        return [0.0; 4];
    };
    let mat = &plane.material;
    let (fog_near, fog_far) = if mat.underwater_fog_far > mat.underwater_fog_near {
        (mat.underwater_fog_near, mat.underwater_fog_far)
    } else {
        (mat.fog_near, mat.fog_far)
    };
    let extinction = underwater_extinction(state.depth, fog_near, fog_far)
        * mat.underwater_fog_amount.clamp(0.0, 8.0);
    let underwater_color = underwater_color_at_depth(
        mat.underwater_color,
        state.depth,
        fog_near,
        mat.absorption_coefficients,
    );
    [
        underwater_color[0],
        underwater_color[1],
        underwater_color[2],
        extinction,
    ]
}

/// Apply authored Starfield extinction coefficients to the underwater target
/// tint. A zero triplet is the legacy sentinel and must remain an exact no-op.
/// Beer–Lambert transmission is `exp(-distance * coefficient)` per channel.
/// The near fog plane is clear by definition and therefore does not
/// contribute to this optical path.
#[inline]
fn underwater_color_at_depth(
    color: [f32; 3],
    depth: f32,
    fog_near: f32,
    absorption_coefficients: [f32; 3],
) -> [f32; 3] {
    if !absorption_coefficients
        .iter()
        .any(|coefficient| coefficient.is_finite() && *coefficient > 0.0)
    {
        return color;
    }
    let optical_depth = (depth - fog_near).max(0.0);
    std::array::from_fn(|index| {
        let coefficient = absorption_coefficients[index];
        if coefficient.is_finite() && coefficient > 0.0 {
            (color[index] * (-optical_depth * coefficient).exp()).clamp(0.0, 1.0)
        } else {
            color[index]
        }
    })
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
        disturbance_rate, make_water_interaction_system, resolve_head_submerged, submersion_system,
        underwater_color_at_depth, underwater_extinction, water_damage_system, DISTURBANCE_BAND,
        DISTURBANCE_RADIUS, WATERLINE_HYSTERESIS,
    };
    use byroredux_core::ecs::components::water::{
        SubmersionState, WaterContact, WaterKind, WaterMaterial, WaterPlane, WaterVolume,
    };
    use byroredux_core::ecs::components::ParticleEmitter;
    use byroredux_core::ecs::components::{ActorValues, ActorVitals, Dead, FollowBehavior};
    use byroredux_core::ecs::{ActiveCamera, GlobalTransform, World};
    use byroredux_core::math::Vec3;
    use byroredux_scripting::{RippleEvent, SplashEvent};

    const EPS: f32 = WATERLINE_HYSTERESIS;

    #[test]
    fn per_frame_water_and_billboard_collections_use_reused_fx_storage() {
        let water = include_str!("water.rs");
        let water = water
            .split("#[cfg(test)]")
            .next()
            .expect("production water source");
        assert!(!water.contains("use std::collections::"));
        for needle in [
            "FxHashSet::<EntityId>::default()",
            "FxHashMap::<EntityId, (EntityId, Vec3, f32)>::default()",
            "std::mem::swap(&mut wet_last_frame, &mut wet_now)",
            "entries.clear()",
            "ripple_by_surface.clear()",
            "splash_by_surface.clear()",
        ] {
            assert!(
                water.contains(needle),
                "water scratch contract lost `{needle}`"
            );
        }

        let billboard = include_str!("billboard.rs");
        let billboard = billboard
            .split("#[cfg(test)]")
            .next()
            .expect("production billboard source");
        assert!(!billboard.contains("use std::collections::"));
        assert!(billboard.contains("FxHashMap<u32, Quat>"));
        assert!(billboard.contains("if geometry_bases.len() > live_geometry_count"));
    }

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
    fn zero_absorption_coefficients_preserve_legacy_underwater_tint() {
        let color = [0.2, 0.4, 0.8];
        assert_eq!(
            underwater_color_at_depth(color, 500.0, 0.0, [0.0; 3]),
            color
        );
    }

    #[test]
    fn starfield_absorption_attenuates_channels_independently_after_near_plane() {
        let color = [1.0, 1.0, 1.0];
        let result = underwater_color_at_depth(color, 20.0, 10.0, [0.16558, 0.09624, 0.07627]);
        assert!((result[0] - (-1.6558f32).exp()).abs() < 1.0e-6);
        assert!((result[1] - (-0.9624f32).exp()).abs() < 1.0e-6);
        assert!((result[2] - (-0.7627f32).exp()).abs() < 1.0e-6);
        assert!(result[2] > result[1] && result[1] > result[0]);
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
                damage_per_second: 0.0,
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

    #[test]
    fn camera_submersion_accepts_a_wave_crest_above_flat_hysteresis() {
        let mut world = World::new();
        let material = WaterMaterial {
            wave_amplitude: 32.0,
            wave_frequency: 0.6,
            ..WaterMaterial::default()
        };
        let (position, wave) = (0..64)
            .flat_map(|x| (0..64).map(move |z| Vec3::new(x as f32 * 17.0, 0.0, z as f32 * 19.0)))
            .map(|position| {
                let wave = byroredux_physics::authored_wave_height_with_weather(
                    &material, position, 0.37, [0.0; 2], 1.0,
                );
                (position, wave)
            })
            .find(|(_, wave)| *wave > WATERLINE_HYSTERESIS + 1.0)
            .expect("test material must produce a crest above the flat band");
        let camera = world.spawn();
        world.insert_resource(ActiveCamera(camera));
        world.insert_resource(byroredux_core::ecs::resources::TotalTime(0.37));
        world.insert(
            camera,
            GlobalTransform::new(
                Vec3::new(position.x, wave + 1.0, position.z),
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
                material,
                damage_per_second: 0.0,
            },
        );
        world.insert(
            water,
            WaterVolume {
                min: [-2000.0, -100.0, -2000.0],
                max: [2000.0, 0.0, 2000.0],
            },
        );

        submersion_system(&world, 0.0);
        let state = world
            .query::<SubmersionState>()
            .expect("submersion storage")
            .get(camera)
            .copied()
            .expect("camera state");
        assert!(state.depth < 0.0, "camera is above the crest: {state:?}");
    }

    #[test]
    fn dynamic_water_contact_emits_entry_once_and_ripples_while_wet() {
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        let surface = world.spawn();
        let body = world.spawn();
        world.insert(
            body,
            GlobalTransform::new(
                Vec3::new(3.0, 1.0, 4.0),
                byroredux_core::math::Quat::IDENTITY,
                1.0,
            ),
        );
        world.insert(
            body,
            WaterContact {
                surface_entity: Some(surface),
                submerged_fraction: 0.5,
                ..WaterContact::default()
            },
        );

        let mut system = make_water_interaction_system();
        system(&world, 0.016);
        let splashes = world.query::<SplashEvent>().expect("splash storage");
        let ripples = world.query::<RippleEvent>().expect("ripple storage");
        // #3115 — was `splashes.get(body)`. The marker belongs on the
        // water-plane entity per `SplashEvent`'s documented contract, which
        // this producer contradicted while its sibling `RippleEvent` (asserted
        // on the line below) already honoured it. `actor` carries the body.
        assert!(splashes.get(surface).is_some());
        assert_eq!(splashes.get(surface).unwrap().actor, body);
        assert!(
            splashes.get(body).is_none(),
            "the body must not host the marker"
        );
        assert!(ripples.get(surface).is_some());
        drop(splashes);
        drop(ripples);

        world.remove::<SplashEvent>(surface);
        world.remove::<RippleEvent>(surface);
        system(&world, 0.016);
        let splashes = world.query::<SplashEvent>().expect("splash storage");
        let ripples = world.query::<RippleEvent>().expect("ripple storage");
        assert!(splashes.get(surface).is_none());
        assert!(ripples.get(surface).is_some());
        drop(splashes);
        drop(ripples);

        world.remove::<RippleEvent>(surface);
        {
            let mut contacts = world.query_mut::<WaterContact>().unwrap();
            contacts.get_mut(body).unwrap().submerged_fraction = 1.0;
        }
        system(&world, 0.016);
        let splashes = world.query::<SplashEvent>().expect("splash storage");
        let ripples = world.query::<RippleEvent>().expect("ripple storage");
        assert!(splashes.get(surface).is_none());
        assert!(
            ripples.get(surface).is_none(),
            "fully submerged bodies must not emit surface ripples"
        );
    }

    #[test]
    fn water_damage_system_applies_contact_hazard_and_marks_death() {
        let mut world = World::new();
        world.register::<ActorValues>();
        world.register::<ActorVitals>();
        world.register::<Dead>();
        world.register::<FollowBehavior>();
        world.register::<WaterContact>();
        world.insert_resource(crate::combat::PendingDeathReconciliations::default());
        let actor = world.spawn();
        world.insert(actor, ActorVitals { health: 7 });
        world.insert(actor, ActorValues::from_pairs([(7, 5.0)]));
        world.insert(
            actor,
            FollowBehavior {
                target_form_id: Some(0x14),
                follow_distance: Some(64.0),
            },
        );
        world.insert(
            actor,
            WaterContact {
                submerged_fraction: 1.0,
                damage_per_second: 20.0,
                ..WaterContact::default()
            },
        );

        water_damage_system(&world, 0.25);

        assert_eq!(world.get::<ActorValues>(actor).unwrap().current(7), 0.0);
        assert!(world.get::<Dead>(actor).is_some());
        assert!(world.get::<FollowBehavior>(actor).is_some());

        crate::combat::reconcile_pending_dead_actors_system(&world, 0.0);
        assert!(
            world.get::<FollowBehavior>(actor).is_none(),
            "the Late sink must apply the shared death reconciler to water kills"
        );
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
                surface_entity: Some(u32::MAX),
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
                damage_per_second: 0.0,
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
            state.surface_entity.is_none(),
            "surface entity must reset to default, not keep feeding \
             compute_underwater_params a stale value"
        );
    }

    #[test]
    fn overlapping_water_volumes_choose_nearest_surface() {
        let mut world = World::new();
        let camera = world.spawn();
        world.insert_resource(ActiveCamera(camera));
        world.insert(
            camera,
            GlobalTransform::new(
                Vec3::new(0.0, 2.0, 0.0),
                byroredux_core::math::Quat::IDENTITY,
                1.0,
            ),
        );
        world.insert(camera, SubmersionState::default());

        let mut expected_surface = None;
        for (surface_y, color) in [(0.0, [0.1, 0.2, 0.3]), (3.0, [0.7, 0.8, 0.9])] {
            let water = world.spawn();
            let mut material = WaterMaterial::default();
            material.shallow_color = color;
            world.insert(
                water,
                WaterPlane {
                    kind: WaterKind::Calm,
                    material,
                    damage_per_second: 0.0,
                },
            );
            world.insert(
                water,
                WaterVolume {
                    min: [-50.0, -100.0, -50.0],
                    max: [50.0, surface_y, 50.0],
                },
            );
            if surface_y == 3.0 {
                expected_surface = Some(water);
            }
        }

        submersion_system(&world, 0.016);
        let state = world
            .query::<SubmersionState>()
            .expect("submersion storage")
            .get(camera)
            .copied()
            .expect("camera state");
        assert_eq!(state.depth, 1.0);
        assert_eq!(state.surface_entity, expected_surface);
    }

    /// #3115 — the two `SplashEvent` producers disagreed with each other and
    /// with the component's own doc about which entity hosts the marker.
    /// `submersion_system` put it on the water-plane entity (correct);
    /// `make_water_interaction_system` put it on the BODY while putting its
    /// sibling `RippleEvent` on the surface, so one function used two host
    /// classes. Nothing was visibly wrong only because the sole consumer reads
    /// the payload's own `position` and ignores the host — a scripting/quest
    /// consumer attaching to a plane would have seen half the splashes.
    #[test]
    fn both_splash_producers_host_the_marker_on_the_water_plane() {
        // Producer A — submersion_system (camera enters the plane).
        let host_a = {
            let mut world = World::new();
            byroredux_scripting::register(&mut world);
            let camera = world.spawn();
            world.insert_resource(ActiveCamera(camera));
            world.insert(
                camera,
                GlobalTransform::new(
                    Vec3::new(0.0, -10.0, 0.0),
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
                    damage_per_second: 0.0,
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
            let splashes = world.query::<SplashEvent>().expect("splash storage");
            assert!(splashes.get(water).is_some(), "producer A: plane must host");
            assert!(
                splashes.get(camera).is_none(),
                "producer A: actor must not host"
            );
            "plane"
        };

        // Producer B — make_water_interaction_system (dynamic body enters).
        let host_b = {
            let mut world = World::new();
            byroredux_scripting::register(&mut world);
            let surface = world.spawn();
            world.insert(
                surface,
                WaterPlane {
                    kind: WaterKind::Calm,
                    material: WaterMaterial::default(),
                    damage_per_second: 0.0,
                },
            );
            world.insert(surface, GlobalTransform::IDENTITY);
            let body = world.spawn();
            world.insert(
                body,
                GlobalTransform::new(
                    Vec3::new(0.0, -5.0, 0.0),
                    byroredux_core::math::Quat::IDENTITY,
                    1.0,
                ),
            );
            world.insert(
                body,
                WaterContact {
                    surface_entity: Some(surface),
                    submerged_fraction: 0.5,
                    ..WaterContact::default()
                },
            );

            let mut system = make_water_interaction_system();
            system(&world, 0.016);
            let splashes = world.query::<SplashEvent>().expect("splash storage");
            assert!(
                splashes.get(surface).is_some(),
                "producer B: plane must host"
            );
            assert!(
                splashes.get(body).is_none(),
                "producer B: body must not host"
            );
            assert_eq!(
                splashes.get(surface).unwrap().actor,
                body,
                "the body belongs in `actor`, not in the host entity"
            );
            "plane"
        };

        assert_eq!(
            host_a, host_b,
            "the two producers must agree on the host class"
        );
    }

    /// #3115 (SIBLING half) — `insert` overwrites, so a `RippleEvent` written
    /// by `submersion_system` earlier in `Stage::Late` was clobbered by
    /// `make_water_interaction_system` whenever a dynamic body shared the
    /// plane: the camera's disturbance vanished without trace. Both producers
    /// now keep the stronger event.
    #[test]
    fn a_weaker_body_disturbance_does_not_clobber_a_stronger_camera_one() {
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        let surface = world.spawn();
        world.insert(
            surface,
            WaterPlane {
                kind: WaterKind::Calm,
                material: WaterMaterial::default(),
                damage_per_second: 0.0,
            },
        );
        world.insert(surface, GlobalTransform::IDENTITY);
        let body = world.spawn();
        world.insert(
            body,
            GlobalTransform::new(
                Vec3::new(0.0, -5.0, 0.0),
                byroredux_core::math::Quat::IDENTITY,
                1.0,
            ),
        );
        world.insert(
            body,
            WaterContact {
                surface_entity: Some(surface),
                submerged_fraction: 0.5,
                ..WaterContact::default()
            },
        );

        // Stand in for the earlier producer's stronger event on the same plane.
        let camera = world.spawn();
        world.insert(
            surface,
            RippleEvent {
                actor: camera,
                intensity: 99.0,
                position: [1.0, 2.0, 3.0],
            },
        );

        let mut system = make_water_interaction_system();
        system(&world, 0.016);

        let ripples = world.query::<RippleEvent>().expect("ripple storage");
        let kept = ripples.get(surface).expect("an event must remain");
        assert_eq!(
            kept.actor, camera,
            "the stronger earlier disturbance (intensity 99) was clobbered by a \
             weaker one from the body producer (#3115)"
        );
        assert_eq!(kept.intensity, 99.0);
    }
}
