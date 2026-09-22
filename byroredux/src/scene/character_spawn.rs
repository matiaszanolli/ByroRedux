//! Character spawn planning, extracted from `scene.rs` (#4569): the
//! door-selection, ground-probe and spawn-plan logic that decides where
//! Character mode places the player, kept apart from the scene-setup
//! orchestration that calls it.

use byroredux_core::ecs::components::Transform;
use byroredux_core::ecs::{World};
use byroredux_core::math::Vec3;

/// Pick the first door whose placement belongs to the collision-ready
/// foreground exterior cell. Interior loads pass `None` and preserve the
/// historical "first door in the cell" behaviour.
///
/// The persistent worldspace CELL is materialised before the streamed
/// foreground tile and can contain doors from every cell in the 3D radius.
/// Treating that combined ECS set as if it were one interior cell lets an
/// arbitrary persistent door win even when its terrain is still queued. The
/// character then starts over an unloaded tile and free-falls before streaming
/// can catch up.
pub(crate) fn select_door_spawn_position(
    door_positions: impl IntoIterator<Item = Vec3>,
    exterior_foreground: Option<(i32, i32)>,
) -> Option<Vec3> {
    door_positions.into_iter().find(|position| {
        exterior_foreground.is_none_or(|foreground| {
            crate::streaming::world_pos_to_grid(position.x, position.z) == foreground
        })
    })
}

/// Convert a floor-surface Y into the Y of a vertical capsule's centre.
///
/// Rapier's `capsule_y(half_height, radius)` extends by both the cylindrical
/// half-height and the hemispherical radius below its centre. Omitting the
/// radius starts the character embedded in the floor, where wall/edge contacts
/// can leave the controller blocked but permanently ungrounded (#2193).
pub(crate) fn capsule_center_y_on_surface(
    surface_y: f32,
    half_height: f32,
    radius: f32,
    kcc_offset_bu: f32,
) -> f32 {
    surface_y + half_height + radius + kcc_offset_bu
}

/// #2858 — a downward capsule sweep must START above the floor it is looking
/// for. The probe capsule's own half-extent is `half_height + radius`, so
/// lifting the origin by that plus a clearance margin keeps the door's own
/// floor out of the initial-penetration blind zone. Derived from the
/// controller rather than hard-coded so a `CharacterController` re-tune cannot
/// reintroduce the blind zone.
pub(crate) const FLOOR_PROBE_CLEARANCE_BU: f32 = 16.0;
/// How far below the reference height the probe keeps searching.
pub(crate) const FLOOR_PROBE_REACH_BELOW_DOOR_BU: f32 = 164.0;

/// Column half-width the spawn census scans, BU.
///
/// `pub(crate)` under #2876 so the `phys.census` console command probes the
/// same column the boot-time door-teleport census does — a live census that
/// used a different radius would not be comparable with the frame-0 log it
/// exists to follow up on.
pub(crate) const SPAWN_CENSUS_RADIUS_BU: f32 = 256.0;

pub(crate) fn floor_probe_lift(cc: byroredux_physics::CharacterController) -> f32 {
    cc.half_height + cc.radius + FLOOR_PROBE_CLEARANCE_BU
}

pub(crate) fn min_walkable_normal_y(cc: byroredux_physics::CharacterController) -> f32 {
    cc.max_slope_climb_deg.to_radians().cos()
}

/// Probe for the nearest walkable floor beneath `(x, z)`, searching a bounded
/// band around `reference_y`.
///
/// The shared rung of the spawn-grounding ladder. Doors and XTEL destinations
/// sit at floor level by construction, so a band centred on that height finds
/// the real local floor — or correctly reports nothing nearby, rather than a
/// false hit far above.
///
/// `exclude` is the entity whose own rigid body must not count as floor. Cold
/// start passes `None` (the capsule does not exist yet); the runtime
/// door-transition path passes the player, whose capsule is very much alive
/// and standing in the sweep (#2869).
///
/// Requires a fresh query pipeline — see
/// [`byroredux_physics::PhysicsWorld::update_query_pipeline`].
pub(crate) fn probe_walkable_floor_near(
    world: &World,
    x: f32,
    z: f32,
    reference_y: f32,
    cc: byroredux_physics::CharacterController,
    exclude: Option<byroredux_core::ecs::storage::EntityId>,
) -> Option<f32> {
    // Resolve the exclusion handle and release the component guard before
    // taking the `PhysicsWorld` lock — `RapierHandles` → `PhysicsWorld` is the
    // order `push_kinematic` uses, and holding the query across the resource
    // would be a second edge for the same pair.
    let excluded_body = exclude.and_then(|entity| {
        world
            .query::<byroredux_physics::RapierHandles>()
            .and_then(|handles| handles.get(entity).map(|h| h.body))
    });
    let probe_lift = floor_probe_lift(cc);
    let center_offset = character_spawn_center_y(world, 0.0, cc);
    let pw = world.resource::<byroredux_physics::PhysicsWorld>();
    pw.cast_capsule_down_onto_walkable_surface(
        Vec3::new(x, reference_y + probe_lift, z),
        cc.half_height,
        cc.radius,
        FLOOR_PROBE_CLEARANCE_BU + FLOOR_PROBE_REACH_BELOW_DOOR_BU,
        min_walkable_normal_y(cc),
        excluded_body,
    )
    .filter(|surface_y| {
        !pw.capsule_overlaps_solid(
            Vec3::new(x, surface_y + center_offset, z),
            cc.half_height,
            cc.radius,
            excluded_body,
        )
    })
}

/// A door pivot is not necessarily free space. Search a small, deterministic
/// ring near that pivot when the usual inward and threshold columns fail.
/// Every candidate must have both walkable support and room for the capsule.
pub(crate) fn clear_spawn_near_door(
    world: &World,
    door: Vec3,
    inward: Option<Vec3>,
    cc: byroredux_physics::CharacterController,
    foreground: Option<(i32, i32)>,
) -> Option<(f32, f32, f32, &'static str)> {
    let direction = inward.unwrap_or(Vec3::X);
    for radius in [64.0, 128.0] {
        for step in 0..8 {
            let angle = step as f32 * std::f32::consts::FRAC_PI_4;
            let (sin, cos) = angle.sin_cos();
            let x = door.x + radius * (direction.x * cos - direction.z * sin);
            let z = door.z + radius * (direction.x * sin + direction.z * cos);
            if foreground.is_some_and(|grid| crate::streaming::world_pos_to_grid(x, z) != grid) {
                continue;
            }
            if let Some(y) = probe_walkable_floor_near(world, x, z, door.y, cc, None) {
                return Some((y, x, z, "clear nearby door column"));
            }
        }
    }
    None
}

pub(crate) fn character_spawn_center_y(
    world: &World,
    surface_y: f32,
    cc: byroredux_physics::CharacterController,
) -> f32 {
    let kcc_offset_bu = world
        .try_resource::<byroredux_physics::ContactConfig>()
        .map(|config| config.kcc_offset_bu)
        .unwrap_or(byroredux_physics::ContactConfig::DEFAULT.kcc_offset_bu);
    capsule_center_y_on_surface(surface_y, cc.half_height, cc.radius, kcc_offset_bu)
}

/// Ground the character beneath the camera/terrain-center spawn column.
///
/// This is both the normal no-door path and the safety net for a door whose
/// capsule probes find no collision-ready floor. Falling back to the requested
/// foreground column is strictly safer than trusting an authored door height:
/// the latter may belong to a persistent exterior reference whose tile has not
/// streamed yet.
/// Outcome of the spawn-time ground probe (EX-04 / #2375).
///
/// A typed result rather than a bare position, because the *reason* a probe
/// failed decides what the engine should do next: with no colliders at all
/// there is nothing to stand on anywhere, whereas an empty column over a
/// populated world is a bad spawn point in an otherwise fine cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum GroundProbe {
    /// A collider was found beneath the spawn column.
    Grounded {
        /// Exact world column shared by the probe and final capsule placement.
        x: f32,
        z: f32,
        /// Surface Y the ray hit.
        surface_y: f32,
        /// Capsule-centre Y derived from it.
        spawn_y: f32,
        /// Static colliders in the physics world at probe time.
        collider_count: u32,
    },
    /// Static colliders exist, but none beneath the spawn column.
    NoFloorBeneath {
        x: f32,
        z: f32,
        searched_bu: f32,
        collider_count: u32,
    },
    /// No static colliders at all — the cell has no walkable surface.
    NoColliders,
}

impl GroundProbe {
    /// Whether the probe found walkable ground.
    ///
    /// This is the gate EX-04 requires: Character mode may only start from a
    /// *verified* walkable surface, not merely from a content-backed cell.
    pub(crate) fn is_walkable(&self) -> bool {
        matches!(self, Self::Grounded { .. })
    }

    pub(crate) fn collider_count(&self) -> u32 {
        match self {
            Self::Grounded { collider_count, .. } | Self::NoFloorBeneath { collider_count, .. } => {
                *collider_count
            }
            Self::NoColliders => 0,
        }
    }

    /// One greppable line for the smoke matrix (EX-04's telemetry clause).
    pub(crate) fn telemetry_line(&self) -> String {
        match self {
            Self::Grounded {
                x,
                z,
                surface_y,
                spawn_y,
                ..
            } => format!(
                "spawn-probe: result=grounded colliders={} x={:.1} z={:.1} surface_y={:.1} spawn_y={:.1}",
                self.collider_count(),
                x,
                z,
                surface_y,
                spawn_y
            ),
            Self::NoFloorBeneath {
                x,
                z,
                searched_bu,
                ..
            } => format!(
                "spawn-probe: result=no-floor colliders={} x={:.1} z={:.1} searched_bu={:.1}",
                self.collider_count(),
                x,
                z,
                searched_bu
            ),
            Self::NoColliders => "spawn-probe: result=no-colliders colliders=0".to_string(),
        }
    }
}

/// Decompose a forward vector into the `(yaw, pitch)` pair that
/// `fly_camera_system` would need to produce it (#2383).
///
/// That system composes `Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch)`
/// and looks down local `-Z`, which expands to
///
/// ```text
/// forward = (-cos(pitch)·sin(yaw),  sin(pitch),  -cos(pitch)·cos(yaw))
/// ```
///
/// so `pitch = asin(y)` and `yaw = atan2(-x, -z)`. Inverting the engine's own
/// convention — rather than picking any rotation that happens to face the
/// right way — is the whole point: a shortest-arc quaternion also faces the
/// right way and still snaps on the first mouse event, because it carries
/// roll the driving system cannot represent.
/// Probe for walkable ground beneath `cam_pos`.
///
/// Split from the spawn itself so the *decision* (may Character mode start?)
/// is separable from the *placement*, and so the outcome is unit-testable.
fn probe_spawn_ground(
    world: &World,
    cam_pos: Vec3,
    cc: byroredux_physics::CharacterController,
) -> GroundProbe {
    let pw = world.resource::<byroredux_physics::PhysicsWorld>();
    let Some((min, max, collider_count)) = pw.static_colliders_aabb() else {
        return GroundProbe::NoColliders;
    };
    let aabb_height = (max[1] - min[1]).max(1.0);
    let ray_origin = Vec3::new(cam_pos.x, max[1] + 50.0, cam_pos.z);
    let searched_bu = aabb_height + 100.0;
    // Runs before the player capsule is spawned, so there is no self-hit to
    // exclude (#2859).
    match pw.cast_ray_down(ray_origin, searched_bu, None) {
        Some(surface_y) => GroundProbe::Grounded {
            x: cam_pos.x,
            z: cam_pos.z,
            surface_y,
            spawn_y: character_spawn_center_y(world, surface_y, cc),
            collider_count,
        },
        None => GroundProbe::NoFloorBeneath {
            x: cam_pos.x,
            z: cam_pos.z,
            searched_bu,
            collider_count,
        },
    }
}

/// Position the capsule from an already-taken [`GroundProbe`].
///
/// Pre-#2375 the no-floor arm placed the capsule at `aabb.max.y + 200` — 200
/// units above everything, with nothing beneath it. That *is* the indefinite
/// free-fall EX-02 describes, and it happened because the mode decision was
/// made before the probe ran, so a failed probe had no way to veto Character
/// mode. The caller now demotes to FlyCam instead; this fallback survives only
/// for the explicit `--player` override, where the operator has asked for the
/// capsule regardless.
fn spawn_position_from_probe(
    probe: GroundProbe,
    cam_pos: Vec3,
    cc: byroredux_physics::CharacterController,
    world: &World,
    reason: &str,
) -> Vec3 {
    match probe {
        GroundProbe::Grounded {
            surface_y, spawn_y, ..
        } => {
            log::info!(
                "M28.5 spawn ray-cast: hit floor at y={:.1} under \
                 ({:.1}, {:.1}); placing capsule at y={:.1} ({reason})",
                surface_y,
                cam_pos.x,
                cam_pos.z,
                spawn_y,
            );
            Vec3::new(cam_pos.x, spawn_y, cam_pos.z)
        }
        GroundProbe::NoFloorBeneath { searched_bu, .. } => {
            let pw = world.resource::<byroredux_physics::PhysicsWorld>();
            let top = pw
                .static_colliders_aabb()
                .map_or(cam_pos.y, |(_, max, _)| max[1]);
            log::warn!(
                "M28.5 spawn ray-cast: NO floor found under ({:.1}, \
                 {:.1}) within {:.1} BU; falling back to \
                 aabb.max.y + 200 ({reason})",
                cam_pos.x,
                cam_pos.z,
                searched_bu,
            );
            Vec3::new(cam_pos.x, top + 200.0, cam_pos.z)
        }
        GroundProbe::NoColliders => cam_pos - Vec3::Y * cc.eye_height,
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CharacterSpawnPlan {
    pub(crate) body_pos: Vec3,
    pub(crate) controller: byroredux_physics::CharacterController,
    pub(crate) ground_probe: GroundProbe,
}

impl CharacterSpawnPlan {
    pub(crate) fn new(
        body_pos: Vec3,
        controller: byroredux_physics::CharacterController,
        ground_probe: GroundProbe,
    ) -> Self {
        if let GroundProbe::Grounded { x, z, .. } | GroundProbe::NoFloorBeneath { x, z, .. } =
            ground_probe
        {
            debug_assert!((body_pos.x - x).abs() < 0.01);
            debug_assert!((body_pos.z - z).abs() < 0.01);
        }
        Self {
            body_pos,
            controller,
            ground_probe,
        }
    }
}

/// Resolve one character spawn plan before mode selection.
///
/// The selected door/camera column, controller dimensions, ground result and
/// final body position travel together. This prevents the startup gate from
/// certifying the camera column and then spawning a differently-sized capsule
/// at an independently-selected door column (#3002).
pub(crate) fn plan_character_spawn(
    world: &World,
    cam_pos: Vec3,
    controller: byroredux_physics::CharacterController,
    exterior_foreground: Option<(i32, i32)>,
    explicit_camera_position: bool,
) -> CharacterSpawnPlan {
    if explicit_camera_position {
        // --camera-pos is an eye position. Keep its column instead of letting
        // an unrelated first door replace it; search near the expected feet,
        // not from the top of the cell where a roof can win.
        let reference_y =
            cam_pos.y - controller.eye_height - character_spawn_center_y(world, 0.0, controller);
        let aabb = world
            .resource::<byroredux_physics::PhysicsWorld>()
            .static_colliders_aabb();
        let floor =
            probe_walkable_floor_near(world, cam_pos.x, cam_pos.z, reference_y, controller, None);
        let (body_pos, ground_probe) = if let Some(surface_y) = floor {
            let spawn_y = character_spawn_center_y(world, surface_y, controller);
            (
                Vec3::new(cam_pos.x, spawn_y, cam_pos.z),
                GroundProbe::Grounded {
                    x: cam_pos.x,
                    z: cam_pos.z,
                    surface_y,
                    spawn_y,
                    collider_count: aabb.map_or(0, |(_, _, count)| count),
                },
            )
        } else {
            // Automatic mode selection will refuse an unsupported column.
            // Explicit --player still forces a capsule, but at the requested
            // eye position, never silently at a different door or on the roof.
            let probe = aabb.map_or(GroundProbe::NoColliders, |(_, _, collider_count)| {
                GroundProbe::NoFloorBeneath {
                    x: cam_pos.x,
                    z: cam_pos.z,
                    searched_bu: FLOOR_PROBE_CLEARANCE_BU + FLOOR_PROBE_REACH_BELOW_DOOR_BU,
                    collider_count,
                }
            });
            (cam_pos - Vec3::Y * controller.eye_height, probe)
        };
        log::info!(
            "Character spawn uses explicit camera column: body=({:.1}, {:.1}, {:.1}), supported={}",
            body_pos.x,
            body_pos.y,
            body_pos.z,
            ground_probe.is_walkable()
        );
        return CharacterSpawnPlan::new(body_pos, controller, ground_probe);
    }
    let door_spawn = {
        let doors = world.query::<crate::components::DoorTeleport>();
        let transforms = world.query::<Transform>();
        match (doors, transforms) {
            (Some(doors), Some(transforms)) => select_door_spawn_position(
                doors
                    .iter()
                    .filter_map(|(entity, _)| transforms.get(entity).map(|t| t.translation)),
                exterior_foreground,
            ),
            _ => None,
        }
    };

    if let Some(door_pos) = door_spawn {
        const INWARD_NUDGE_BU: f32 = 64.0;
        let aabb = world
            .resource::<byroredux_physics::PhysicsWorld>()
            .static_colliders_aabb();
        let inward_xz = aabb.and_then(|(min, max, _)| {
            let centre = Vec3::new(0.5 * (min[0] + max[0]), 0.0, 0.5 * (min[2] + max[2]));
            let to_centre = Vec3::new(centre.x - door_pos.x, 0.0, centre.z - door_pos.z);
            (to_centre.length_squared() > 1.0).then(|| to_centre.normalize())
        });
        let nudge = inward_xz.unwrap_or(Vec3::ZERO) * INWARD_NUDGE_BU;
        let nudged_x = door_pos.x + nudge.x;
        let nudged_z = door_pos.z + nudge.z;

        // Prefer the inward column, fall back to the threshold itself, then
        // search the full cell height at the inward column. Every rung uses
        // the exact controller that will be inserted on the player entity.
        let near_door_floor_y =
            probe_walkable_floor_near(world, nudged_x, nudged_z, door_pos.y, controller, None);
        let door_xz_floor_y = near_door_floor_y
            .is_none()
            .then(|| {
                probe_walkable_floor_near(
                    world, door_pos.x, door_pos.z, door_pos.y, controller, None,
                )
            })
            .flatten();
        let nearby_floor = if near_door_floor_y.is_none() && door_xz_floor_y.is_none() {
            clear_spawn_near_door(world, door_pos, inward_xz, controller, exterior_foreground)
        } else {
            None
        };
        let wide_floor_y =
            if near_door_floor_y.is_none() && door_xz_floor_y.is_none() && nearby_floor.is_none() {
                aabb.and_then(|(min, max, _)| {
                    let probe_lift = floor_probe_lift(controller);
                    world
                        .resource::<byroredux_physics::PhysicsWorld>()
                        .cast_capsule_down_onto_walkable_surface(
                            Vec3::new(nudged_x, max[1] + probe_lift, nudged_z),
                            controller.half_height,
                            controller.radius,
                            (max[1] - min[1]).max(1.0) + probe_lift + 100.0,
                            min_walkable_normal_y(controller),
                            None,
                        )
                })
                .filter(|surface_y| {
                    let center_y = character_spawn_center_y(world, *surface_y, controller);
                    !world
                        .resource::<byroredux_physics::PhysicsWorld>()
                        .capsule_overlaps_solid(
                            Vec3::new(nudged_x, center_y, nudged_z),
                            controller.half_height,
                            controller.radius,
                            None,
                        )
                })
            } else {
                None
            };

        let resolved = near_door_floor_y
            .map(|surface_y| (surface_y, nudged_x, nudged_z, "nudged XZ near door height"))
            .or_else(|| {
                door_xz_floor_y.map(|surface_y| {
                    (
                        surface_y,
                        door_pos.x,
                        door_pos.z,
                        "door XZ near door height (nudge landed over a hole)",
                    )
                })
            })
            .or(nearby_floor)
            .or_else(|| {
                wide_floor_y.map(|surface_y| {
                    (
                        surface_y,
                        nudged_x,
                        nudged_z,
                        "full-cell sweep at nudged XZ",
                    )
                })
            });

        if let Some((surface_y, spawn_x, spawn_z, floor_rung)) = resolved {
            let spawn_y = character_spawn_center_y(world, surface_y, controller);
            let body_pos = Vec3::new(spawn_x, spawn_y, spawn_z);
            let collider_count = aabb.map_or(0, |(_, _, count)| count);
            log::info!(
                "M28.5 spawn at door teleporter: door at ({:.1}, {:.1}, {:.1}); \
                 floor probe hit y={surface_y:.1} via {floor_rung}; placing capsule \
                 at ({:.1}, {:.1}, {:.1}){}",
                door_pos.x,
                door_pos.y,
                door_pos.z,
                body_pos.x,
                body_pos.y,
                body_pos.z,
                if inward_xz.is_none() {
                    " — NUDGE DEGRADED: no usable AABB-centre direction"
                } else {
                    ""
                },
            );
            return CharacterSpawnPlan::new(
                body_pos,
                controller,
                GroundProbe::Grounded {
                    x: body_pos.x,
                    z: body_pos.z,
                    surface_y,
                    spawn_y,
                    collider_count,
                },
            );
        }

        log::warn!(
            "M28.5 spawn at door teleporter: all floor probes missed at door \
             ({:.1}, {:.1}, {:.1}); rejecting the door column",
            door_pos.x,
            door_pos.y,
            door_pos.z,
        );
        let probe_lift = floor_probe_lift(controller);
        let authoring = world
            .try_resource::<crate::cell_loader::NifImportRegistry>()
            .map(|registry| registry.collision_authoring_totals_registry_wide());
        byroredux_physics::dump_spawn_collider_census(
            world,
            byroredux_physics::SpawnCensusProbe {
                x: nudged_x,
                y: door_pos.y + probe_lift,
                z: nudged_z,
                radius: SPAWN_CENSUS_RADIUS_BU,
                capsule_half_height: controller.half_height,
                capsule_radius: controller.radius,
                max_distance: FLOOR_PROBE_CLEARANCE_BU + FLOOR_PROBE_REACH_BELOW_DOOR_BU,
                min_walkable_normal_y: min_walkable_normal_y(controller),
                authoring,
                // #3965 — no player capsule exists yet at this boot-time
                // spawn-probe path, so there is nothing to self-hit.
                excluded_body: None,
            },
        );
    }

    let reason = if door_spawn.is_some() {
        "door floor probe missed"
    } else {
        "no foreground DoorTeleport"
    };
    let ground_probe = probe_spawn_ground(world, cam_pos, controller);
    let body_pos = spawn_position_from_probe(ground_probe, cam_pos, controller, world, reason);
    CharacterSpawnPlan::new(body_pos, controller, ground_probe)
}


