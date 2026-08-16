//! Scene setup and NIF loading logic.

use byroredux_core::animation::{AnimationClipRegistry, AnimationPlayer};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{ActiveCamera, Camera, GlobalTransform, MeshHandle, Transform, World};
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::StringPool;
use byroredux_renderer::vulkan::GpuUploadCtx;
use byroredux_renderer::{cube_vertices, quad_vertices, triangle_vertices, VulkanContext};
use byroredux_ui::UiManager;

use crate::anim_convert::convert_nif_clip;
use crate::asset_provider::{build_material_provider, build_texture_provider, parse_grid_coords};
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
    cornell: bool,
    has_content: bool,
    foreground_ready_for_character: bool,
    ground_walkable: bool,
) -> crate::systems::PlayerMode {
    if want_fly {
        crate::systems::PlayerMode::FlyCam
    } else if want_player {
        crate::systems::PlayerMode::Character
    } else if cornell {
        // The Cornell box has no colliders; a character capsule would fall
        // through the floor. Fly-cam unless explicitly overridden.
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
const FLOOR_PROBE_CLEARANCE_BU: f32 = 16.0;
/// How far below the reference height the probe keeps searching.
const FLOOR_PROBE_REACH_BELOW_DOOR_BU: f32 = 164.0;

fn floor_probe_lift(cc: byroredux_physics::CharacterController) -> f32 {
    cc.half_height + cc.radius + FLOOR_PROBE_CLEARANCE_BU
}

fn min_walkable_normal_y(cc: byroredux_physics::CharacterController) -> f32 {
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
        /// Surface Y the ray hit.
        surface_y: f32,
        /// Capsule-centre Y derived from it.
        spawn_y: f32,
        /// Static colliders in the physics world at probe time.
        collider_count: u32,
    },
    /// Static colliders exist, but none beneath the spawn column.
    NoFloorBeneath {
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
                surface_y, spawn_y, ..
            } => format!(
                "spawn-probe: result=grounded colliders={} surface_y={:.1} spawn_y={:.1}",
                self.collider_count(),
                surface_y,
                spawn_y
            ),
            Self::NoFloorBeneath { searched_bu, .. } => format!(
                "spawn-probe: result=no-floor colliders={} searched_bu={:.1}",
                self.collider_count(),
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
            surface_y,
            spawn_y: character_spawn_center_y(world, surface_y, cc),
            collider_count,
        },
        None => GroundProbe::NoFloorBeneath {
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

fn spawn_on_camera_ground(
    world: &World,
    cam_pos: Vec3,
    cc: byroredux_physics::CharacterController,
    reason: &str,
) -> Vec3 {
    let probe = probe_spawn_ground(world, cam_pos, cc);
    spawn_position_from_probe(probe, cam_pos, cc, world, reason)
}

mod world_setup;
// Re-export the streaming setup helpers so the M40 Phase 2 cell-
// transition orchestrator in `main.rs::App::step_cell_transition` can
// reuse them on Interior→Exterior swaps — same boot-path code, no
// duplication. See cell_loader::transition.
pub(crate) use world_setup::{
    apply_cell_climate_override, apply_worldspace_weather, stream_initial_radius,
    ExteriorBootstrapMode,
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

    // Cornell-box test harness (`--cornell`, `--cornell-sun`, the
    // controlled `--cornell-oracle l0|l1|l2` ladder, or the separate
    // native-scale `--cornell-glass-dragon` Skyrim experiment).
    // Takes precedence over the ESM / NIF / demo paths. Returns the
    // camera pose to use (overridable by the usual `--camera-pos` /
    // `--camera-forward`). `--cornell-sun` selects the exterior /
    // sun-only variant (#1942); see `crate::cornell`.
    let cornell_oracle =
        crate::cornell::cornell_oracle_rung(&args).unwrap_or_else(|message| panic!("{message}"));
    let cornell_glass_dragon = crate::cornell::glass_dragon_mode(&args);
    let cornell_sun = cornell_sun_mode(&args);
    let cornell = cornell_glass_dragon || cornell_oracle.is_some() || cornell_sun.is_some();
    let mut cornell_cam: Option<(Vec3, Vec3)> = None;

    // Cell loading mode: --esm <path> --cell <editor_id> OR --wrld <name> --grid <x>,<y>
    if cornell_glass_dragon {
        let (pos, target) = crate::cornell::setup_cornell_glass_dragon_scene(world, ctx, &args)
            .unwrap_or_else(|message| panic!("{message}"));
        cornell_cam = Some((pos, target));
        cam_center = target;
        has_nif_content = true;
    } else if let Some(rung) = cornell_oracle {
        let world_offset = crate::cornell::cornell_oracle_world_offset(&args)
            .unwrap_or_else(|message| panic!("{message}"));
        let (pos, target) =
            crate::cornell::setup_cornell_oracle_scene(world, ctx, rung, world_offset);
        cornell_cam = Some((pos, target));
        cam_center = target;
        has_nif_content = true;
    } else if let Some(sun) = cornell_sun {
        let (pos, target) = crate::cornell::setup_cornell_scene(world, ctx, sun);
        cornell_cam = Some((pos, target));
        cam_center = target;
        // Skip the demo-primitive spawn + flag the scene as populated so
        // the player rig defaults sensibly (see FlyCam gate below).
        has_nif_content = true;
    } else if let Some(esm_idx) = args.iter().position(|a| a == "--esm") {
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
            world.insert_resource(crate::asset_provider::build_script_provider(&args));
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
                    crate::asset_provider::populate_scene_runtime(world, &wctx.record_index);
                    crate::asset_provider::populate_havok_idle_runtime(
                        world,
                        &wctx.record_index,
                        &tex_provider,
                    );
                    apply_worldspace_weather(world, ctx, &tex_provider, &wctx);
                    let mut state =
                        WorldStreamingState::new(wctx, tex_provider, mat_provider, radius);
                    state.last_player_grid = Some((cx, cy));
                    state.spawn_lod_water(world, ctx);
                    let bootstrap_mode = ExteriorBootstrapMode::from_cli_args(&args);
                    cam_center =
                        stream_initial_radius(world, ctx, &mut state, cx, cy, bootstrap_mode);
                    log::info!(
                        "Streaming context ready: worldspace '{}', radius {} (load), {} (unload), {} cells loaded, {} pending ({:?})",
                        state.wctx.worldspace_key,
                        state.radius_load,
                        state.radius_unload,
                        state.loaded.len(),
                        state.pending.len(),
                        bootstrap_mode,
                    );
                    *streaming_slot = Some(state);
                }
                Err(e) => log::error!("Failed to build exterior world context: {:#}", e),
            }
        } else {
            log::error!("--esm requires either --cell <editor_id> or --grid <x>,<y>");
        }
    } else {
        // NIF loading mode: loose file or BSA extraction.
        let (nif_count, loaded_root) = load_nif_from_args(world, ctx);
        has_nif_content = nif_count > 0;
        nif_root = loaded_root;
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
                                            )
                                        })
                                    };
                                    if let Some((bools, floats, colors)) = channels {
                                        crate::anim_convert::attach_animation_sinks(
                                            world, &bools, &floats, &colors, root,
                                        );
                                    }
                                }
                            }

                            // Spawn an AnimationPlayer scoped to the NIF subtree.
                            let player_entity = world.spawn();
                            let mut player = AnimationPlayer::new(first_handle);
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
    let cam_pos = match (camera_pos_override, cornell_cam) {
        (Some((x, y, z)), _) => Vec3::new(x, y, z),
        // Cornell box uses small world-unit scale (room ~8 units), so the
        // NIF camera offset (100, 200) would put the camera far outside.
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
    world.insert(cam, Camera::default());
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
    let cc_for_probe = byroredux_physics::CharacterController::HUMAN;
    let ground_probe = if want_fly || cornell || !(has_nif_content || want_player) {
        None
    } else {
        // `physics_sync_system` inserts the colliders into `ColliderSet`, but
        // `QueryPipeline` only learns about them via `pipeline.step()`. dt=0
        // registers newcomers without moving anything; the explicit BVH flush
        // is what makes `cast_ray_down` see the cell architecture at all.
        byroredux_physics::physics_sync_system(world, 0.0);
        {
            let mut pw = world.resource_mut::<byroredux_physics::PhysicsWorld>();
            pw.update_query_pipeline();
        }
        let probe = probe_spawn_ground(world, cam_pos, cc_for_probe);
        // Greppable telemetry line for the smoke matrix — EX-04 asks for the
        // static-collider count and the probe result to be captured.
        log::info!("{}", probe.telemetry_line());
        Some(probe)
    };
    // Absent probe = a path that was never going to pick Character, so it must
    // not veto anything; `true` keeps the pre-existing precedence intact.
    let ground_walkable = ground_probe.is_none_or(|p| p.is_walkable());

    let player_mode = select_initial_player_mode(
        want_fly,
        want_player,
        cornell,
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
        use byroredux_physics::CharacterController;
        let cc = CharacterController::HUMAN;

        // M28.5 — physics_sync_system normally runs in the scheduler's
        // Stage::Physics, but the character body needs to spawn at a
        // position that doesn't overlap any cell collider. We need
        // the static-collider AABB to pick a safe Y. Force one early
        // physics tick (dt=0 so no movement, just newcomer registration)
        // so the AABB is available.
        //
        // This also means the player body's own newcomer registration
        // happens on the FIRST scheduler-driven tick, not this one —
        // which is correct, since the body isn't spawned yet.
        // The dt=0 sync and BVH flush this path used to perform now happen
        // above, before the mode decision, so the ground probe can veto
        // Character mode rather than discovering the missing floor too late
        // (#2375). Re-running them here would be redundant work.

        // Spawn precedence:
        //   1. **Door-teleporter spawn** — find any REFR with a
        //      `DoorTeleport` component (XTEL — the entries/exits of
        //      this cell) and place the player at that door's
        //      `Transform.translation`, offset upward by capsule
        //      `half_height + offset_skin` so the capsule's feet rest
        //      on the door's floor reference. This matches Bethesda's
        //      own spawn convention — when you teleport INTO a cell,
        //      you appear at the door REFR that pointed at the cell
        //      you came from. Spawning at one of THIS cell's doors at
        //      cold-start gives the same "you walked in here" effect
        //      without needing to know which exterior cell you came
        //      from. See user-requested change M28.5 follow-up:
        //      "always spawn in the proper spawn point for a room
        //      (this should be on the end of a teleporter object like
        //      a door)".
        //   2. **Ray-cast down** — when there's no DoorTeleport in
        //      the cell (debug `--mesh` loads, exterior cells without
        //      teleporter REFRs, etc.) — fall back to the previous
        //      M28.5 strategy: ray-cast from `aabb.max.y + 50 BU` and
        //      place the capsule above the first solid floor.
        //   3. **AABB + slack** — ray-cast found nothing within the
        //      AABB-height + 100 BU budget. Place at `aabb.max.y +
        //      200` (very rare; was the pre-#1230 path).
        //   4. **No static colliders** — bare `cam_pos - eye_height`.
        //
        // Offset 4.0 BU on the upward shift matches the KCC's
        // `controller.offset`, so the capsule rests against (not
        // embedded in) the door's floor reference.
        let exterior_foreground = streaming_slot
            .as_ref()
            .and_then(|state| state.last_player_grid);
        let door_spawn = {
            let dq = world.query::<crate::components::DoorTeleport>();
            let tq = world.query::<Transform>();
            match (dq, tq) {
                (Some(dq), Some(tq)) => select_door_spawn_position(
                    dq.iter()
                        .filter_map(|(entity, _door)| tq.get(entity).map(|t| t.translation)),
                    exterior_foreground,
                ),
                _ => None,
            }
        };
        let body_pos = if let Some(door_pos) = door_spawn {
            // Door REFRs are placed at the door's outer threshold — the
            // boundary between cell interior and exterior. Spawning the
            // capsule at exactly `door_pos` puts its centre on that
            // boundary; with capsule radius 18 BU the capsule projects
            // beyond the static-collider AABB and lands in the void
            // (observed at WhiterunBanneredMare: door Z=1152.0, AABB
            // Z_max=1151.9, character free-falls). Push the spawn
            // *inward* along the XZ vector from door to the static-
            // collider AABB centre so the capsule's XZ lands well past
            // the threshold, independent of door rotation conventions or
            // per-game subtleties.
            const INWARD_NUDGE_BU: f32 = 64.0;
            let aabb = {
                let pw = world.resource::<byroredux_physics::PhysicsWorld>();
                pw.static_colliders_aabb()
            };
            let inward_xz = aabb.and_then(|(min, max, _)| {
                let centre = Vec3::new(0.5 * (min[0] + max[0]), 0.0, 0.5 * (min[2] + max[2]));
                let to_centre = Vec3::new(centre.x - door_pos.x, 0.0, centre.z - door_pos.z);
                let len_sq = to_centre.length_squared();
                if len_sq > 1.0 {
                    Some(to_centre / len_sq.sqrt())
                } else {
                    // Door is at the AABB centre already — no
                    // meaningful inward direction. Skip the nudge.
                    None
                }
            });
            let nudge = inward_xz.unwrap_or(Vec3::ZERO) * INWARD_NUDGE_BU;
            let nudged_x = door_pos.x + nudge.x;
            let nudged_z = door_pos.z + nudge.z;

            // #2013 — trusting `door_pos.y` as the floor height (the
            // pre-fix behavior) assumes the door's own Y is the spawn
            // floor, but the inward nudge moves the XZ position off the
            // threshold into whatever is 64 BU further into the room —
            // which empirically is NOT always clear floor: on Skyrim's
            // WhiterunDragonsreach the nudge lands over open space (the
            // capsule free-falls forever, `grounded` never true) and on
            // Oblivion's ICMarketDistrictTheGildedCarafe it wedges against
            // a sloped surface a few BU below door height (confirmed via a
            // one-off downward-cast probe: the resting contact normal's
            // dot-up was -0.44 — a slanted surface, not a floor — so the
            // character sits blocked but ungrounded rather than free-
            // falling). Re-verify the nudged XZ against the actual
            // architecture with a capsule-shaped downward probe (a bare
            // ray can slip through gaps beside sloped/narrow geometry the
            // KCC's own capsule would still clip) instead of trusting door
            // height blindly.
            //
            // The probe starts a modest margin above the DOOR's own
            // height, not the whole cell's AABB ceiling — starting from
            // the ceiling picks up whatever clutter (shelves, beams,
            // upper floors) happens to sit anywhere above the nudged XZ,
            // which is *not* the floor the door actually opens onto.
            // Doors sit at floor level by construction, so a bounded
            // search near that height finds the real local floor (or
            // correctly reports nothing nearby, rather than a false hit
            // far above).
            // #2858 — the sweep must START above the floor it is looking
            // for. The probe capsule's own half-extent is
            // `half_height + radius` (64 BU for HUMAN), so the previous
            // fixed `+50.0` origin put the capsule's BOTTOM 14 BU *below*
            // door height. Every floor within ~15 BU of the door — i.e. the
            // door's own floor, which is the normal case given "doors sit at
            // floor level by construction" — was then either an
            // initial-penetration configuration (discarded by
            // `stop_at_penetration: false`, or reported at
            // `time_of_impact = 0` with a degenerate normal the walkable
            // filter rejects) or simply outside the swept half-space. Rungs 1
            // and 2 could therefore never answer on an ordinary flat
            // threshold, and every door spawn silently degraded to the rung-3
            // ceiling sweep this code documents as unreliable — which also
            // made the `floor_rung` telemetry mis-attribute the cause.
            //
            // Derive the lift from the capsule rather than hard-coding it, so
            // a `CharacterController` re-tune cannot reintroduce the blind
            // zone, and extend the range by the same amount so the band
            // searched BELOW the door is unchanged.
            // No player body exists yet on this path — the capsule is spawned
            // later in `setup_scene` — so there is nothing to self-hit.
            let near_door_floor_y =
                probe_walkable_floor_near(world, nudged_x, nudged_z, door_pos.y, cc, None);
            // Second rung — the nudge itself can be the problem. It moves
            // the XZ 64 BU into the room, and when that lands over a hole
            // (Skyrim's `BleakFallsBarrow01`: the nudged XZ has no floor
            // anywhere near door height) the threshold the door actually
            // opens onto is still solid. Doors sit at floor level by
            // construction — the same premise rung 1 relies on — so probe
            // the UN-nudged door XZ before resorting to a deep sweep.
            // Standing on the threshold is a better spawn than the bottom
            // of whatever shaft the nudge happened to point at.
            let door_xz_floor_y = near_door_floor_y.or_else(|| {
                probe_walkable_floor_near(world, door_pos.x, door_pos.z, door_pos.y, cc, None)
            });

            // Third rung — neither XZ has floor near door height, so the
            // room genuinely isn't flat here (stairwell gap, balcony edge,
            // multi-level drop). Sweep the cell's whole vertical extent.
            //
            // #2013 follow-up: this rung previously could not reach the
            // cell floor at all. It destructured the AABB as `(_, max, _)`,
            // discarding `min`, and substituted `door_pos.y` for the cell's
            // bottom — so from an origin of `max.y + 50` it travelled only
            // `(max.y - door_pos.y) + 150`, terminating 100 BU BELOW THE
            // DOOR rather than below the cell. On `BleakFallsBarrow01` that
            // is y=412 against colliders reaching y=-4514: the sweep
            // covered 100 of the 5026 BU beneath the spawn and reported a
            // miss, and the character free-fell out of the world from door
            // height with every asset loaded (black screen, 0 draws). Now
            // spans `max.y - min.y` like the no-door ray-cast branch below,
            // which had this right all along.
            let wide_floor_y = door_xz_floor_y.or_else(|| {
                aabb.and_then(|(min, max, _)| {
                    let pw = world.resource::<byroredux_physics::PhysicsWorld>();
                    // Same capsule-half-extent clearance as rungs 1 and 2
                    // (#2858), so a floor sitting near the cell's own AABB
                    // ceiling is inside the swept band rather than behind
                    // the probe's starting penetration.
                    let probe_lift = floor_probe_lift(cc);
                    let probe_origin = Vec3::new(nudged_x, max[1] + probe_lift, nudged_z);
                    let max_distance = (max[1] - min[1]).max(1.0) + probe_lift + 100.0;
                    pw.cast_capsule_down_onto_walkable_surface(
                        probe_origin,
                        cc.half_height,
                        cc.radius,
                        max_distance,
                        min_walkable_normal_y(cc),
                        None,
                    )
                })
            });
            let floor_y = wide_floor_y;
            // Which rung answered, so a bad spawn names its own cause in one
            // run instead of needing a bisect. Derived by comparison rather
            // than threaded through the `or_else` chain to keep the ladder
            // readable.
            let floor_rung = if near_door_floor_y.is_some() {
                "nudged XZ near door height"
            } else if door_xz_floor_y.is_some() {
                "door XZ near door height (nudge landed over a hole)"
            } else if wide_floor_y.is_some() {
                "full-cell sweep at nudged XZ"
            } else {
                "none"
            };
            let spawn_y = character_spawn_center_y(world, floor_y.unwrap_or(door_pos.y), cc);
            // Rung 2 resolves the threshold's own floor, so the capsule must
            // stand on the threshold rather than at the nudged XZ it just
            // rejected as floor-less.
            let (spawn_x, spawn_z) = if near_door_floor_y.is_none() && door_xz_floor_y.is_some() {
                (door_pos.x, door_pos.z)
            } else {
                (nudged_x, nudged_z)
            };
            let spawn = Vec3::new(spawn_x, spawn_y, spawn_z);

            let nudge_degraded = inward_xz.is_none();
            let floor_probe_failed = floor_y.is_none();
            log::info!(
                "M28.5 spawn at door teleporter: door at ({:.1}, {:.1}, {:.1}); \
                 inward nudge ({:.1}, _, {:.1}) BU; floor probe {}; placing capsule \
                 at ({:.1}, {:.1}, {:.1}){}{}",
                door_pos.x,
                door_pos.y,
                door_pos.z,
                nudge.x,
                nudge.z,
                match floor_y {
                    Some(y) => format!("hit y={y:.1} via {floor_rung}"),
                    None => "MISS on all 3 rungs (rejecting door spawn)".to_string(),
                },
                spawn.x,
                spawn.y,
                spawn.z,
                if nudge_degraded {
                    " — NUDGE DEGRADED: no static colliders for AABB-centre \
                     computation; capsule will rest ON the door threshold and \
                     may project beyond a thin floor (#1295). If the character \
                     free-falls from this spawn, the root cause is missing \
                     static colliders, not the spawn position."
                } else {
                    ""
                },
                if floor_probe_failed {
                    " — FLOOR PROBE MISS: nudged XZ found no floor within range; \
                     rejecting this door and using the camera-ground fallback."
                } else {
                    ""
                },
            );
            // #2202 — all three rungs missed, so the column really is empty
            // to the probe. Census what IS there before the character starts
            // falling: the probe filters to non-Dynamic bodies and the
            // static-AABB sanity log counts only Fixed ones, so between them
            // an authored-Dynamic floor is invisible twice over and reads
            // identically to no floor at all. Only on the failure path — a
            // healthy spawn pays nothing.
            if floor_probe_failed {
                const SPAWN_CENSUS_RADIUS_BU: f32 = 256.0;
                // Mirror rung 1's geometry exactly so the census's unfiltered
                // re-sweep answers for the probe that actually failed (#2874),
                // and so the column ordering centres on the height the floor
                // was expected at rather than on the world's ceiling (#2875).
                let probe_lift = floor_probe_lift(cc);
                let authoring = world
                    .try_resource::<crate::cell_loader::NifImportRegistry>()
                    .map(|registry| registry.collision_authoring_totals());
                byroredux_physics::dump_spawn_collider_census(
                    world,
                    byroredux_physics::SpawnCensusProbe {
                        x: spawn.x,
                        y: door_pos.y + probe_lift,
                        z: spawn.z,
                        radius: SPAWN_CENSUS_RADIUS_BU,
                        capsule_half_height: cc.half_height,
                        capsule_radius: cc.radius,
                        max_distance: FLOOR_PROBE_CLEARANCE_BU + FLOOR_PROBE_REACH_BELOW_DOOR_BU,
                        min_walkable_normal_y: min_walkable_normal_y(cc),
                        authoring,
                    },
                );
            }
            if floor_probe_failed {
                spawn_on_camera_ground(world, cam_pos, cc, "door floor probe missed")
            } else {
                spawn
            }
        } else {
            spawn_on_camera_ground(world, cam_pos, cc, "no foreground DoorTeleport")
        };
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
        crate::inventory::attach_to_player(world, body);
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

    // UI: --swf <path> loads a SWF menu overlay.
    if let Some(swf_idx) = args.iter().position(|a| a == "--swf") {
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
                                    log::info!("UI texture registered (handle {})", handle);
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
mod nif_loader_light_tests;
#[cfg(test)]
mod procedural_fallback_tests;
#[cfg(test)]
mod radius_parse_tests;
#[cfg(test)]
mod spawn_tests;
