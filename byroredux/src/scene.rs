//! Scene setup and NIF loading logic.

use byroredux_core::animation::{AnimationClipRegistry, AnimationPlayer};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{
    ActiveCamera, Camera, GlobalTransform,
    MeshHandle, Transform,
    World,
};
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::StringPool;
use byroredux_renderer::{cube_vertices, quad_vertices, triangle_vertices, VulkanContext};
use byroredux_ui::UiManager;

use crate::anim_convert::convert_nif_clip;
use crate::asset_provider::{
    build_material_provider, build_texture_provider, parse_grid_coords,
};
use crate::cell_loader;
use crate::components::{CellLightingRes, InputState, Spinning};
use crate::streaming::WorldStreamingState;

// Test child modules (procedural_fallback_tests, climate_tod_hours_tests,
// cloud_tile_scale_tests) reach for these via `use super::*;` — keep
// them in scope under cfg(test). Production code reaches them through
// the `world_setup` submodule directly.
#[cfg(test)]
#[allow(unused_imports)]
use crate::components::{GameTimeRes, SkyParamsRes, WeatherDataRes};
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use world_setup::climate_tod_hours;

/// Parse the `--radius` CLI argument into a clamped grid radius for
/// [`cell_loader::load_exterior_cells`]. Falls back to `3` (7×7 = 49
/// cells, ~28K terrain units view distance) on any parse failure so
/// an unparseable value loads the default rather than silently
/// bailing. Clamped to `1..=7` — below 1 the center cell alone isn't
/// useful, above 7 the cell count (15×15 = 225) already exceeds the
/// streaming budget today.
///
/// Pulled out as a free function so a unit test can pin the bounds
/// contract without standing up a whole App / World. See #531.
pub(crate) fn parse_exterior_radius(s: &str) -> i32 {
    match s.trim().parse::<i32>() {
        Ok(r) => r.clamp(1, 7),
        Err(_) => 3,
    }
}


mod world_setup;
use world_setup::{apply_worldspace_weather, stream_initial_radius};
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
    let args: Vec<String> = std::env::args().collect();
    let mut cam_center = Vec3::ZERO;
    let mut has_nif_content = false;
    let mut nif_root: Option<EntityId> = None;

    // Cell loading mode: --esm <path> --cell <editor_id> OR --wrld <name> --grid <x>,<y>
    if let Some(esm_idx) = args.iter().position(|a| a == "--esm") {
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
        // Defaults to 3 (7×7 grid, ~28K terrain units view distance)
        // to preserve pre-fix behaviour. Clamped to 1..=7 by
        // [`parse_exterior_radius`] so an accidental 100 doesn't try
        // to load 40 401 cells.
        let radius = args
            .iter()
            .position(|a| a == "--radius")
            .and_then(|i| args.get(i + 1))
            .map(|s| parse_exterior_radius(s))
            .unwrap_or(3);

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
                    // Store cell lighting for the renderer.
                    if let Some(ref lit) = result.lighting {
                        let (rx, ry) = (lit.directional_rotation[0], lit.directional_rotation[1]);
                        // Route the authored XCLL Euler angles through
                        // `euler_zup_to_quat_yup` — the same
                        // CW-convention helper REFR placements use —
                        // then apply the resulting Y-up quaternion to
                        // Gamebryo's NiDirectionalLight model
                        // direction `(1, 0, 0)` (per the 2.3
                        // `NiDirectionalLight.h` comment: "The model
                        // direction of the light is (1,0,0)"). The
                        // Z-up → Y-up coord swap leaves +X invariant,
                        // so the Y-up model vector is also `(1, 0, 0)`.
                        // Pre-#380 an inline spherical formula treated
                        // ry as elevation-from-horizon and drifted
                        // from the authored intent as ry grew. See
                        // audit F3-09.
                        let quat = cell_loader::euler_zup_to_quat_yup(rx, ry, 0.0);
                        let dir_v = quat * Vec3::new(1.0, 0.0, 0.0);
                        let dir = [dir_v.x, dir_v.y, dir_v.z];
                        // load_cell() only handles interior cells —
                        // `is_interior: true` skips the directional as
                        // a scene light to prevent wall light leakage.
                        // The 9 extended XCLL fields (`fog_clip`,
                        // `directional_ambient`, etc.) are propagated
                        // by `from_cell_lighting` even though the
                        // renderer doesn't yet consume them — #861
                        // establishes the data plumbing; #865 + a
                        // future Skyrim ambient-cube uniform are the
                        // shader-side follow-ups.
                        world.insert_resource(CellLightingRes::from_cell_lighting(lit, dir, true));
                        log::info!(
                            "Cell lighting: ambient={:?} directional={:?} dir={:?} fog={:?} near={:.0} far={:.0}",
                            lit.ambient,
                            lit.directional_color,
                            dir,
                            lit.fog_color,
                            lit.fog_near,
                            lit.fog_far,
                        );
                    }
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
        let cube_handle = ctx
            .mesh_registry
            .upload(&ctx.device, alloc, queue, pool, &verts, &idxs, rt, None)
            .expect("Failed to upload cube mesh");

        let (quad_verts, quad_idxs) = quad_vertices();
        let quad_handle = ctx
            .mesh_registry
            .upload(
                &ctx.device,
                alloc,
                queue,
                pool,
                &quad_verts,
                &quad_idxs,
                rt,
                None,
            )
            .expect("Failed to upload quad mesh");

        let (red_verts, red_idxs) = triangle_vertices([1.0, 0.2, 0.2]);
        let red_handle = ctx
            .mesh_registry
            .upload(
                &ctx.device,
                alloc,
                queue,
                pool,
                &red_verts,
                &red_idxs,
                rt,
                None,
            )
            .expect("Failed to upload red triangle mesh");

        let (blue_verts, blue_idxs) = triangle_vertices([0.2, 0.2, 1.0]);
        let blue_handle = ctx
            .mesh_registry
            .upload(
                &ctx.device,
                alloc,
                queue,
                pool,
                &blue_verts,
                &blue_idxs,
                rt,
                None,
            )
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
    let cam_pos = match camera_pos_override {
        Some((x, y, z)) => Vec3::new(x, y, z),
        None if has_nif_content => cam_center + Vec3::new(0.0, 100.0, 200.0),
        None => Vec3::new(0.0, 1.5, 4.0),
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

    // NOTE: M28 Phase 1 attached a `PlayerBody::HUMAN` capsule to the
    // camera so the fly cam would collide with world geometry. That
    // path doesn't actually work as a camera rig — physics_sync_system
    // Phase 4 clobbers the rotation the fly camera writes each frame
    // (locking the view to the body's initial yaw), and setting linvel
    // directly overrides gravity so the player can't fall. A proper
    // kinematic character controller lands in M28.5; until then, the
    // fly camera stays free-fly and physics runs only on world +
    // clutter bodies spawned by the cell loader.

    // Initialize fly camera yaw/pitch from the initial look direction.
    {
        let mut input = world.resource_mut::<InputState>();
        input.yaw = forward.x.atan2(-forward.z);
        input.pitch = forward.y.asin();
    }

    // Build the global geometry SSBO for RT reflection ray UV lookups.
    if let Err(e) = ctx.mesh_registry.build_geometry_ssbo(
        &ctx.device,
        ctx.allocator.as_ref().unwrap(),
        &ctx.graphics_queue,
        ctx.transfer_pool,
        None, // TODO: thread StagingPool through scene load (#242)
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
                            match ctx.texture_registry.register_rgba(
                                &ctx.device,
                                allocator,
                                &ctx.graphics_queue,
                                ctx.transfer_pool,
                                w,
                                h,
                                &pixels,
                            ) {
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
pub(crate) use nif_loader::load_nif_bytes_with_skeleton;
use nif_loader::load_nif_from_args;

#[cfg(test)]
mod radius_parse_tests;
#[cfg(test)]
mod cloud_tile_scale_tests;
#[cfg(test)]
mod procedural_fallback_tests;
#[cfg(test)]
mod climate_tod_hours_tests;
