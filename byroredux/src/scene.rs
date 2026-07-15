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

// Test child modules (procedural_fallback_tests, climate_tod_hours_tests,
// cloud_tile_scale_tests) reach for these via `use super::*;` — keep
// them in scope under cfg(test). Production code reaches them through
// the `world_setup` submodule directly.
#[cfg(test)]
#[allow(unused_imports)]
use crate::components::{GameTimeRes, SkyParamsRes, WeatherDataRes};
// Re-exported for the EXAL boundary (`env_translate::translate_weather`)
// and the `climate_tod_hours_tests` child module.
pub(crate) use world_setup::climate_tod_hours;

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

mod world_setup;
// Re-export the streaming setup helpers so the M40 Phase 2 cell-
// transition orchestrator in `main.rs::App::step_cell_transition` can
// reuse them on Interior→Exterior swaps — same boot-path code, no
// duplication. See cell_loader::transition.
pub(crate) use world_setup::{apply_worldspace_weather, stream_initial_radius};
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
    let mut cam_center = Vec3::ZERO;
    let mut has_nif_content = false;
    let mut nif_root: Option<EntityId> = None;

    // Pending-cell-transition slot — pre-inserted so `&World`-only
    // trigger sites (`door.teleport` console command, M40 Phase 2
    // Stage 4 F-key activate) can write the queued transition via
    // `resource_mut` without structural insertion. The main loop's
    // per-frame `take_pending_transition` drains the slot back to
    // `None`. See cell_loader::transition.
    world.insert_resource(cell_loader::PendingCellTransitionSlot::default());

    // Cornell-box test harness (`--cornell`) — a self-contained RT
    // validation scene needing no on-disk game data. Takes precedence
    // over the ESM / NIF / demo paths. Returns the camera pose to use
    // (overridable by the usual `--camera-pos` / `--camera-forward`).
    let cornell = args.iter().any(|a| a == "--cornell");
    let mut cornell_cam: Option<(Vec3, Vec3)> = None;

    // Cell loading mode: --esm <path> --cell <editor_id> OR --wrld <name> --grid <x>,<y>
    if cornell {
        let (pos, target) = crate::cornell::setup_cornell_scene(world, ctx);
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
            // Initial-radius cells are loaded synchronously here so
            // the first rendered frame has a populated world.
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
                    has_nif_content = true;
                    apply_worldspace_weather(world, ctx, &tex_provider, &wctx);
                    let mut state =
                        WorldStreamingState::new(wctx, tex_provider, mat_provider, radius);
                    state.last_player_grid = Some((cx, cy));
                    cam_center = stream_initial_radius(world, ctx, &mut state, cx, cy);
                    log::info!(
                        "Streaming context ready: worldspace '{}', radius {} (load), {} (unload), {} cells loaded initially",
                        state.wctx.worldspace_key,
                        state.radius_load,
                        state.radius_unload,
                        state.loaded.len(),
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
    let player_mode = if want_fly {
        crate::systems::PlayerMode::FlyCam
    } else if want_player {
        crate::systems::PlayerMode::Character
    } else if cornell {
        // The Cornell box has no colliders; a character capsule would
        // fall through the floor. Fly-cam unless explicitly overridden.
        crate::systems::PlayerMode::FlyCam
    } else if has_nif_content {
        crate::systems::PlayerMode::Character
    } else {
        crate::systems::PlayerMode::FlyCam
    };
    world.insert_resource(player_mode);
    if player_mode == crate::systems::PlayerMode::FlyCam {
        log::info!(
            "Player rig: FlyCam (use `--player` to force Character mode without cell content)"
        );
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
        byroredux_physics::physics_sync_system(world, 0.0);
        // M28.5 spawn-fallback fix — `physics_sync_system` inserts the
        // colliders into `ColliderSet` but `QueryPipeline` only learns
        // about them as a side-effect of `pipeline.step()`. We haven't
        // stepped yet (dt=0), so the BVH is empty and `cast_ray_down`
        // returns `None` for every shot. Flush the BVH explicitly so
        // the spawn ray-cast sees the cell architecture.
        {
            let mut pw = world.resource_mut::<byroredux_physics::PhysicsWorld>();
            pw.update_query_pipeline();
        }

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
        let door_spawn = {
            let dq = world.query::<crate::components::DoorTeleport>();
            let tq = world.query::<Transform>();
            match (dq, tq) {
                (Some(dq), Some(tq)) => dq
                    .iter()
                    .find_map(|(entity, _door)| tq.get(entity).map(|t| t.translation)),
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
            // collider AABB centre so the capsule lands on architecture
            // every time, independent of door rotation conventions or
            // per-game subtleties. Y stays at door height — the door
            // floor IS the spawn floor.
            const INWARD_NUDGE_BU: f32 = 64.0;
            let inward_xz = {
                let pw = world.resource::<byroredux_physics::PhysicsWorld>();
                pw.static_colliders_aabb().and_then(|(min, max, _)| {
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
                })
            };
            let nudge = inward_xz.unwrap_or(Vec3::ZERO) * INWARD_NUDGE_BU;
            let spawn = Vec3::new(
                door_pos.x + nudge.x,
                door_pos.y + cc.half_height + 4.0,
                door_pos.z + nudge.z,
            );
            // Inward-nudge degradation diagnostic. #1295 — when the
            // cell has no static colliders (broken bhk extraction, or
            // pre-#1294 SF cells before the trimesh-fallback gate
            // landed), `static_colliders_aabb()` returns `None` →
            // inward_xz=None → nudge=ZERO → capsule lands on the
            // exact door-threshold position. The threshold often sits
            // on a thin floor reference 1-2 BU thick; without a
            // surrounding collider the capsule projects beyond it
            // and free-falls. Without this explicit warn the failure
            // mode reads as "door teleporter wasn't used" (the spawn
            // log shows the door position) when actually it WAS used
            // with degraded input. Cydonia 2026-05-28 first-render
            // hit exactly this trap; surfaced as filed-and-closed
            // #1295.
            let nudge_degraded = inward_xz.is_none();
            log::info!(
                "M28.5 spawn at door teleporter: door at ({:.1}, {:.1}, {:.1}); \
                 inward nudge ({:.1}, _, {:.1}) BU; placing capsule at \
                 ({:.1}, {:.1}, {:.1}){}",
                door_pos.x,
                door_pos.y,
                door_pos.z,
                nudge.x,
                nudge.z,
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
            );
            spawn
        } else {
            let pw = world.resource::<byroredux_physics::PhysicsWorld>();
            match pw.static_colliders_aabb() {
                Some((min, max, _)) => {
                    let aabb_height = (max[1] - min[1]).max(1.0);
                    // Start the ray ~50 BU above the top of the cell.
                    let ray_origin = Vec3::new(cam_pos.x, max[1] + 50.0, cam_pos.z);
                    // Look down through the entire cell + slack.
                    let max_distance = aabb_height + 100.0;
                    match pw.cast_ray_down(ray_origin, max_distance) {
                        Some(hit_y) => {
                            log::info!(
                                "M28.5 spawn ray-cast: hit floor at y={:.1} under \
                                 ({:.1}, {:.1}); placing capsule at y={:.1} (no \
                                 DoorTeleport in cell — fell through to ray-cast)",
                                hit_y,
                                cam_pos.x,
                                cam_pos.z,
                                hit_y + cc.half_height + 4.0,
                            );
                            Vec3::new(cam_pos.x, hit_y + cc.half_height + 4.0, cam_pos.z)
                        }
                        None => {
                            log::warn!(
                                "M28.5 spawn ray-cast: NO floor found under ({:.1}, \
                                 {:.1}) within {:.1} BU; falling back to \
                                 aabb.max.y + 200 (M28.5 spawn fallback)",
                                cam_pos.x,
                                cam_pos.z,
                                max_distance,
                            );
                            Vec3::new(cam_pos.x, max[1] + 200.0, cam_pos.z)
                        }
                    }
                }
                None => cam_pos - Vec3::Y * cc.eye_height,
            }
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
mod climate_tod_hours_tests;
#[cfg(test)]
mod cloud_tile_scale_tests;
#[cfg(test)]
mod procedural_fallback_tests;
#[cfg(test)]
mod radius_parse_tests;
