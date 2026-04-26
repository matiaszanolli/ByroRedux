//! Scene setup and NIF loading logic.

use byroredux_core::animation::{AnimationClipRegistry, AnimationPlayer};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{
    ActiveCamera, Billboard, BillboardMode, Camera, GlobalTransform, LocalBound, Material,
    MeshHandle, Name, Parent, ParticleEmitter, SceneFlags, SkinnedMesh, TextureHandle, Transform,
    World, WorldBound, MAX_BONES_PER_MESH,
};
use byroredux_core::math::{Mat4, Quat, Vec3};
use byroredux_core::string::StringPool;
use byroredux_renderer::{cube_vertices, quad_vertices, triangle_vertices, Vertex, VulkanContext};
use byroredux_ui::UiManager;

use crate::anim_convert::convert_nif_clip;
use crate::asset_provider::{
    build_material_provider, build_texture_provider, merge_bgsm_into_mesh, parse_grid_coords,
    resolve_texture, MaterialProvider, TextureProvider,
};
use crate::cell_loader;
use crate::components::{
    AlphaBlend, CellLightingRes, DarkMapHandle, Decal, ExtraTextureMaps, GameTimeRes, InputState,
    NormalMapHandle, SkyParamsRes, Spinning, TwoSided, WeatherDataRes, WeatherTransitionRes,
};
use crate::helpers::add_child;

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

/// Called once after the renderer is ready — uploads meshes and spawns entities.
pub(crate) fn setup_scene(
    world: &mut World,
    ctx: &mut VulkanContext,
    ui_manager: &mut Option<UiManager>,
    ui_texture_handle: &mut Option<u32>,
    camera_pos_override: Option<(f32, f32, f32)>,
    camera_forward_override: Option<(f32, f32, f32)>,
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
                        world.insert_resource(CellLightingRes {
                            ambient: lit.ambient,
                            directional_color: lit.directional_color,
                            directional_dir: dir,
                            // load_cell() only handles interior cells —
                            // the directional will be skipped as a scene
                            // light to prevent wall light leakage.
                            is_interior: true,
                            fog_color: lit.fog_color,
                            fog_near: lit.fog_near,
                            fog_far: lit.fog_far,
                        });
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
            // Exterior cell mode: --esm <path> --grid <x>,<y>
            let (cx, cy) = parse_grid_coords(grid);
            let tex_provider = build_texture_provider(&args);
            let mut mat_provider = build_material_provider(&args);
            // Radius defaults to 3 (7×7 = 49 cells, ~28K terrain
            // units view distance). Overridable via `--radius N`,
            // clamped to 1..=7 so worst-case is a 15×15 = 225 cell
            // stress load. See #531.
            match cell_loader::load_exterior_cells_with_masters(
                &masters,
                esm_path,
                cx,
                cy,
                radius,
                world,
                ctx,
                &tex_provider,
                Some(&mut mat_provider),
                wrld_name.as_deref(),
            ) {
                Ok(result) => {
                    cam_center = result.center;
                    has_nif_content = true;
                    log::info!(
                        "Exterior '{}' ready: {} entities",
                        result.cell_name,
                        result.entity_count
                    );
                    // Exterior cells: set up lighting + sky from WTHR data
                    // or a procedural Mojave-style fallback.
                    let sun_dir: [f32; 3] = [-0.4, 0.8, -0.45];
                    if let Some(ref wthr) = result.weather {
                        use byroredux_plugin::esm::records::weather::*;
                        // Extract "day" time slot colors (initial state).
                        // Raw monitor-space per commit 0e8efc6 — matches XCLL / LIGH /
                        // NIF material policy. sRGB decode would darken every warm hue.
                        let ambient = wthr.sky_colors[SKY_AMBIENT][TOD_DAY].to_rgb_f32();
                        let sunlight = wthr.sky_colors[SKY_SUNLIGHT][TOD_DAY].to_rgb_f32();
                        let fog_col = wthr.sky_colors[SKY_FOG][TOD_DAY].to_rgb_f32();
                        let zenith = wthr.sky_colors[SKY_UPPER][TOD_DAY].to_rgb_f32();
                        let horizon = wthr.sky_colors[SKY_HORIZON][TOD_DAY].to_rgb_f32();
                        let sun_col = wthr.sky_colors[SKY_SUN][TOD_DAY].to_rgb_f32();
                        log::info!(
                            "WTHR '{}': zenith={:?} horizon={:?} sun={:?} fog_day={:.0}–{:.0}",
                            wthr.editor_id,
                            zenith,
                            horizon,
                            sun_col,
                            wthr.fog_day_near,
                            wthr.fog_day_far,
                        );
                        world.insert_resource(CellLightingRes {
                            ambient,
                            directional_color: sunlight,
                            directional_dir: sun_dir,
                            is_interior: false,
                            fog_color: fog_col,
                            fog_near: wthr.fog_day_near,
                            fog_far: wthr.fog_day_far,
                        });
                        // Resolve WTHR cloud layer 0 through the texture provider.
                        // On failure (no path / archive miss / corrupt DDS) we keep
                        // cloud rendering disabled rather than falling back to the
                        // checkerboard — a magenta sky dome is worse than no clouds.
                        //
                        // Path normalization (textures\ prefix) happens inside
                        // `TextureProvider::extract` — authored WTHR cloud paths
                        // are `textures\`-root-relative (`sky\cloudsnoon.dds`)
                        // but the BSA layer stores them with the full prefix.
                        // See #468.
                        let (cloud_tex_index, cloud_tile_scale) = match wthr.cloud_textures[0]
                            .as_deref()
                        {
                            Some(path) => match tex_provider.extract(path) {
                                Some(dds_bytes) => {
                                    let alloc = ctx.allocator.as_ref().unwrap();
                                    match ctx.texture_registry.load_dds(
                                        &ctx.device,
                                        alloc,
                                        &ctx.graphics_queue,
                                        ctx.transfer_pool,
                                        path,
                                        &dds_bytes,
                                    ) {
                                        Ok(h) => {
                                            log::info!("Cloud texture '{}' → handle {}", path, h);
                                            // Tile scale 0.15 spreads one texture
                                            // over ~6.7 view-direction units above
                                            // the horizon — looks right at typical
                                            // Bethesda 512² cloud authoring.
                                            (h, 0.15_f32)
                                        }
                                        Err(e) => {
                                            log::warn!(
                                                "Cloud DDS load failed '{}': {} — disabling clouds",
                                                path,
                                                e
                                            );
                                            (0u32, 0.0_f32)
                                        }
                                    }
                                }
                                None => {
                                    log::debug!("Cloud texture '{}' not in archives", path);
                                    (0u32, 0.0_f32)
                                }
                            },
                            None => (0u32, 0.0_f32),
                        };
                        // Resolve WTHR cloud layer 1 (CNAM). Same pattern as layer 0.
                        // Tile scale 0.20 (slightly higher than layer 0's 0.15) so
                        // layer 1 visually reads as a higher-altitude, finer-grained
                        // cloud deck — most obvious when both layers are visible
                        // simultaneously. Disabled (0.0) when CNAM is absent.
                        let (cloud_tex_index_1, cloud_tile_scale_1) =
                            match wthr.cloud_textures[1].as_deref() {
                                Some(path) => match tex_provider.extract(path) {
                                    Some(dds_bytes) => {
                                        let alloc = ctx.allocator.as_ref().unwrap();
                                        match ctx.texture_registry.load_dds(
                                            &ctx.device,
                                            alloc,
                                            &ctx.graphics_queue,
                                            ctx.transfer_pool,
                                            path,
                                            &dds_bytes,
                                        ) {
                                            Ok(h) => {
                                                log::info!(
                                                    "Cloud layer 1 texture '{}' → handle {}",
                                                    path,
                                                    h
                                                );
                                                (h, 0.20_f32)
                                            }
                                            Err(e) => {
                                                log::warn!(
                                                    "Cloud layer 1 DDS load failed '{}': {} — disabling layer 1",
                                                    path,
                                                    e
                                                );
                                                (0u32, 0.0_f32)
                                            }
                                        }
                                    }
                                    None => {
                                        log::debug!(
                                            "Cloud layer 1 texture '{}' not in archives",
                                            path
                                        );
                                        (0u32, 0.0_f32)
                                    }
                                },
                                None => (0u32, 0.0_f32),
                            };
                        // Resolve WTHR cloud layer 2 (ANAM). Tile scale 0.25 —
                        // higher altitude than layers 0/1, finer-grained. (M33.1)
                        let (cloud_tex_index_2, cloud_tile_scale_2) =
                            match wthr.cloud_textures[2].as_deref() {
                                Some(path) => match tex_provider.extract(path) {
                                    Some(dds_bytes) => {
                                        let alloc = ctx.allocator.as_ref().unwrap();
                                        match ctx.texture_registry.load_dds(
                                            &ctx.device,
                                            alloc,
                                            &ctx.graphics_queue,
                                            ctx.transfer_pool,
                                            path,
                                            &dds_bytes,
                                        ) {
                                            Ok(h) => (h, 0.25_f32),
                                            Err(_) => (0u32, 0.0_f32),
                                        }
                                    }
                                    None => (0u32, 0.0_f32),
                                },
                                None => (0u32, 0.0_f32),
                            };
                        // Resolve WTHR cloud layer 3 (BNAM). Tile scale 0.30 —
                        // topmost, finest-grained cirrus-style layer. (M33.1)
                        let (cloud_tex_index_3, cloud_tile_scale_3) =
                            match wthr.cloud_textures[3].as_deref() {
                                Some(path) => match tex_provider.extract(path) {
                                    Some(dds_bytes) => {
                                        let alloc = ctx.allocator.as_ref().unwrap();
                                        match ctx.texture_registry.load_dds(
                                            &ctx.device,
                                            alloc,
                                            &ctx.graphics_queue,
                                            ctx.transfer_pool,
                                            path,
                                            &dds_bytes,
                                        ) {
                                            Ok(h) => (h, 0.30_f32),
                                            Err(_) => (0u32, 0.0_f32),
                                        }
                                    }
                                    None => (0u32, 0.0_f32),
                                },
                                None => (0u32, 0.0_f32),
                            };
                        // CLMT FNAM sun-sprite resolution — #478.
                        // Same extract-then-load-DDS pattern as the
                        // cloud texture above; path normalization
                        // (`textures\` prefix + slash flip) happens
                        // inside `TextureProvider::extract` and
                        // `TextureRegistry::load_dds` (see #522).
                        // `0` = fall back to the procedural disc in
                        // composite.frag (every code path that hit
                        // the sun rendering pre-#478 continues to).
                        let sun_tex_index: u32 = result
                            .climate
                            .as_ref()
                            .and_then(|c| c.sun_texture.as_deref())
                            .filter(|s| !s.is_empty())
                            .and_then(|path| {
                                let dds = tex_provider.extract(path)?;
                                let alloc = ctx.allocator.as_ref().unwrap();
                                match ctx.texture_registry.load_dds(
                                    &ctx.device,
                                    alloc,
                                    &ctx.graphics_queue,
                                    ctx.transfer_pool,
                                    path,
                                    &dds,
                                ) {
                                    Ok(h) => {
                                        log::info!("Sun texture '{}' → handle {}", path, h);
                                        Some(h)
                                    }
                                    Err(e) => {
                                        log::warn!(
                                            "Sun DDS load failed '{}': {} — using procedural disc",
                                            path,
                                            e
                                        );
                                        None
                                    }
                                }
                            })
                            .unwrap_or(0);
                        world.insert_resource(SkyParamsRes {
                            zenith_color: zenith,
                            horizon_color: horizon,
                            sun_direction: sun_dir,
                            sun_color: sun_col,
                            sun_size: 0.9995,
                            sun_intensity: 4.0,
                            is_exterior: true,
                            cloud_scroll: [0.0, 0.0],
                            cloud_tile_scale,
                            cloud_texture_index: cloud_tex_index,
                            sun_texture_index: sun_tex_index,
                            cloud_scroll_1: [0.0, 0.0],
                            cloud_tile_scale_1,
                            cloud_texture_index_1: cloud_tex_index_1,
                            cloud_scroll_2: [0.0, 0.0],
                            cloud_tile_scale_2,
                            cloud_texture_index_2: cloud_tex_index_2,
                            cloud_scroll_3: [0.0, 0.0],
                            cloud_tile_scale_3,
                            cloud_texture_index_3: cloud_tex_index_3,
                        });
                        // Store full NAM0 color table for per-frame time-of-day interpolation.
                        let mut sky_colors = [[[0.0f32; 3]; 6]; 10];
                        for g in 0..SKY_COLOR_GROUPS {
                            for s in 0..SKY_TIME_SLOTS {
                                sky_colors[g][s] = wthr.sky_colors[g][s].to_rgb_f32();
                            }
                        }
                        // #463 — Per-climate sunrise/sunset breakpoints.
                        // CLMT TNAM bytes are in 10-minute units →
                        // divide by 6 for hours. Fall back to the pre-
                        // #463 hardcoded values (6h/10h/18h/22h) when
                        // the cell has no climate record (or all bytes
                        // are zero, which happens on stubbed test data).
                        let tod_hours = result
                            .climate
                            .as_ref()
                            .filter(|c| {
                                c.sunrise_begin | c.sunrise_end
                                    | c.sunset_begin | c.sunset_end
                                    != 0
                            })
                            .map(|c| {
                                [
                                    c.sunrise_begin as f32 / 6.0,
                                    c.sunrise_end as f32 / 6.0,
                                    c.sunset_begin as f32 / 6.0,
                                    c.sunset_end as f32 / 6.0,
                                ]
                            })
                            .unwrap_or([6.0, 10.0, 18.0, 22.0]);
                        let new_weather = WeatherDataRes {
                            sky_colors,
                            fog: [
                                wthr.fog_day_near,
                                wthr.fog_day_far,
                                wthr.fog_night_near,
                                wthr.fog_night_far,
                            ],
                            tod_hours,
                        };
                        // If a previous weather is already live, fade into the
                        // new one over 8 seconds rather than snapping. On first
                        // load there is no previous weather so we insert directly.
                        if world.try_resource::<WeatherDataRes>().is_some() {
                            world.insert_resource(WeatherTransitionRes {
                                target: new_weather,
                                elapsed_secs: 0.0,
                                duration_secs: 8.0,
                            });
                        } else {
                            world.insert_resource(new_weather);
                            world.insert_resource(GameTimeRes::default());
                        }
                    } else {
                        // Procedural fallback — warm Mojave desert sky.
                        world.insert_resource(CellLightingRes {
                            ambient: [0.15, 0.14, 0.12],
                            directional_color: [1.0, 0.95, 0.8],
                            directional_dir: sun_dir,
                            is_interior: false,
                            fog_color: [0.65, 0.7, 0.8],
                            fog_near: 15000.0,
                            fog_far: 80000.0,
                        });
                        world.insert_resource(SkyParamsRes {
                            zenith_color: [0.15, 0.3, 0.65],
                            horizon_color: [0.55, 0.5, 0.42],
                            sun_direction: sun_dir,
                            sun_color: [1.0, 0.95, 0.8],
                            sun_size: 0.9995,
                            sun_intensity: 4.0,
                            is_exterior: true,
                            cloud_scroll: [0.0, 0.0],
                            cloud_tile_scale: 0.0, // no WTHR → no clouds
                            cloud_texture_index: 0,
                            sun_texture_index: 0, // procedural disc (#478)
                            cloud_scroll_1: [0.0, 0.0],
                            cloud_tile_scale_1: 0.0,
                            cloud_texture_index_1: 0,
                            cloud_scroll_2: [0.0, 0.0],
                            cloud_tile_scale_2: 0.0,
                            cloud_texture_index_2: 0,
                            cloud_scroll_3: [0.0, 0.0],
                            cloud_tile_scale_3: 0.0,
                            cloud_texture_index_3: 0,
                        });
                    }
                }
                Err(e) => log::error!("Failed to load exterior cells: {:#}", e),
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

/// Parse CLI arguments and load NIF data accordingly.
///
/// Supported flags:
///   `cargo run -- path/to/file.nif` — loose NIF file
///   `cargo run -- --bsa meshes.bsa --mesh meshes\foo.nif` — extract from BSA
///   `cargo run -- --bsa meshes.bsa --mesh meshes\foo.nif --textures-bsa textures.bsa`
fn load_nif_from_args(world: &mut World, ctx: &mut VulkanContext) -> (usize, Option<EntityId>) {
    let args: Vec<String> = std::env::args().collect();

    // Collect BSA/BA2 archives (auto-detects format).
    let tex_provider = build_texture_provider(&args);
    let mut mat_provider = build_material_provider(&args);

    if let Some(bsa_idx) = args.iter().position(|a| a == "--bsa") {
        // BSA mode: --bsa <archive> --mesh <path_in_archive>
        let bsa_path = match args.get(bsa_idx + 1) {
            Some(p) => p,
            None => {
                log::error!("--bsa requires an archive path");
                return (0, None);
            }
        };
        let mesh_path = match args
            .iter()
            .position(|a| a == "--mesh")
            .and_then(|i| args.get(i + 1))
        {
            Some(p) => p,
            None => {
                log::error!("--bsa requires --mesh <path>");
                return (0, None);
            }
        };

        let archive = match byroredux_bsa::BsaArchive::open(bsa_path) {
            Ok(a) => a,
            Err(e) => {
                log::error!("Failed to open BSA '{}': {}", bsa_path, e);
                return (0, None);
            }
        };
        let data = match archive.extract(mesh_path) {
            Ok(d) => d,
            Err(e) => {
                log::error!("Failed to extract '{}': {}", mesh_path, e);
                return (0, None);
            }
        };
        log::info!("Extracted {} bytes from BSA '{}'", data.len(), mesh_path);
        load_nif_bytes(
            world,
            ctx,
            &data,
            mesh_path,
            &tex_provider,
            Some(&mut mat_provider),
        )
    } else if let Some(nif_path) = args.get(1) {
        if nif_path.starts_with("--") {
            return (0, None); // Skip flags that aren't NIF paths
        }
        // Loose file mode: <path.nif>
        let data = match std::fs::read(nif_path) {
            Ok(d) => d,
            Err(e) => {
                log::error!("Failed to read NIF file '{}': {}", nif_path, e);
                return (0, None);
            }
        };
        load_nif_bytes(
            world,
            ctx,
            &data,
            nif_path,
            &tex_provider,
            Some(&mut mat_provider),
        )
    } else {
        (0, None)
    }
}

/// Parse NIF bytes, import meshes with hierarchy, upload to GPU, and spawn ECS entities.
/// Returns (entity_count, root_entity).
pub(crate) fn load_nif_bytes(
    world: &mut World,
    ctx: &mut VulkanContext,
    data: &[u8],
    label: &str,
    tex_provider: &TextureProvider,
    mat_provider: Option<&mut MaterialProvider>,
) -> (usize, Option<EntityId>) {
    let scene = match byroredux_nif::parse_nif(data) {
        Ok(s) => s,
        Err(e) => {
            log::error!("Failed to parse NIF '{}': {}", label, e);
            return (0, None);
        }
    };

    let mut imported = byroredux_nif::import::import_nif_scene(&scene);
    // FO4+ external material resolution (#493). NIF fields take precedence;
    // only empty slots fill in from the resolved BGSM/BGEM chain.
    if let Some(provider) = mat_provider {
        for mesh in &mut imported.meshes {
            merge_bgsm_into_mesh(mesh, provider);
        }
    }

    // Phase 1: Spawn node entities (NiNode hierarchy).
    // node_index → EntityId mapping.
    // Also build a name → EntityId map so Phase 3 can resolve skinning
    // bone names to the entities they should drive. Skeleton nodes are
    // the only entities with unique names in a typical NIF, so collisions
    // (multiple nodes sharing a name) are rare; on collision we keep the
    // first spawn (root-most in depth-first order).
    let mut node_entities: Vec<EntityId> = Vec::with_capacity(imported.nodes.len());
    let mut node_by_name: std::collections::HashMap<std::sync::Arc<str>, EntityId> =
        std::collections::HashMap::with_capacity(imported.nodes.len());
    for node in &imported.nodes {
        let quat = Quat::from_xyzw(
            node.rotation[0],
            node.rotation[1],
            node.rotation[2],
            node.rotation[3],
        );
        let translation = Vec3::new(
            node.translation[0],
            node.translation[1],
            node.translation[2],
        );

        let entity = world.spawn();
        world.insert(entity, Transform::new(translation, quat, node.scale));
        world.insert(entity, GlobalTransform::IDENTITY);

        if let Some(ref name) = node.name {
            let mut pool = world.resource_mut::<StringPool>();
            let sym = pool.intern(name);
            drop(pool);
            world.insert(entity, Name(sym));
            node_by_name.entry(name.clone()).or_insert(entity);
        }

        // Attach collision data if present.
        if let Some((ref shape, ref body)) = node.collision {
            log::info!(
                "Collision attached to '{}': {:?} motion={:?} mass={:.1}",
                node.name.as_deref().unwrap_or("?"),
                std::mem::discriminant(shape),
                body.motion_type,
                body.mass,
            );
            world.insert(entity, shape.clone());
            world.insert(entity, body.clone());
        }

        // Attach Billboard component for NiBillboardNode-derived entities.
        // See #225 — nif import normalizes pre/post 10.1.0.0 mode layouts
        // into a single u16 before we map it to BillboardMode.
        if let Some(raw) = node.billboard_mode {
            world.insert(entity, Billboard::new(BillboardMode::from_nif(raw)));
        }

        // Attach raw NiAVObject flags so gameplay systems can branch on
        // DISABLE_SORTING, SELECTIVE_UPDATE, IS_NODE, DISPLAY_OBJECT,
        // etc. without re-reading the source NIF. APP_CULLED (bit 0) is
        // already consumed by the import-time visibility filter in
        // `walk.rs`, so every spawned node arrives with that bit clear.
        // We still emit the component unconditionally (not gated on
        // `flags != 0`) so a future toggle-visible system can just flip
        // the bit on the existing component. See #222.
        if node.flags != 0 {
            world.insert(entity, SceneFlags::from_nif(node.flags));
        }

        node_entities.push(entity);
    }

    // Phase 2: Set up Parent/Children relationships for nodes.
    for (node_idx, node) in imported.nodes.iter().enumerate() {
        if let Some(parent_idx) = node.parent_node {
            let child_entity = node_entities[node_idx];
            let parent_entity = node_entities[parent_idx];
            world.insert(child_entity, Parent(parent_entity));
            add_child(world, parent_entity, child_entity);
        }
    }

    // Phase 2.5: Particle emitters. The NIF importer surfaces every
    // NiParticleSystem / NiParticles / NiBSPArrayController as an
    // [`ImportedParticleEmitter`] tagged with its host node index, but
    // it doesn't carry per-emitter values — `NiPSysBlock` discards
    // every parsed field. We pick a heuristic ParticleEmitter preset
    // (torch_flame / smoke / magic_sparkles / generic flame fallback)
    // by scanning the host node's name. Every emitter is attached
    // directly to the host entity so the simulation sources its
    // world-space spawn origin from the host's GlobalTransform. See
    // #401 / audit OBL-D6-2.
    for emitter in &imported.particle_emitters {
        let Some(host_idx) = emitter.parent_node else {
            continue;
        };
        let Some(&host_entity) = node_entities.get(host_idx) else {
            continue;
        };
        let host_name = imported.nodes[host_idx]
            .name
            .as_deref()
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        // Embers / sparks check FIRST so a node like "FireSparks" lands
        // on the bright-glint preset rather than the larger torch flame
        // (the `fire` substring would otherwise win).
        let preset = if host_name.contains("spark")
            || host_name.contains("ember")
            || host_name.contains("cinder")
        {
            ParticleEmitter::embers()
        } else if host_name.contains("torch")
            || host_name.contains("fire")
            || host_name.contains("flame")
            || host_name.contains("brazier")
            || host_name.contains("candle")
        {
            ParticleEmitter::torch_flame()
        } else if host_name.contains("smoke")
            || host_name.contains("steam")
            || host_name.contains("ash")
        {
            ParticleEmitter::smoke()
        } else if host_name.contains("magic")
            || host_name.contains("enchant")
            || host_name.contains("sparkle")
            || host_name.contains("glow")
        {
            ParticleEmitter::magic_sparkles()
        } else {
            // Fallback — many vanilla NIFs don't name the host node
            // descriptively (e.g. just "EmitterNode"). Default to a
            // visible flame so the audit's "every torch invisible"
            // failure is still resolved end-to-end.
            ParticleEmitter::torch_flame()
        };
        world.insert(host_entity, preset);
    }

    // Phase 3: Spawn mesh entities with parent links.
    let mut count = 0;
    let mut blas_specs: Vec<(u32, u32, u32)> = Vec::new();
    for mesh in &imported.meshes {
        let num_verts = mesh.positions.len();
        // Skinned vertices use the per-vertex bone indices + weights that
        // #151 / #177 extracted from NiSkinData / BSTriShape. Rigid
        // vertices pass zero weights and the shader's rigid-path routes
        // them through `pc.model` instead of the bone palette.
        let skin_vertex_data = mesh
            .skin
            .as_ref()
            .filter(|s| !s.vertex_bone_indices.is_empty() && !s.vertex_bone_weights.is_empty());
        let vertices: Vec<Vertex> = (0..num_verts)
            .map(|i| {
                let position = mesh.positions[i];
                // Drop alpha — current `Vertex` color is 3-channel.
                // Imported colors carry RGBA so alpha is preserved on
                // the import side for a future 4-channel vertex format
                // (#618). Hair-tip / eyelash modulation will become
                // visible once the renderer's Vertex extends.
                let color = if i < mesh.colors.len() {
                    let c = mesh.colors[i];
                    [c[0], c[1], c[2]]
                } else {
                    [1.0, 1.0, 1.0]
                };
                let normal = if i < mesh.normals.len() {
                    mesh.normals[i]
                } else {
                    [0.0, 1.0, 0.0]
                };
                let uv = if i < mesh.uvs.len() {
                    mesh.uvs[i]
                } else {
                    [0.0, 0.0]
                };
                if let Some(skin) = skin_vertex_data {
                    // Guard against parallel-vector truncation — if the
                    // sparse skin upload filled fewer vertices than the
                    // mesh has positions, fall back to rigid for the
                    // remainder rather than panicking on index.
                    if i < skin.vertex_bone_indices.len() && i < skin.vertex_bone_weights.len() {
                        let idx = skin.vertex_bone_indices[i];
                        let w = skin.vertex_bone_weights[i];
                        return Vertex::new_skinned(
                            position,
                            color,
                            normal,
                            uv,
                            [idx[0] as u32, idx[1] as u32, idx[2] as u32, idx[3] as u32],
                            w,
                        );
                    }
                }
                Vertex::new(position, color, normal, uv)
            })
            .collect();

        let alloc = ctx.allocator.as_ref().unwrap();
        // upload_scene_mesh registers the vertices/indices into the global
        // geometry SSBO that RT ray queries sample for reflection UVs.
        // See #371.
        let mesh_handle = match ctx.mesh_registry.upload_scene_mesh(
            &ctx.device,
            alloc,
            &ctx.graphics_queue,
            ctx.transfer_pool,
            &vertices,
            &mesh.indices,
            ctx.device_caps.ray_query_supported,
            None,
        ) {
            Ok(h) => h,
            Err(e) => {
                log::warn!(
                    "Failed to upload NIF mesh '{}': {}",
                    mesh.name.as_deref().unwrap_or("?"),
                    e
                );
                continue;
            }
        };

        // Collect BLAS specs for batched build after the loop.
        blas_specs.push((mesh_handle, num_verts as u32, mesh.indices.len() as u32));

        let tex_handle = resolve_texture(ctx, tex_provider, mesh.texture_path.as_deref());

        let quat = Quat::from_xyzw(
            mesh.rotation[0],
            mesh.rotation[1],
            mesh.rotation[2],
            mesh.rotation[3],
        );
        let translation = Vec3::new(
            mesh.translation[0],
            mesh.translation[1],
            mesh.translation[2],
        );

        let entity = world.spawn();
        world.insert(entity, Transform::new(translation, quat, mesh.scale));
        world.insert(entity, GlobalTransform::IDENTITY);
        world.insert(entity, MeshHandle(mesh_handle));
        world.insert(entity, TextureHandle(tex_handle));

        // Attach bounding data (#217): LocalBound captures the mesh-local
        // sphere; WorldBound is a placeholder filled in by the bound
        // propagation system once GlobalTransform has been computed.
        world.insert(
            entity,
            LocalBound::new(
                Vec3::new(
                    mesh.local_bound_center[0],
                    mesh.local_bound_center[1],
                    mesh.local_bound_center[2],
                ),
                mesh.local_bound_radius,
            ),
        );
        world.insert(entity, WorldBound::ZERO);
        if mesh.has_alpha {
            world.insert(
                entity,
                AlphaBlend {
                    src_blend: mesh.src_blend_mode,
                    dst_blend: mesh.dst_blend_mode,
                },
            );
        }
        if mesh.two_sided {
            world.insert(entity, TwoSided);
        }
        if mesh.is_decal {
            world.insert(entity, Decal);
        }
        // Carry `NiAVObject.flags` across — gameplay systems branch on
        // DISABLE_SORTING / SELECTIVE_UPDATE / DISPLAY_OBJECT bits
        // without touching the NIF source. APP_CULLED shapes never
        // reach this point (filtered import-side in walk.rs). See #222.
        if mesh.flags != 0 {
            world.insert(entity, SceneFlags::from_nif(mesh.flags));
        }
        // Attach material data (specular, emissive, glossiness, UV transform, etc.)
        world.insert(
            entity,
            Material {
                emissive_color: mesh.emissive_color,
                emissive_mult: mesh.emissive_mult,
                specular_color: mesh.specular_color,
                specular_strength: mesh.specular_strength,
                diffuse_color: mesh.diffuse_color,
                ambient_color: mesh.ambient_color,
                glossiness: mesh.glossiness,
                uv_offset: mesh.uv_offset,
                uv_scale: mesh.uv_scale,
                alpha: mesh.mat_alpha,
                env_map_scale: mesh.env_map_scale,
                normal_map: mesh.normal_map.clone(),
                texture_path: mesh.texture_path.clone(),
                material_path: mesh.material_path.clone(),
                glow_map: mesh.glow_map.clone(),
                detail_map: mesh.detail_map.clone(),
                gloss_map: mesh.gloss_map.clone(),
                dark_map: mesh.dark_map.clone(),
                vertex_color_mode: mesh.vertex_color_mode,
                alpha_test: mesh.alpha_test,
                alpha_threshold: mesh.alpha_threshold,
                alpha_test_func: mesh.alpha_test_func,
                material_kind: mesh.material_kind,
                z_test: mesh.z_test,
                z_write: mesh.z_write,
                z_function: mesh.z_function,
                shader_type_fields: if mesh.shader_type_fields.is_empty() {
                    None
                } else {
                    Some(Box::new(mesh.shader_type_fields.to_core()))
                },
            },
        );

        // Load and attach normal map texture handle.
        if let Some(ref nmap_path) = mesh.normal_map {
            let h = resolve_texture(ctx, tex_provider, Some(nmap_path.as_str()));
            if h != ctx.texture_registry.fallback() {
                world.insert(entity, NormalMapHandle(h));
            }
        }
        // Load and attach dark/lightmap texture handle.
        if let Some(ref dark_path) = mesh.dark_map {
            let h = resolve_texture(ctx, tex_provider, Some(dark_path.as_str()));
            if h != ctx.texture_registry.fallback() {
                world.insert(entity, DarkMapHandle(h));
            }
        }
        // #399 — three NiTexturingProperty extra slots packed into one
        // ECS component. Mirrors the cell_loader.rs path; only attached
        // when at least one slot resolved to a real texture handle.
        let mut resolve = |path: &Option<String>| -> u32 {
            path.as_deref()
                .map(|p| resolve_texture(ctx, tex_provider, Some(p)))
                .filter(|&h| h != ctx.texture_registry.fallback())
                .unwrap_or(0)
        };
        let glow_h = resolve(&mesh.glow_map);
        let detail_h = resolve(&mesh.detail_map);
        let gloss_h = resolve(&mesh.gloss_map);
        let parallax_h = resolve(&mesh.parallax_map);
        let env_h = resolve(&mesh.env_map);
        let env_mask_h = resolve(&mesh.env_mask);
        if glow_h != 0
            || detail_h != 0
            || gloss_h != 0
            || parallax_h != 0
            || env_h != 0
            || env_mask_h != 0
        {
            world.insert(
                entity,
                ExtraTextureMaps {
                    glow: glow_h,
                    detail: detail_h,
                    gloss: gloss_h,
                    parallax: parallax_h,
                    env: env_h,
                    env_mask: env_mask_h,
                    parallax_height_scale: mesh.parallax_height_scale.unwrap_or(0.04),
                    parallax_max_passes: mesh.parallax_max_passes.unwrap_or(4.0),
                },
            );
        }

        if let Some(ref name) = mesh.name {
            let mut pool = world.resource_mut::<StringPool>();
            let sym = pool.intern(name);
            drop(pool);
            world.insert(entity, Name(sym));
        }

        // Attach skinning binding if present. Resolves each bone name to
        // the entity spawned for that node in Phase 1. Missing bones are
        // kept as `None`; the palette system substitutes identity for them.
        if let Some(ref skin) = mesh.skin {
            if skin.bones.len() > MAX_BONES_PER_MESH {
                log::warn!(
                    "Skinned mesh '{}' has {} bones (> MAX_BONES_PER_MESH={}); skipping skinning",
                    mesh.name.as_deref().unwrap_or("?"),
                    skin.bones.len(),
                    MAX_BONES_PER_MESH
                );
            } else {
                let mut bones: Vec<Option<EntityId>> = Vec::with_capacity(skin.bones.len());
                let mut binds: Vec<Mat4> = Vec::with_capacity(skin.bones.len());
                let mut unresolved = 0_usize;
                for bone in &skin.bones {
                    match node_by_name.get(&bone.name) {
                        Some(&e) => bones.push(Some(e)),
                        None => {
                            bones.push(None);
                            unresolved += 1;
                        }
                    }
                    binds.push(Mat4::from_cols_array_2d(&bone.bind_inverse));
                }
                let root_entity = skin
                    .skeleton_root
                    .as_ref()
                    .and_then(|n| node_by_name.get(n).copied());
                world.insert(entity, SkinnedMesh::new(root_entity, bones, binds));
                log::info!(
                    "Skinned mesh '{}': {} bones ({} unresolved), root={:?}",
                    mesh.name.as_deref().unwrap_or("?"),
                    skin.bones.len(),
                    unresolved,
                    skin.skeleton_root,
                );
            }
        }

        // Set up parent relationship.
        if let Some(parent_idx) = mesh.parent_node {
            let parent_entity = node_entities[parent_idx];
            world.insert(entity, Parent(parent_entity));
            add_child(world, parent_entity, entity);
        }

        log::info!(
            "Loaded NIF mesh '{}': {} verts, {} tris, tex={:?}",
            mesh.name.as_deref().unwrap_or("unnamed"),
            num_verts,
            mesh.indices.len() / 3,
            mesh.texture_path,
        );
        count += 1;
    }

    // Batched BLAS build: single GPU submission for all NIF meshes.
    if !blas_specs.is_empty() {
        ctx.build_blas_batched(&blas_specs);
    }

    let root = node_entities.first().copied();

    // #261 — mesh-embedded controller chains (water UV scroll, torch
    // flame visibility, lava emissive pulse). `import_nif_scene`
    // collected every NiObjectNET.controller_ref chain into a single
    // looping clip. Register it and spawn an AnimationPlayer scoped to
    // the NIF root so the subtree-local name lookup works the same way
    // it does for KF clips.
    if let Some(nif_embedded_clip) = imported.embedded_clip.as_ref() {
        let float_ct = nif_embedded_clip.float_channels.len();
        let color_ct = nif_embedded_clip.color_channels.len();
        let bool_ct = nif_embedded_clip.bool_channels.len();
        let duration = nif_embedded_clip.duration;
        let clip_handle = {
            let mut pool = world.resource_mut::<StringPool>();
            let clip = crate::anim_convert::convert_nif_clip(nif_embedded_clip, &mut pool);
            drop(pool);
            let mut registry = world.resource_mut::<AnimationClipRegistry>();
            registry.add(clip)
        };
        let player_entity = world.spawn();
        let mut player = AnimationPlayer::new(clip_handle);
        if let Some(root) = root {
            player.root_entity = Some(root);
        }
        world.insert(player_entity, player);
        log::info!(
            "Embedded animation clip registered from '{}' ({:.2}s, {} float + {} color + {} bool channels) → handle {}",
            label,
            duration,
            float_ct,
            color_ct,
            bool_ct,
            clip_handle,
        );
    }

    log::info!(
        "Imported {} nodes + {} meshes from '{}'",
        imported.nodes.len(),
        count,
        label
    );
    (count + imported.nodes.len(), root)
}

#[cfg(test)]
mod radius_parse_tests {
    //! Regression tests for [`parse_exterior_radius`] — issue #531.
    //!
    //! The CLI `--radius N` argument pre-fix was silently ignored
    //! (hardcoded `3` in the `load_exterior_cells` call). These tests
    //! pin the clamp bounds + the fallback behaviour so a future
    //! refactor that tries to "simplify" the parse (e.g. remove the
    //! clamp, loosen the Err-fallback to 0) gets caught.

    use super::parse_exterior_radius;

    #[test]
    fn parses_valid_radius_verbatim() {
        assert_eq!(parse_exterior_radius("1"), 1);
        assert_eq!(parse_exterior_radius("3"), 3);
        assert_eq!(parse_exterior_radius("5"), 5);
        assert_eq!(parse_exterior_radius("7"), 7);
    }

    #[test]
    fn clamps_below_one_to_one() {
        assert_eq!(parse_exterior_radius("0"), 1);
        assert_eq!(parse_exterior_radius("-5"), 1);
    }

    #[test]
    fn clamps_above_seven_to_seven() {
        assert_eq!(parse_exterior_radius("8"), 7);
        assert_eq!(parse_exterior_radius("100"), 7, "accidental large input must not load 40k cells");
    }

    #[test]
    fn falls_back_to_default_on_parse_failure() {
        // Non-numeric input → fall back to 3 (default 7×7 grid).
        assert_eq!(parse_exterior_radius("foo"), 3);
        assert_eq!(parse_exterior_radius(""), 3);
        assert_eq!(parse_exterior_radius("3.5"), 3);
    }

    #[test]
    fn trims_whitespace_before_parse() {
        assert_eq!(parse_exterior_radius("  5  "), 5);
    }
}
