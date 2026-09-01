//! Scene setup and NIF loading logic.

use byroredux_core::animation::{AnimationClipRegistry, AnimationPlayer};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{ActiveCamera, Camera, GlobalTransform, MeshHandle, Transform, World};
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::StringPool;
use byroredux_renderer::vulkan::GpuUploadCtx;
use byroredux_renderer::{cube_vertices, quad_vertices, triangle_vertices, VulkanContext};
use byroredux_ui::{ScaleformProfile, UiManager};
use std::sync::Arc;

use crate::anim_convert::convert_nif_clip;
use crate::asset_provider::{
    build_material_provider, build_texture_provider, parse_grid_coords, Archive,
};
use crate::cell_loader;
use crate::components::{InputState, Spinning};
// Interior cell lighting is now applied via
// `cell_loader::apply_interior_cell_lighting` (#1340), so production
// `scene` code no longer names `CellLightingRes` — but the `scene::*`
// test submodules reach it through `use super::*`, so keep it imported
// in test builds only.
#[cfg(test)]
use crate::components::CellLightingRes;
use crate::streaming::WorldStreamingState;
use byroredux_sdk::studio::{AssetBounds, AssetSource, BoundSphere, CornellFit};

// Test child modules (procedural_fallback_tests, cloud_tile_scale_tests)
// reach for these via `use super::*;` — keep them in scope under
// cfg(test). Production code reaches them through the `world_setup`
// submodule directly.
#[cfg(test)]
#[allow(unused_imports)]
use crate::components::{GameTimeRes, SkyParamsRes, WeatherDataRes};

/// Parse the `--radius` CLI argument into a clamped grid radius for
/// [`cell_loader::load_exterior_cells`]. Falls back to `3` (7×7 = 49
/// cells, ~28K terrain units view distance) on any parse failure so
/// an unparseable value loads the default rather than silently
/// bailing. Clamped to `1..=12` — below 1 the center cell alone isn't
/// useful, above 12 the cell count (25×25 = 625) approaches the
/// streaming + RT-BLAS budget ceiling (each static mesh carries a BLAS,
/// see audit D5-02). This bounds only the FULL-DETAIL ring; distant view
/// distance comes from the engine-generated LOD ring
/// (`cell_loader::terrain_lod`), which the 300K-unit camera far plane
/// (`Camera::default`) is sized to cover so neither ring is clipped.
///
/// Pulled out as a free function so a unit test can pin the bounds
/// contract without standing up a whole App / World. See #531.
pub(crate) fn parse_exterior_radius(s: &str) -> i32 {
    match s.trim().parse::<i32>() {
        Ok(r) => r.clamp(1, 12),
        Err(_) => 5,
    }
}

/// Which Cornell-box variant the CLI asked for, if any.
///
/// `None` = no Cornell flag (fall through to the ESM / NIF / demo
/// paths). `Some(false)` = `--cornell`, the interior / point-light
/// scene. `Some(true)` = `--cornell-sun`, the exterior / sun-only
/// variant (#1942).
///
/// `--cornell-sun` implies `--cornell`, and passing both is not an
/// error — sun mode wins, since asking for the sun variant at all means
/// the sun paths are what's being bisected. Pulled out as a free
/// function so the flag contract is unit-testable without a Vulkan
/// device, same rationale as [`parse_exterior_radius`].
pub(crate) fn cornell_sun_mode(args: &[String]) -> Option<bool> {
    let sun = args.iter().any(|a| a == "--cornell-sun");
    if sun {
        Some(true)
    } else if args.iter().any(|a| a == "--cornell") {
        Some(false)
    } else {
        None
    }
}

fn studio_source_label(args: &[String]) -> String {
    args.iter()
        .position(|arg| arg == "--mesh" || arg == "--tree")
        .or_else(|| args.iter().position(|arg| arg == "--studio"))
        .and_then(|index| args.get(index + 1))
        .filter(|value| !value.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| "Imported asset".to_owned())
}

/// Resolve the archive-backed menu CLI pair. Keeping this parser independent
/// of Vulkan gives the engine route a default-suite regression guard (#3147).
fn archive_menu_args(args: &[String]) -> Result<Option<(String, String)>, String> {
    let Some(menu_index) = args.iter().position(|arg| arg == "--menu") else {
        return Ok(None);
    };
    let menu = args
        .get(menu_index + 1)
        .filter(|value| !value.starts_with("--"))
        .ok_or_else(|| "--menu requires an archive-relative SWF path".to_string())?;
    let archive_index = args
        .iter()
        .position(|arg| arg == "--menu-archive")
        .ok_or_else(|| "--menu requires --menu-archive <BSA-or-BA2>".to_string())?;
    let archive = args
        .get(archive_index + 1)
        .filter(|value| !value.starts_with("--"))
        .ok_or_else(|| "--menu-archive requires a BSA/BA2 path".to_string())?;
    Ok(Some((menu.clone(), archive.clone())))
}

/// Choose the starting player rig.
///
/// `ground_walkable` is EX-04's gate (#2375): Character mode may only start
/// from a *verified* walkable surface, not merely from a content-backed cell.
/// A cell can be fully populated and still have nothing under the spawn
/// column — FO3 `MegatonWorld` (0,0) is the reference case — and a capsule
/// placed there falls indefinitely.
///
/// `--player` still overrides everything, per the acceptance: the operator has
/// asked for the capsule and gets it, with a warning.
fn select_initial_player_mode(
    want_fly: bool,
    want_player: bool,
    diagnostic_scene: bool,
    has_content: bool,
    foreground_ready_for_character: bool,
    ground_walkable: bool,
) -> crate::systems::PlayerMode {
    if want_fly {
        crate::systems::PlayerMode::FlyCam
    } else if want_player {
        crate::systems::PlayerMode::Character
    } else if diagnostic_scene {
        // Renderer harness geometry intentionally has no gameplay colliders;
        // a character capsule would fall through it. Fly-cam unless explicit.
        crate::systems::PlayerMode::FlyCam
    } else if has_content && foreground_ready_for_character && ground_walkable {
        crate::systems::PlayerMode::Character
    } else {
        crate::systems::PlayerMode::FlyCam
    }
}

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
fn select_door_spawn_position(
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
fn capsule_center_y_on_surface(
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
    let pw = world.resource::<byroredux_physics::PhysicsWorld>();
    pw.cast_capsule_down_onto_walkable_surface(
        Vec3::new(x, reference_y + probe_lift, z),
        cc.half_height,
        cc.radius,
        FLOOR_PROBE_CLEARANCE_BU + FLOOR_PROBE_REACH_BELOW_DOOR_BU,
        min_walkable_normal_y(cc),
        excluded_body,
    )
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
struct CharacterSpawnPlan {
    body_pos: Vec3,
    controller: byroredux_physics::CharacterController,
    ground_probe: GroundProbe,
}

impl CharacterSpawnPlan {
    fn new(
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
fn plan_character_spawn(
    world: &World,
    cam_pos: Vec3,
    controller: byroredux_physics::CharacterController,
    exterior_foreground: Option<(i32, i32)>,
) -> CharacterSpawnPlan {
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
        let wide_floor_y = if near_door_floor_y.is_none() && door_xz_floor_y.is_none() {
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
            .map(|registry| registry.collision_authoring_totals());
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

mod world_setup;
// Re-export the streaming setup helpers so the M40 Phase 2 cell-
// transition orchestrator in `main.rs::App::step_cell_transition` can
// reuse them on Interior→Exterior swaps — same boot-path code, no
// duplication. See cell_loader::transition.
pub(crate) use world_setup::{
    apply_cell_climate_override, apply_cell_region_ambient, assemble_exterior_streaming,
    begin_exterior_streaming, ExteriorBootstrapMode,
};
// The four `scene/*_tests.rs` child modules reach for these helpers
// via `use super::*;` so they need to be in scope at the parent
// module level. Gating the imports on `#[cfg(test)]` keeps the
// production build from carrying redundant `use` lines.
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use world_setup::{
    cloud_tile_scale_for_dds, insert_procedural_fallback_resources, CLOUD_TILE_SCALE_LAYER_0,
    CLOUD_TILE_SCALE_LAYER_1,
};

/// Called once after the renderer is ready — uploads meshes and spawns entities.
pub(crate) fn setup_scene(
    world: &mut World,
    ctx: &mut VulkanContext,
    ui_manager: &mut Option<UiManager>,
    ui_texture_handle: &mut Option<u32>,
    camera_pos_override: Option<(f32, f32, f32)>,
    camera_forward_override: Option<(f32, f32, f32)>,
    streaming_slot: &mut Option<WorldStreamingState>,
) {
    // Load content from CLI: cell, loose NIF, or BSA NIF.
    let args: Vec<String> = crate::cli_args::effective_args();
    // Game time is global gameplay state, not an exterior-render resource.
    // Seed it for every scene kind so direct-to-interior sessions advance the
    // same persistent clock before they ever visit a worldspace.
    world_setup::ensure_game_time(world);
    let mut cam_center = Vec3::ZERO;
    let mut has_nif_content = false;
    // Exterior CELL presence is not sufficient for Character mode: Bethesda
    // masters contain valid empty/dummy tiles (for example MegatonWorld 0,0).
    // Keep the normal default for every other scene kind and lower it only
    // when the selected exterior foreground has no authored content source.
    let mut foreground_ready_for_character = true;
    let mut nif_root: Option<EntityId> = None;

    // Pending-cell-transition slot — pre-inserted so `&World`-only
    // trigger sites (`door.teleport` console command, gameplay
    // E-key activate) can write the queued transition via
    // `resource_mut` without structural insertion. The main loop's
    // per-frame `take_pending_transition` drains the slot back to
    // `None`. See cell_loader::transition.
    world.insert_resource(cell_loader::PendingCellTransitionSlot::default());

    // Renderer test harnesses (`--combustion-lab`, `--cornell`, `--cornell-sun`, the
    // controlled `--cornell-oracle l0|l1|l2` ladder, or the separate
    // native-scale `--cornell-glass-dragon` Skyrim experiment).
    // Takes precedence over the ESM / NIF / demo paths. Returns the
    // camera pose to use (overridable by the usual `--camera-pos` /
    // `--camera-forward`). `--cornell-sun` selects the exterior /
    // sun-only variant (#1942); see `crate::cornell`.
    let cornell_oracle =
        crate::cornell::cornell_oracle_rung(&args).unwrap_or_else(|message| panic!("{message}"));
    let combustion_lab = crate::cornell::combustion_lab_mode(&args);
    let cornell_glass_dragon = crate::cornell::glass_dragon_mode(&args);
    let cornell_sun = cornell_sun_mode(&args);
    let studio_mode = args.iter().any(|arg| arg == "--studio");
    let diagnostic_scene = combustion_lab
        || cornell_glass_dragon
        || cornell_oracle.is_some()
        || cornell_sun.is_some()
        || studio_mode;
    let mut harness_cam: Option<(Vec3, Vec3)> = None;

    // Cell loading mode: --esm <path> --cell <editor_id> OR --wrld <name> --grid <x>,<y>
    if combustion_lab {
        let (pos, target) = crate::cornell::setup_combustion_lab_scene(world, ctx);
        harness_cam = Some((pos, target));
        cam_center = target;
        has_nif_content = true;
    } else if cornell_glass_dragon {
        let (pos, target) = crate::cornell::setup_cornell_glass_dragon_scene(world, ctx, &args)
            .unwrap_or_else(|message| panic!("{message}"));
        harness_cam = Some((pos, target));
        cam_center = target;
        has_nif_content = true;
    } else if let Some(rung) = cornell_oracle {
        let world_offset = crate::cornell::cornell_oracle_world_offset(&args)
            .unwrap_or_else(|message| panic!("{message}"));
        let (pos, target) =
            crate::cornell::setup_cornell_oracle_scene(world, ctx, rung, world_offset);
        harness_cam = Some((pos, target));
        cam_center = target;
        has_nif_content = true;
    } else if let Some(sun) = cornell_sun {
        let (pos, target) = crate::cornell::setup_cornell_scene(world, ctx, sun);
        harness_cam = Some((pos, target));
        cam_center = target;
        // Skip the demo-primitive spawn + flag the scene as populated so
        // the player rig defaults sensibly (see FlyCam gate below).
        has_nif_content = true;
    } else if let Some(esm_idx) = args
        .iter()
        .position(|a| a == "--esm")
        .filter(|_| !studio_mode)
    {
        let esm_path = args.get(esm_idx + 1).cloned();
        let cell_id = args
            .iter()
            .position(|a| a == "--cell")
            .and_then(|i| args.get(i + 1))
            .cloned();
        let grid_str = args
            .iter()
            .position(|a| a == "--grid")
            .and_then(|i| args.get(i + 1))
            .cloned();
        // #444 — explicit worldspace EDID override. Used with --grid
        // when the ESM defines multiple exterior worldspaces (e.g.
        // FO3 + DLC masters ship Wasteland, PointLookout, Zeta, Pitt,
        // Anchorage) and the automatic pick lands on the wrong one.
        // Case-insensitive EDID match inside load_exterior_cells.
        let wrld_name = args
            .iter()
            .position(|a| a == "--wrld")
            .and_then(|i| args.get(i + 1))
            .cloned();
        // #531 — optional `--radius N` override for the exterior grid.
        // Defaults to 5 (11×11 grid, ~45K terrain units view distance);
        // raised from 3 for a longer horizon. Clamped to 1..=12 by
        // [`parse_exterior_radius`] so an accidental 100 doesn't try
        // to load 40 401 cells.
        let radius = args
            .iter()
            .position(|a| a == "--radius")
            .and_then(|i| args.get(i + 1))
            .map(|s| parse_exterior_radius(s))
            // #1745 — default full-detail ring extended to the 12-cell max
            // (25×25 = 625 cells, ~98K-unit view). Distant content beyond it
            // is the engine LOD ring; the user wants non-distant geometry to
            // reach much further by default. Override with `--radius N`.
            .unwrap_or(12);

        // #561 — repeatable `--master <path>` arg. Order matters:
        // base masters first, then any required intermediate masters
        // (Update.esm before Dawnguard.esm), and finally the main
        // ESM via `--esm`. Each `--master` is collected in CLI order;
        // the cell loader's `_with_masters` entry points compose the
        // global load order as `[masters…, esm]` and parse each plugin
        // with the appropriate FormID remap so a DLC interior REFR
        // placing a base-game STAT resolves cleanly. Without this,
        // Dawnguard / HearthFires / Dragonborn interiors render
        // empty silently. See M46.0.
        let masters: Vec<String> = args
            .iter()
            .enumerate()
            .filter_map(|(i, a)| {
                if a == "--master" {
                    args.get(i + 1).cloned()
                } else {
                    None
                }
            })
            .collect();
        if !masters.is_empty() {
            log::info!("Load order: masters={:?}, main='{:?}'", masters, esm_path);
        }

        // M40 Phase 2 Stage 3 — snapshot the CLI plugin config so the
        // transition orchestrator can re-call `load_cell_with_masters`
        // for the destination of a portal swap without re-parsing CLI
        // args. Inserted whenever --esm is present, before either
        // interior or exterior dispatch.
        if let Some(ref path) = esm_path {
            world.insert_resource(cell_loader::LoadedPluginSet {
                masters: masters.clone(),
                esm_path: path.clone(),
            });
            // M47.2 — install the compiled-script archive (`--scripts-bsa`)
            // so the cell loader's REFR-attach path can resolve a base
            // record's VMAD-named `.pex`, decompile it, and run it through
            // the recognizer chain. Empty (no flag) → every lookup misses
            // and the attach path falls through, same as an unregistered
            // SCPT. Inserted once; it persists across door-walk cell
            // transitions (which reuse the same World).
            let script_provider = crate::asset_provider::build_script_provider(&args);
            let script_principals = script_provider.principals().cloned().collect::<Vec<_>>();
            crate::extensions::register_legacy_script_principals(world, script_principals)
                .expect("engine extension host must be installed before scene setup");
            world.insert_resource(script_provider);
        }

        if let (Some(ref esm_path), Some(ref cell_id)) = (&esm_path, &cell_id) {
            // Interior cell mode
            let tex_provider = build_texture_provider(&args);
            let mut mat_provider = build_material_provider(&args);
            match cell_loader::load_cell_with_masters(
                &masters,
                esm_path,
                cell_id,
                world,
                ctx,
                &tex_provider,
                Some(&mut mat_provider),
            ) {
                Ok(result) => {
                    cam_center = result.center;
                    has_nif_content = true;
                    // Store cell lighting for the renderer. Shared with
                    // the door-walk transition + `cell.load` debug paths
                    // via `apply_interior_cell_lighting` so a runtime cell
                    // switch can't leave a sealed interior lit by the
                    // previous cell's resource (#1340). Always called
                    // (not gated on `Some`) so a cell with neither `XCLL`
                    // nor a resolvable `LTMP` still gets the engine-default
                    // interior fallback instead of inheriting a stale
                    // resource (FNV-D1-01).
                    cell_loader::apply_interior_cell_lighting(world, result.lighting.as_ref());
                    // EX-16 item 1 (#2372) — same "always insert, never
                    // leave a stale prior-cell resource" reasoning as
                    // lighting above; `RegionAmbientRes::default()` is the
                    // correct value for a cell with no ambient directive,
                    // not an absent resource.
                    world.insert_resource(result.region_ambient);
                    // #3559 — keep the phase split readable after the load
                    // returns, so the blocking-first-frame work has a
                    // measurement to target rather than a guess.
                    world.insert_resource(result.phases);
                    log::info!(
                        "Cell '{}' ready: {} entities",
                        result.cell_name,
                        result.entity_count
                    );
                }
                Err(e) => log::error!("Failed to load cell: {:#}", e),
            }
        } else if let (Some(ref esm_path), Some(ref grid)) = (&esm_path, &grid_str) {
            // Exterior cell mode: --esm <path> --grid <x>,<y> — driven
            // through `WorldStreamingState` (M40 Phase 1a). The bulk
            // loader has been retired from this path; cells stream in
            // around the player via `step_streaming` from frame 1.
            // Interactive startup waits only for the foreground cell;
            // deterministic benches keep the fully-populated-radius contract.
            let (cx, cy) = parse_grid_coords(grid);
            let tex_provider = build_texture_provider(&args);
            let mat_provider = build_material_provider(&args);
            match cell_loader::build_exterior_world_context(
                &masters,
                esm_path,
                cx,
                cy,
                radius,
                wrld_name.as_deref(),
            ) {
                Ok(wctx) => {
                    let foreground_readiness = wctx.foreground_readiness((cx, cy), 5);
                    foreground_ready_for_character = foreground_readiness.is_content_backed();
                    has_nif_content = true;
                    let worldspace_key = wctx.worldspace_key.clone();
                    let bootstrap_mode = ExteriorBootstrapMode::from_cli_args(&args);
                    let (state, center) = crate::scene::assemble_exterior_streaming(
                        world,
                        ctx,
                        wctx,
                        tex_provider,
                        mat_provider,
                        (cx, cy),
                        radius,
                        bootstrap_mode,
                        // First-ever load off the `--grid` CLI path — no
                        // prior streaming state to preserve a root from.
                        None,
                    );
                    cam_center = center;
                    log::info!(
                        "Streaming context ready: worldspace '{}', radius {} (load), {} (unload), {} cells loaded, {} pending ({:?})",
                        state.wctx.worldspace_key,
                        state.radius_load,
                        state.radius_unload,
                        state.loaded.len(),
                        state.pending.len(),
                        bootstrap_mode,
                    );
                    // EX-09/17 item 4 — mirror `begin_exterior_streaming`'s identity
                    // stamp: this call site can't use that helper directly
                    // (it needs `wctx.foreground_readiness` before the
                    // context is consumed), so it stamps
                    // `CurrentExteriorContext` itself instead of silently
                    // leaving boot-time exterior sessions unsaveable.
                    world.insert_resource(cell_loader::CurrentExteriorContext {
                        worldspace_key,
                        esm_path: esm_path.clone(),
                        masters: masters.clone(),
                        grid: (cx, cy),
                        radius_load: state.radius_load,
                        radius_unload: state.radius_unload,
                    });
                    *streaming_slot = Some(state);
                }
                Err(e) => log::error!("Failed to build exterior world context: {:#}", e),
            }
        } else {
            log::error!("--esm requires either --cell <editor_id> or --grid <x>,<y>");
        }
    } else {
        // NIF loading mode: loose file or BSA extraction.
        let first_asset_entity = world.next_entity_id();
        let (nif_count, loaded_root) = load_nif_from_args(world, ctx);
        has_nif_content = nif_count > 0;
        nif_root = loaded_root;
        if studio_mode && has_nif_content {
            let last_asset_entity = world.next_entity_id();
            let mut propagate = byroredux_core::ecs::systems::make_transform_propagation_system();
            propagate(world, 0.0);
            let mut propagate_bounds = crate::systems::make_world_bound_propagation_system();
            propagate_bounds(world, 0.0);

            let mut objects: Vec<EntityId> = world
                .query::<byroredux_core::ecs::LocalBound>()
                .map(|query| {
                    query
                        .iter()
                        .filter_map(|(entity, _)| {
                            (entity >= first_asset_entity && entity < last_asset_entity)
                                .then_some(entity)
                        })
                        .collect()
                })
                .unwrap_or_default();
            // Canonicalize the import order before assigning SDK ObjectIds.
            // Entity allocation follows the deterministic NIF/SPT traversal,
            // while storage iteration order is an implementation detail.
            objects.sort_unstable();
            let bounds = AssetBounds::from_spheres(objects.iter().filter_map(|&entity| {
                world
                    .get::<byroredux_core::ecs::WorldBound>(entity)
                    .map(|bound| BoundSphere {
                        center: bound.center.to_array(),
                        radius: bound.radius,
                    })
            }))
            .unwrap_or(AssetBounds {
                min: [-1.0; 3],
                max: [1.0; 3],
            });
            let fit = CornellFit::around(bounds);
            let (camera, target) = crate::cornell::setup_studio_room(world, ctx, fit);
            harness_cam = Some((camera, target));
            cam_center = target;
            crate::studio_host::install_session(
                world,
                AssetSource {
                    label: studio_source_label(&args),
                },
                objects,
            );
        }
    }

    // Animation: --kf <path> loads a .kf file and starts playback.
    // Tries BSA extraction first (KF files live in mesh BSAs), falls back to loose file.
    if let Some(kf_idx) = args.iter().position(|a| a == "--kf") {
        if let Some(kf_path) = args.get(kf_idx + 1).cloned() {
            let kf_provider = build_texture_provider(&args);
            let kf_data = kf_provider
                .extract_mesh(&kf_path)
                .inspect(|_| {
                    log::info!("Extracted KF from BSA: '{}'", kf_path);
                })
                .or_else(|| {
                    std::fs::read(&kf_path)
                        .map_err(|e| log::error!("Failed to read KF '{}': {}", kf_path, e))
                        .ok()
                });
            if let Some(kf_data) = kf_data {
                match byroredux_nif::parse_nif(&kf_data) {
                    Ok(kf_scene) => {
                        let nif_clips = byroredux_nif::anim::import_kf(&kf_scene);
                        if nif_clips.is_empty() {
                            log::warn!("No animation clips found in '{}'", kf_path);
                        } else {
                            let first_handle;
                            {
                                let mut registry = world.resource_mut::<AnimationClipRegistry>();
                                let mut pool = world.resource_mut::<StringPool>();
                                for nif_clip in &nif_clips {
                                    let clip = convert_nif_clip(nif_clip, &mut pool);
                                    let handle = registry.add(clip);
                                    log::info!(
                                        "Loaded animation clip '{}' ({:.2}s, {} channels) → handle {}",
                                        nif_clip.name,
                                        nif_clip.duration,
                                        nif_clip.channels.len(),
                                        handle,
                                    );
                                }
                                first_handle = registry.len() as u32 - nif_clips.len() as u32;
                            }

                            // #2221 — attach the non-transform sinks for
                            // EVERY clip this KF registered, not just
                            // `first_handle`: an `AnimationStack` can
                            // later play any of them on the same subtree,
                            // and a sink that only exists for clip 0
                            // leaves the rest silently inert.
                            if let Some(root) = nif_root {
                                let last_handle = first_handle + nif_clips.len() as u32;
                                for handle in first_handle..last_handle {
                                    let channels = {
                                        let registry = world.resource::<AnimationClipRegistry>();
                                        registry.get(handle).map(|clip| {
                                            (
                                                clip.bool_channels.clone(),
                                                clip.float_channels.clone(),
                                                clip.color_channels.clone(),
                                                clip.texture_flip_channels.clone(),
                                            )
                                        })
                                    };
                                    if let Some((bools, floats, colors, texture_flips)) = channels {
                                        crate::anim_convert::attach_animation_sinks(
                                            world,
                                            &bools,
                                            &floats,
                                            &colors,
                                            &texture_flips,
                                            Some(ctx),
                                            Some(&kf_provider),
                                            root,
                                        );
                                    }
                                }
                            }

                            // Spawn an AnimationPlayer scoped to the NIF subtree.
                            let player_entity = world.spawn();
                            // #3345 — start at the clip's authored phase offset.
                            let phase = world
                                .resource::<AnimationClipRegistry>()
                                .get(first_handle)
                                .map(|c| c.phase)
                                .unwrap_or(0.0);
                            let mut player = AnimationPlayer::new(first_handle).with_phase(phase);
                            if let Some(root) = nif_root {
                                player.root_entity = Some(root);
                            }
                            world.insert(player_entity, player);
                            log::info!("Animation playback started (clip handle {})", first_handle);
                        }
                    }
                    Err(e) => log::error!("Failed to parse KF '{}': {}", kf_path, e),
                }
            }
        }
    }

    // Only spawn demo primitives when no NIF content was loaded.
    if !has_nif_content {
        let alloc = ctx.allocator.as_ref().unwrap();
        let (verts, idxs) = cube_vertices();
        let queue = &ctx.graphics_queue;
        let pool = ctx.transfer_pool;
        let rt = ctx.device_caps.ray_query_supported;
        let upload_ctx = GpuUploadCtx {
            device: &ctx.device,
            allocator: alloc,
            queue,
            command_pool: pool,
        };
        let cube_handle = ctx
            .mesh_registry
            .upload(upload_ctx, &verts, &idxs, rt, None)
            .expect("Failed to upload cube mesh");

        let (quad_verts, quad_idxs) = quad_vertices();
        let quad_handle = ctx
            .mesh_registry
            .upload(upload_ctx, &quad_verts, &quad_idxs, rt, None)
            .expect("Failed to upload quad mesh");

        let (red_verts, red_idxs) = triangle_vertices([1.0, 0.2, 0.2]);
        let red_handle = ctx
            .mesh_registry
            .upload(upload_ctx, &red_verts, &red_idxs, rt, None)
            .expect("Failed to upload red triangle mesh");

        let (blue_verts, blue_idxs) = triangle_vertices([0.2, 0.2, 1.0]);
        let blue_handle = ctx
            .mesh_registry
            .upload(upload_ctx, &blue_verts, &blue_idxs, rt, None)
            .expect("Failed to upload blue triangle mesh");

        // Batched BLAS build for RT shadows on demo meshes.
        let (cv, ci) = (verts.len() as u32, idxs.len() as u32);
        let (qv, qi) = (quad_verts.len() as u32, quad_idxs.len() as u32);
        let (rv, ri) = (red_verts.len() as u32, red_idxs.len() as u32);
        let (bv, bi) = (blue_verts.len() as u32, blue_idxs.len() as u32);
        ctx.build_blas_batched(&[
            (cube_handle, cv, ci),
            (quad_handle, qv, qi),
            (red_handle, rv, ri),
            (blue_handle, bv, bi),
        ]);

        let cube = world.spawn();
        world.insert(cube, Transform::from_translation(Vec3::new(-1.5, 0.0, 0.0)));
        world.insert(cube, GlobalTransform::IDENTITY);
        world.insert(cube, MeshHandle(cube_handle));
        world.insert(cube, Spinning);

        let quad = world.spawn();
        world.insert(quad, Transform::from_translation(Vec3::new(0.0, 0.0, -1.0)));
        world.insert(quad, GlobalTransform::IDENTITY);
        world.insert(quad, MeshHandle(quad_handle));
        world.insert(quad, Spinning);

        let red_tri = world.spawn();
        world.insert(
            red_tri,
            Transform::from_translation(Vec3::new(1.5, 0.0, 0.5)),
        );
        world.insert(red_tri, GlobalTransform::IDENTITY);
        world.insert(red_tri, MeshHandle(red_handle));
        world.insert(red_tri, Spinning);

        let blue_tri = world.spawn();
        world.insert(
            blue_tri,
            Transform::from_translation(Vec3::new(1.8, 0.0, -0.3)),
        );
        world.insert(blue_tri, GlobalTransform::IDENTITY);
        world.insert(blue_tri, MeshHandle(blue_handle));
        world.insert(blue_tri, Spinning);
    }

    // Spawn camera entity looking at the scene center — unless CLI
    // overrides are supplied (`--camera-pos` / `--camera-forward`),
    // in which case the requested pose wins. Useful for offline
    // diagnostic renders without needing interactive WASD.
    let cam = world.spawn();
    let cam_pos = match (camera_pos_override, harness_cam) {
        (Some((x, y, z)), _) => Vec3::new(x, y, z),
        // Each renderer harness owns its physical scale, so its declared
        // camera wins over the NIF-oriented fallback offset.
        (None, Some((pos, _))) => pos,
        (None, None) if has_nif_content => cam_center + Vec3::new(0.0, 100.0, 200.0),
        (None, None) => Vec3::new(0.0, 1.5, 4.0),
    };
    let cam_target = cam_center;
    let forward = match camera_forward_override {
        Some((x, y, z)) => {
            let v = Vec3::new(x, y, z);
            if v.length_squared() > 1e-8 {
                v.normalize()
            } else {
                log::warn!("--camera-forward 0,0,0 is invalid; using computed look-at");
                (cam_target - cam_pos).normalize()
            }
        }
        None => (cam_target - cam_pos).normalize(),
    };
    let cam_rotation = Quat::from_rotation_arc(-Vec3::Z, forward);
    world.insert(cam, Transform::new(cam_pos, cam_rotation, 1.0));
    world.insert(cam, GlobalTransform::new(cam_pos, cam_rotation, 1.0));
    // #3308 — BU-scale content (a loaded worldspace/interior cell, a loose
    // NIF mesh/tree view) gets the vanilla-matching near plane;
    // `harness_cam.is_some()` excludes calibrated renderer harnesses
    // (Cornell/combustion-lab scenes), which declare their own camera pose
    // and some of which sit at comparably small physical scale despite
    // loading real NIF content — content presence alone isn't a safe scale
    // signal. See `Camera::for_content_scale`'s doc.
    world.insert(
        cam,
        Camera::for_content_scale(has_nif_content && harness_cam.is_none()),
    );
    // M44 Phase 1: the camera entity doubles as the audio listener
    // ("ears at the eyes"). M28.5 character controller will likely
    // split listener onto a head joint of the player capsule, but
    // for fly-cam fidelity this is canonical.
    world.insert(cam, byroredux_audio::AudioListener);
    // M44 Phase 3.5: opt the camera into footstep dispatch. Stride
    // threshold + per-footstep volume are read from `FootstepConfig`
    // (engine-wide resource set up in `App::new`).
    world.insert(cam, crate::components::FootstepEmitter::new());
    // Submersion state is recomputed each frame by `submersion_system`
    // from active `WaterPlane` / `WaterVolume` entities. Pre-inserting
    // the default keeps the system on the pure-mutation path (no
    // structural inserts mid-frame).
    world.insert(
        cam,
        byroredux_core::ecs::components::water::SubmersionState::default(),
    );
    world.insert_resource(ActiveCamera(cam));

    // M28.5 — Player rig selection. Character mode requires actual
    // content in the world (cell loaded successfully OR loose NIF
    // loaded) — spawning the capsule into an empty void falls forever
    // and the user sees a blank screen with no way to diagnose. Gate
    // on `has_nif_content` so a failed `--esm` load (missing BSA /
    // missing ESM at the CLI-given path) silently falls back to
    // FlyCam, which at least shows the sweet-roll cubes. The log
    // shows the underlying load error either way.
    //
    //   --fly                       → FlyCam (force, useful for debug)
    //   --player                    → Character (force, even with no content)
    //   --esm/--mesh/--tree loaded  → Character (default for content)
    //   no content                  → FlyCam (default)
    let want_fly = args.iter().any(|a| a == "--fly");
    let want_player = args.iter().any(|a| a == "--player");

    // EX-04 / #2375 — probe for walkable ground *before* choosing the mode.
    //
    // Pre-fix the order was reversed: the mode was chosen from cell content
    // alone, then the spawn probe ran, and a probe miss placed the capsule at
    // `aabb.max.y + 200` — 200 units above the world with nothing beneath it.
    // That is the indefinite free-fall EX-02 describes; the probe knew the
    // ground was missing but had no way to veto the decision it came after.
    //
    // Only run when Character is actually reachable: `--fly`, Cornell and
    // content-less loads all resolve to FlyCam regardless, and the probe costs
    // an early physics sync.
    let character_controller = byroredux_physics::CharacterController::HUMAN;
    let spawn_plan = if want_fly || (!want_player && (diagnostic_scene || !has_nif_content)) {
        None
    } else {
        // Scene setup owns `&mut World` before the scheduler starts. Register
        // only pending colliders and refresh the query BVH for the ground
        // probe; a zero-dt full physics tick would mutate unrelated phases
        // without scheduler access analysis (#3267).
        byroredux_physics::register_newcomers_and_refresh_queries(world);
        let exterior_foreground = streaming_slot
            .as_ref()
            .and_then(|state| state.last_player_grid);
        let plan = plan_character_spawn(world, cam_pos, character_controller, exterior_foreground);
        // Greppable telemetry line for the smoke matrix — EX-04 asks for the
        // static-collider count and the probe result to be captured.
        log::info!("{}", plan.ground_probe.telemetry_line());
        Some(plan)
    };
    let ground_probe = spawn_plan.map(|plan| plan.ground_probe);
    // Absent probe = a path that was never going to pick Character, so it must
    // not veto anything; `true` keeps the pre-existing precedence intact.
    let ground_walkable = ground_probe.is_none_or(|p| p.is_walkable());

    let player_mode = select_initial_player_mode(
        want_fly,
        want_player,
        diagnostic_scene,
        has_nif_content,
        foreground_ready_for_character,
        ground_walkable,
    );
    world.insert_resource(player_mode);
    if player_mode == crate::systems::PlayerMode::FlyCam {
        if has_nif_content && !foreground_ready_for_character && !want_fly {
            log::warn!(
                "Player rig: FlyCam because the requested exterior foreground is empty or missing \
                 (use `--player` to override at your own risk)"
            );
        } else if has_nif_content && !ground_walkable && !want_fly {
            // Distinct from the empty-foreground case above: the cell has
            // content, but nothing walkable sits under the spawn column, so a
            // capsule here would free-fall.
            log::warn!(
                "Player rig: FlyCam because the spawn ground probe found no walkable surface \
                 ({}) — use `--player` to override at your own risk",
                ground_probe
                    .map(|p| p.telemetry_line())
                    .unwrap_or_else(|| "no probe".to_string()),
            );
        } else {
            log::info!(
                "Player rig: FlyCam (use `--player` to force Character mode without cell content)"
            );
        }
    } else {
        log::info!("Player rig: Character (M28.5 kinematic capsule + gravity)");
    }

    // M28.5 — Spawn the player character body when in Character mode.
    // The body sits at `cam_pos` (the camera's initial spawn point)
    // minus eye_height so the eyes end up where the camera was.
    // `character_controller_system` will then take over per-frame
    // updates; `camera_follow_system` re-pins the camera to the body
    // head each frame.
    if player_mode == crate::systems::PlayerMode::Character {
        let plan = spawn_plan.expect("Character mode requires a resolved spawn plan");
        let cc = plan.controller;
        let body_pos = plan.body_pos;
        let body = world.spawn();
        world.insert(body, Transform::new(body_pos, Quat::IDENTITY, 1.0));
        world.insert(body, GlobalTransform::new(body_pos, Quat::IDENTITY, 1.0));
        world.insert(body, cc);
        // M28.5 follow-up — character body flows through the unified
        // Path A in `physics_sync_system`. Attach the capsule + body
        // data so `register_newcomers` builds the Rapier body from the
        // same code path as every NIF-imported collider. The
        // `CharacterKinematic` motion type maps to Rapier's
        // `KinematicPositionBased` but signals to `push_kinematic` that
        // it must NOT push the ECS Transform each frame — the character
        // controller system drives the pose explicitly via
        // `set_kinematic_translation`.
        use byroredux_core::ecs::components::collision::{
            CollisionShape, MotionType, RigidBodyData,
        };
        world.insert(
            body,
            CollisionShape::Capsule {
                half_height: cc.half_height,
                radius: cc.radius,
            },
        );
        world.insert(
            body,
            RigidBodyData {
                motion_type: MotionType::CharacterKinematic,
                mass: 80.0,
                friction: 0.5,
                restitution: 0.0,
                linear_damping: 0.0,
                angular_damping: 0.0,
                collidable: true,
            },
        );
        // #1846 / SAVE-03 — attach a FormIdComponent built from the
        // reserved player sentinel pair so the player body is a normal
        // remappable entity for the M45.1 live-load `old -> live` remap
        // (`build_form_id_remap`), the same mechanism every NPC's
        // FormIdComponent already uses. Without this, any persistable
        // component landing on the player body (inventory, equipment,
        // actor values) is captured to disk but silently dropped on
        // every live load — the remap has no pair to match it against.
        {
            use byroredux_core::ecs::components::FormIdComponent;
            use byroredux_core::form_id::{FormIdPool, PLAYER_FORM_ID_PAIR};
            let fid = world
                .resource_mut::<FormIdPool>()
                .intern(PLAYER_FORM_ID_PAIR);
            world.insert(body, FormIdComponent(fid));
        }
        // #3158 — give the player a `Perks` component up front, even
        // though nothing populates it yet. `ConditionFunction::HasPerk`
        // reads `world.get::<Perks>(entity)` and returns `0.0` when the
        // component is absent, which is indistinguishable from "the actor
        // genuinely lacks this perk" — so before this, every perk-gated
        // dialogue/quest/package CTDA on the player was a silent
        // structural false in every game, with no log or test able to tell
        // the two apart. An empty component makes "checked and absent"
        // representable and gives a future `AddPerk` effect somewhere to
        // write.
        //
        // SIBLING (#3158's completeness box): `ActorValues`,
        // `CharacterLevel` and `Background` have the identical
        // single-writer shape (`spawn_npc_entity` only) and are likewise
        // absent on the player. They are deliberately NOT stubbed here.
        // An empty `ActorValues` would flip `melee_damage_charal_bonus`
        // (`combat.rs`) off its `else { return 0.0 }` arm and onto a
        // derived-value computation over a zero SPECIAL set, and
        // `Background` has no honest value until the player has a real
        // race/class. Populating those is CHARAL work (#3004 / #2986),
        // not a component stub.
        world.insert(body, byroredux_core::character::Perks::default());
        crate::inventory::attach_to_player(world, body);
        // The player participates in QUST aliases exactly like an authored
        // ACHR: Skyrim's MQ101 alias 119 is a forced reference to 0x14 and
        // carries the opening inventory/package injections.  The player body
        // was already persistently identifiable through FormIdComponent, but
        // the alias resolver deliberately reads SceneAliasCandidate instead;
        // without this stamp MQ101 could start while its Player alias remained
        // permanently unbound.
        world.insert(
            body,
            byroredux_scripting::SceneAliasCandidate {
                reference_form_id: 0x0000_0014,
                base_form_id: 0x0000_0007,
                linked_refs: Vec::new(),
                location_ref_types: Vec::new(),
            },
        );
        byroredux_scripting::mark_scene_actor_bindings_dirty(world);
        world.insert_resource(crate::systems::PlayerEntity(Some(body)));
        // M47.0 — the scripting crate's papyrus_demo systems
        // (rumble_on_activate, quest_advance, mg07_door,
        // dlc2_ttr4a) fetch this resource UNCONDITIONALLY at the
        // top of their bodies, before the event-loop early-return.
        // Distinct struct from `crate::systems::PlayerEntity` —
        // papyrus_demo's `PlayerEntity(EntityId)` has no Option
        // wrapper (designed assuming caller always inserts), so an
        // absent resource panics on the first frame. Bind to the
        // same `body` entity so any future scripting-driven player
        // lookup (Game.GetPlayer().GetReference()) resolves to the
        // M28.5 capsule. See the M47.0 / R5 closeout.
        world.insert_resource(byroredux_scripting::papyrus_demo::PlayerEntity(body));
        // M47.0 — same pattern as PlayerEntity above. The
        // quest_advance / dlc2_ttr4a / mg07_door dispatcher systems
        // do `world.resource_mut::<QuestStageState>()` unconditionally
        // (set_stage writes), and mg07_door also `resource()`-reads it
        // for stage-gated activation. QuestStageState::default() is
        // an empty HashMap — scripts populate it lazily on first
        // set_stage. M47.1 condition functions GetStage / GetStageDone
        // already use try_resource so they're safe on absence.
        world.insert_resource(byroredux_scripting::quest_stages::QuestStageState::default());
        log::info!(
            "M28.5 player character spawned at ({:.1}, {:.1}, {:.1}); eyes at ({:.1}, {:.1}, {:.1})",
            body_pos.x,
            body_pos.y,
            body_pos.z,
            cam_pos.x,
            cam_pos.y,
            cam_pos.z,
        );
    } else {
        // FlyCam mode — the PlayerEntity resource still exists (so
        // systems can early-return on `.0.is_none()` instead of
        // panicking on absent resource), it's just empty.
        world.insert_resource(crate::systems::PlayerEntity::default());
        // M47.0 — papyrus_demo's PlayerEntity has no Option wrapper
        // and its consumer systems fetch the resource before the
        // event-loop early-return. Spawn an empty placeholder
        // entity so the resource fetch resolves; the scripting
        // systems no-op on it because the placeholder has no
        // Player / Camera / Reference components. Cost: one
        // unused EntityId.
        let placeholder = world.spawn();
        world.insert_resource(byroredux_scripting::papyrus_demo::PlayerEntity(placeholder));
        // M47.0 — same insert as the Character-mode branch above so
        // the quest-stage-aware systems don't panic on FlyCam scenes
        // (debug bench, --mesh standalone NIF loads, headless smoke).
        world.insert_resource(byroredux_scripting::quest_stages::QuestStageState::default());
    }

    // Initialize fly camera yaw/pitch from the initial look direction.
    // Even in Character mode the InputState yaw/pitch drives the
    // camera + WASD alignment — there's no separate character-mode
    // input path.
    {
        let mut input = world.resource_mut::<InputState>();
        input.yaw = forward.x.atan2(-forward.z);
        input.pitch = forward.y.asin();
    }

    // Build the global geometry SSBO for RT reflection ray UV lookups.
    // StagingPool reuse lives on `MeshRegistry.geometry_staging_pool` —
    // lazy-init on first call, reused across cell loads + frame-loop
    // rebuilds. Closes the #242 consumer-side TODO (#1055).
    if let Err(e) = ctx.mesh_registry.build_geometry_ssbo(
        &ctx.device,
        ctx.allocator.as_ref().unwrap(),
        &ctx.graphics_queue,
        ctx.transfer_pool,
        ctx.device_caps.ray_query_supported,
    ) {
        log::warn!("Failed to build geometry SSBO: {e}");
    }
    // Write global geometry buffers to scene descriptor sets for RT reflection UV lookups.
    if let (Some(ref vb), Some(ref ib)) = (
        &ctx.mesh_registry.global_vertex_buffer,
        &ctx.mesh_registry.global_index_buffer,
    ) {
        for f in 0..2 {
            ctx.scene_buffers.write_geometry_buffers(
                &ctx.device,
                f,
                vb.buffer,
                vb.size,
                ib.buffer,
                ib.size,
            );
        }
    }

    let total_entities = world.next_entity_id();
    log::info!(
        "Scene ready: {} entities, 1 camera. Press Escape to capture mouse for fly camera.",
        total_entities
    );

    // Register the fullscreen quad mesh for UI overlay.
    if let Err(e) = ctx.register_ui_quad() {
        log::error!("Failed to register UI quad: {e:#}");
    }
    // Register the unit XY quad used by the CPU particle billboard path
    // (#401). One DrawCommand per live particle references this handle.
    if let Err(e) = ctx.register_particle_quad() {
        log::error!("Failed to register particle quad: {e:#}");
    }

    // UI: `--menu` launches a vanilla archive-backed menu with its relative
    // imports; `--swf` remains the loose-file developer route.
    let archive_menu = archive_menu_args(&args);
    if let Ok(Some((menu_path, archive_path))) = archive_menu.as_ref() {
        match Archive::open(archive_path) {
            Ok(archive) => match archive.extract(menu_path) {
                Ok(root_bytes) => match ScaleformProfile::detect(&root_bytes) {
                    Ok(profile) => {
                        let (w, h) = ctx.swapchain_extent();
                        let mut ui = UiManager::new(w, h);
                        match ui.load_swf_from_resource_provider(
                            Arc::new(archive),
                            menu_path,
                            menu_path,
                            profile,
                        ) {
                            Ok(()) => {
                                let pixels = vec![0u8; (w * h * 4) as usize];
                                let allocator = ctx.allocator.as_ref().unwrap();
                                let upload_ctx = GpuUploadCtx {
                                    device: &ctx.device,
                                    allocator,
                                    queue: &ctx.graphics_queue,
                                    command_pool: ctx.transfer_pool,
                                };
                                match ctx
                                    .texture_registry
                                    .register_rgba(upload_ctx, w, h, &pixels)
                                {
                                    Ok(handle) => {
                                        // #3273 — the only success-side
                                        // observable on this route. Every
                                        // other arm below logs its failure,
                                        // so without this the route is
                                        // silent exactly when it works and
                                        // a smoke gate has nothing to
                                        // assert on. Keep the
                                        // `ui.menu: loaded` prefix and the
                                        // `profile=` / `texture=` keys
                                        // stable — `m48-menu-load.sh`
                                        // greps for them (a fixed-string
                                        // match on the prefix, so appending
                                        // `state=` below is safe).
                                        //
                                        // #3427 — `state=` is the missing
                                        // observable this line lacked: an
                                        // AVM2 menu whose host object landed
                                        // in `NotPresent` used to print this
                                        // exact line with no way to tell it
                                        // apart from a clean injection.
                                        log::info!(
                                            "ui.menu: loaded path={} archive={} profile={:?} texture={:?} state={:?}",
                                            menu_path,
                                            archive_path,
                                            profile,
                                            handle,
                                            ui.host_object_state()
                                        );
                                        *ui_texture_handle = Some(handle);
                                        *ui_manager = Some(ui);
                                    }
                                    Err(error) => {
                                        log::error!("Failed to register UI texture: {error:#}")
                                    }
                                }
                            }
                            Err(error) => log::error!(
                                "Failed to load archive menu '{}' from '{}': {error:#}",
                                menu_path,
                                archive_path
                            ),
                        }
                    }
                    Err(error) => log::error!(
                        "Failed to detect Scaleform profile for '{}': {error:#}",
                        menu_path
                    ),
                },
                Err(error) => log::error!(
                    "Failed to extract archive menu '{}' from '{}': {error}",
                    menu_path,
                    archive_path
                ),
            },
            Err(error) => log::error!("Failed to open UI archive '{}': {error}", archive_path),
        }
    } else if let Err(error) = archive_menu {
        log::error!("{error}");
    } else if let Some(swf_idx) = args.iter().position(|a| a == "--swf") {
        if let Some(swf_path) = args.get(swf_idx + 1) {
            match std::fs::read(swf_path) {
                Ok(swf_data) => {
                    let (w, h) = ctx.swapchain_extent();
                    let mut ui = UiManager::new(w, h);
                    match ui.load_swf(&swf_data, swf_path) {
                        Ok(()) => {
                            // Create the initial UI texture (transparent black).
                            let pixels = vec![0u8; (w * h * 4) as usize];
                            let allocator = ctx.allocator.as_ref().unwrap();
                            let upload_ctx = GpuUploadCtx {
                                device: &ctx.device,
                                allocator,
                                queue: &ctx.graphics_queue,
                                command_pool: ctx.transfer_pool,
                            };
                            match ctx
                                .texture_registry
                                .register_rgba(upload_ctx, w, h, &pixels)
                            {
                                Ok(handle) => {
                                    *ui_texture_handle = Some(handle);
                                    // #3427 — same `state=` observable as the
                                    // `--menu` archive route, so a loose-file
                                    // AVM2 dev SWF whose host object landed in
                                    // `NotPresent` is visible here too.
                                    log::info!(
                                        "UI texture registered (handle {}) state={:?}",
                                        handle,
                                        ui.host_object_state()
                                    );
                                }
                                Err(e) => log::error!("Failed to register UI texture: {e:#}"),
                            }
                            *ui_manager = Some(ui);
                        }
                        Err(e) => log::error!("Failed to load SWF '{}': {e:#}", swf_path),
                    }
                }
                Err(e) => log::error!("Failed to read SWF file '{}': {e}", swf_path),
            }
        } else {
            log::error!("--swf requires a file path");
        }
    }
}

mod nif_loader;
use nif_loader::load_nif_from_args;
pub(crate) use nif_loader::{load_nif_bytes, load_nif_bytes_with_skeleton};

#[cfg(test)]
mod cloud_tile_scale_tests;
#[cfg(test)]
mod archive_menu_route_tests {
    use super::archive_menu_args;

    #[test]
    fn route_requires_and_returns_menu_and_archive_paths() {
        let args = vec![
            "byroredux".to_string(),
            "--menu".to_string(),
            "interface\\hudmenu.swf".to_string(),
            "--menu-archive".to_string(),
            "Fallout4 - Interface.ba2".to_string(),
        ];
        assert_eq!(
            archive_menu_args(&args).unwrap(),
            Some((
                "interface\\hudmenu.swf".to_string(),
                "Fallout4 - Interface.ba2".to_string()
            ))
        );
        assert!(archive_menu_args(&args[..3]).is_err());
    }
}
#[cfg(test)]
mod nif_loader_light_tests;
#[cfg(test)]
mod procedural_fallback_tests;
#[cfg(test)]
mod radius_parse_tests;
#[cfg(test)]
mod spawn_tests;

#[cfg(test)]
mod studio_cli_tests {
    use super::studio_source_label;

    #[test]
    fn archive_mesh_label_wins_over_the_marker_flag() {
        let args = [
            "byroredux",
            "--studio",
            "--bsa",
            "meshes.bsa",
            "--mesh",
            "meshes/probe.nif",
        ]
        .map(str::to_owned);
        assert_eq!(studio_source_label(&args), "meshes/probe.nif");
    }
}
