//! Scene setup and NIF loading logic.

use byroredux_core::animation::{AnimationClipRegistry, AnimationPlayer};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{
    ActiveCamera, Billboard, BillboardMode, Camera, GlobalTransform, LocalBound, Material,
    MeshHandle, Name, Parent, SkinnedMesh, TextureHandle, Transform, World, WorldBound,
    MAX_BONES_PER_MESH,
};
use byroredux_core::math::{Mat4, Quat, Vec3};
use byroredux_core::string::StringPool;
use byroredux_renderer::{cube_vertices, quad_vertices, triangle_vertices, Vertex, VulkanContext};
use byroredux_ui::UiManager;

use crate::anim_convert::convert_nif_clip;
use crate::asset_provider::{
    build_texture_provider, parse_grid_coords, resolve_texture, TextureProvider,
};
use crate::cell_loader;
use crate::components::{AlphaBlend, CellLightingRes, Decal, InputState, Spinning, TwoSided};
use crate::helpers::add_child;

/// Called once after the renderer is ready — uploads meshes and spawns entities.
pub(crate) fn setup_scene(
    world: &mut World,
    ctx: &mut VulkanContext,
    ui_manager: &mut Option<UiManager>,
    ui_texture_handle: &mut Option<u32>,
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

        if let (Some(ref esm_path), Some(ref cell_id)) = (&esm_path, &cell_id) {
            // Interior cell mode
            let tex_provider = build_texture_provider(&args);
            match cell_loader::load_cell(esm_path, cell_id, world, ctx, &tex_provider) {
                Ok(result) => {
                    cam_center = result.center;
                    has_nif_content = true;
                    // Store cell lighting for the renderer.
                    if let Some(ref lit) = result.lighting {
                        let (rx, ry) = (lit.directional_rotation[0], lit.directional_rotation[1]);
                        // Convert Euler XY rotation to direction vector (Z-up → Y-up).
                        let dir_z_up = [ry.cos() * rx.cos(), ry.cos() * rx.sin(), -ry.sin()];
                        // Z-up to Y-up: (x, y, z) → (x, z, -y)
                        let dir = [dir_z_up[0], dir_z_up[2], -dir_z_up[1]];
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
            match cell_loader::load_exterior_cells(esm_path, cx, cy, 1, world, ctx, &tex_provider) {
                Ok(result) => {
                    cam_center = result.center;
                    has_nif_content = true;
                    log::info!(
                        "Exterior '{}' ready: {} entities",
                        result.cell_name,
                        result.entity_count
                    );
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
                            let mut registry = world.resource_mut::<AnimationClipRegistry>();
                            for nif_clip in &nif_clips {
                                let clip = convert_nif_clip(nif_clip);
                                let handle = registry.add(clip);
                                log::info!(
                                    "Loaded animation clip '{}' ({:.2}s, {} channels) → handle {}",
                                    nif_clip.name,
                                    nif_clip.duration,
                                    nif_clip.channels.len(),
                                    handle,
                                );
                            }
                            let first_handle = registry.len() as u32 - nif_clips.len() as u32;
                            drop(registry);

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

        // Build BLAS for RT shadows on demo meshes.
        let (cv, ci) = (verts.len() as u32, idxs.len() as u32);
        ctx.build_blas_for_mesh(cube_handle, cv, ci);
        let (qv, qi) = (quad_verts.len() as u32, quad_idxs.len() as u32);
        ctx.build_blas_for_mesh(quad_handle, qv, qi);
        let (rv, ri) = (red_verts.len() as u32, red_idxs.len() as u32);
        ctx.build_blas_for_mesh(red_handle, rv, ri);
        let (bv, bi) = (blue_verts.len() as u32, blue_idxs.len() as u32);
        ctx.build_blas_for_mesh(blue_handle, bv, bi);

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

    // Spawn camera entity looking at the scene center.
    let cam = world.spawn();
    let cam_pos = if has_nif_content {
        cam_center + Vec3::new(0.0, 100.0, 200.0)
    } else {
        Vec3::new(0.0, 1.5, 4.0)
    };
    let cam_target = cam_center;
    let forward = (cam_target - cam_pos).normalize();
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
    ) {
        log::warn!("Failed to build geometry SSBO: {e}");
    }
    // Write global geometry buffers to scene descriptor sets for RT reflection UV lookups.
    if let (Some(ref vb), Some(ref ib)) = (&ctx.mesh_registry.global_vertex_buffer, &ctx.mesh_registry.global_index_buffer) {
        for f in 0..2 {
            ctx.scene_buffers.write_geometry_buffers(
                &ctx.device, f,
                vb.buffer, vb.size,
                ib.buffer, ib.size,
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

    // Collect BSA archives.
    let mut tex_provider = TextureProvider::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--textures-bsa" {
            if let Some(path) = args.get(i + 1) {
                match byroredux_bsa::BsaArchive::open(path) {
                    Ok(a) => {
                        log::info!("Opened textures BSA: '{}'", path);
                        tex_provider.texture_archives.push(a);
                    }
                    Err(e) => log::warn!("Failed to open textures BSA '{}': {}", path, e),
                }
                i += 2;
                continue;
            }
        }
        i += 1;
    }
    // The --bsa mesh archive is also added to the provider for cell loading.
    if let Some(bsa_idx) = args.iter().position(|a| a == "--bsa") {
        if let Some(bsa_path) = args.get(bsa_idx + 1) {
            match byroredux_bsa::BsaArchive::open(bsa_path) {
                Ok(a) => {
                    tex_provider.mesh_archives.push(a);
                }
                Err(_) => {} // Will be reported below in the mesh extraction path.
            }
        }
    }

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
        load_nif_bytes(world, ctx, &data, mesh_path, &tex_provider)
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
        load_nif_bytes(world, ctx, &data, nif_path, &tex_provider)
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
) -> (usize, Option<EntityId>) {
    let scene = match byroredux_nif::parse_nif(data) {
        Ok(s) => s,
        Err(e) => {
            log::error!("Failed to parse NIF '{}': {}", label, e);
            return (0, None);
        }
    };

    let imported = byroredux_nif::import::import_nif_scene(&scene);

    // Phase 1: Spawn node entities (NiNode hierarchy).
    // node_index → EntityId mapping.
    // Also build a name → EntityId map so Phase 3 can resolve skinning
    // bone names to the entities they should drive. Skeleton nodes are
    // the only entities with unique names in a typical NIF, so collisions
    // (multiple nodes sharing a name) are rare; on collision we keep the
    // first spawn (root-most in depth-first order).
    let mut node_entities: Vec<EntityId> = Vec::with_capacity(imported.nodes.len());
    let mut node_by_name: std::collections::HashMap<String, EntityId> =
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

    // Phase 3: Spawn mesh entities with parent links.
    let mut count = 0;
    for mesh in &imported.meshes {
        let num_verts = mesh.positions.len();
        // Skinned vertices use the per-vertex bone indices + weights that
        // #151 / #177 extracted from NiSkinData / BSTriShape. Rigid
        // vertices pass zero weights and the shader's rigid-path routes
        // them through `pc.model` instead of the bone palette.
        let skin_vertex_data = mesh.skin.as_ref().filter(|s| {
            !s.vertex_bone_indices.is_empty() && !s.vertex_bone_weights.is_empty()
        });
        let vertices: Vec<Vertex> = (0..num_verts)
            .map(|i| {
                let position = mesh.positions[i];
                let color = if i < mesh.colors.len() {
                    mesh.colors[i]
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
        let mesh_handle = match ctx.mesh_registry.upload(
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

        // Build BLAS for RT shadow rays.
        ctx.build_blas_for_mesh(mesh_handle, num_verts as u32, mesh.indices.len() as u32);

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
            world.insert(entity, AlphaBlend);
        }
        if mesh.two_sided {
            world.insert(entity, TwoSided);
        }
        if mesh.is_decal {
            world.insert(entity, Decal);
        }
        // Attach material data (specular, emissive, glossiness, UV transform, etc.)
        world.insert(
            entity,
            Material {
                emissive_color: mesh.emissive_color,
                emissive_mult: mesh.emissive_mult,
                specular_color: mesh.specular_color,
                specular_strength: mesh.specular_strength,
                glossiness: mesh.glossiness,
                uv_offset: mesh.uv_offset,
                uv_scale: mesh.uv_scale,
                alpha: mesh.mat_alpha,
                env_map_scale: mesh.env_map_scale,
                normal_map: mesh.normal_map.clone(),
                texture_path: mesh.texture_path.clone(),
                glow_map: mesh.glow_map.clone(),
                detail_map: mesh.detail_map.clone(),
                gloss_map: mesh.gloss_map.clone(),
                vertex_color_mode: mesh.vertex_color_mode,
                alpha_test: mesh.alpha_test,
                alpha_threshold: mesh.alpha_threshold,
            },
        );

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

    let root = node_entities.first().copied();
    log::info!(
        "Imported {} nodes + {} meshes from '{}'",
        imported.nodes.len(),
        count,
        label
    );
    (count + imported.nodes.len(), root)
}
