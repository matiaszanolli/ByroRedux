//! ByroRedux — ECS-driven game loop with Vulkan rendering.

mod cell_loader;

use anyhow::Result;
use byroredux_core::animation::{
    advance_stack, advance_time, sample_blended_transform, sample_bool_channel,
    sample_color_channel, sample_float_channel, sample_rotation, sample_scale, sample_translation,
    split_root_motion, AnimBoolKey, AnimColorKey, AnimFloatKey, AnimationClip,
    AnimationClipRegistry, AnimationPlayer, AnimationStack, BoolChannel, ColorChannel, ColorTarget,
    CycleType, FloatChannel, FloatTarget, KeyType, RootMotionDelta, RotationKey, ScaleKey,
    TransformChannel, TranslationKey,
};
use byroredux_core::console::{CommandOutput, CommandRegistry, ConsoleCommand};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::Resource;
use byroredux_core::ecs::{
    ActiveCamera, AnimatedAlpha, AnimatedColor, AnimatedVisibility, Camera, Children, Component,
    DebugStats, DeltaTime, EngineConfig, GlobalTransform, LightSource, Material, MeshHandle, Name,
    Parent, Scheduler, SparseSetStorage, TextureHandle, TotalTime, Transform, World,
};
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::{FixedString, StringPool};
use byroredux_core::types::Color;
use byroredux_platform::window::{self, WindowConfig};
use byroredux_renderer::vulkan::context::DrawCommand;
use byroredux_renderer::{cube_vertices, quad_vertices, triangle_vertices, Vertex, VulkanContext};
use byroredux_ui::UiManager;
use std::collections::HashSet;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

/// Marker component for entities that should spin in the demo scene.
#[derive(Debug, Clone, Copy)]
struct Spinning;
impl Component for Spinning {
    type Storage = SparseSetStorage<Self>;
}

/// Marker component for entities that use alpha blending.
#[derive(Debug, Clone, Copy)]
struct AlphaBlend;
impl Component for AlphaBlend {
    type Storage = SparseSetStorage<Self>;
}

/// Marker component for entities that need two-sided rendering (no backface culling).
#[derive(Debug, Clone, Copy)]
struct TwoSided;
impl Component for TwoSided {
    type Storage = SparseSetStorage<Self>;
}

/// Marker component for decal geometry (renders on top of coplanar surfaces).
#[derive(Debug, Clone, Copy)]
struct Decal;
impl Component for Decal {
    type Storage = SparseSetStorage<Self>;
}

/// System names stored as a resource for the `systems` console command.
struct SystemList(Vec<String>);
impl Resource for SystemList {}

/// Cell lighting from the ESM (ambient + directional).
struct CellLightingRes {
    ambient: [f32; 3],
    directional_color: [f32; 3],
    /// Direction vector in Y-up space (computed from rotation).
    directional_dir: [f32; 3],
}
impl Resource for CellLightingRes {}

/// Cached name→entity mapping for the animation system.
/// Rebuilt only when the entity count changes (no per-frame allocations).
struct NameIndex {
    map: std::collections::HashMap<FixedString, EntityId>,
    generation: u32,
}
impl Resource for NameIndex {}

impl NameIndex {
    fn new() -> Self {
        Self {
            map: std::collections::HashMap::new(),
            generation: u32::MAX, // Force rebuild on first use.
        }
    }
}

/// Tracks keyboard and mouse input state for the fly camera.
struct InputState {
    keys_held: HashSet<KeyCode>,
    /// Yaw (horizontal) and pitch (vertical) in radians.
    yaw: f32,
    pitch: f32,
    mouse_captured: bool,
    move_speed: f32,
    look_sensitivity: f32,
}

impl Resource for InputState {}

impl Default for InputState {
    fn default() -> Self {
        Self {
            keys_held: HashSet::new(),
            yaw: 0.0,
            pitch: 0.0,
            mouse_captured: false,
            move_speed: 200.0, // Bethesda units per second
            look_sensitivity: 0.002,
        }
    }
}

/// Fly camera system: WASD + mouse look. Updates the active camera's Transform.
fn fly_camera_system(world: &World, dt: f32) {
    let Some(active) = world.try_resource::<ActiveCamera>() else {
        return;
    };
    let cam_entity = active.0;
    drop(active);

    let Some(input) = world.try_resource::<InputState>() else {
        return;
    };
    if !input.mouse_captured {
        return;
    }

    let speed = input.move_speed * dt;
    let yaw = input.yaw;
    let pitch = input.pitch;

    // Build movement vector from held keys.
    let mut move_dir = Vec3::ZERO;
    if input.keys_held.contains(&KeyCode::KeyW) {
        move_dir.z += 1.0;
    }
    if input.keys_held.contains(&KeyCode::KeyS) {
        move_dir.z -= 1.0;
    }
    if input.keys_held.contains(&KeyCode::KeyA) {
        move_dir.x -= 1.0;
    }
    if input.keys_held.contains(&KeyCode::KeyD) {
        move_dir.x += 1.0;
    }
    if input.keys_held.contains(&KeyCode::Space) {
        move_dir.y += 1.0;
    }
    if input.keys_held.contains(&KeyCode::ShiftLeft) {
        move_dir.y -= 1.0;
    }

    // Speed boost with Ctrl.
    let boost = if input.keys_held.contains(&KeyCode::ControlLeft) {
        3.0
    } else {
        1.0
    };
    drop(input);

    // Build rotation from yaw/pitch.
    let rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);

    if let Some(mut tq) = world.query_mut::<Transform>() {
        if let Some(transform) = tq.get_mut(cam_entity) {
            transform.rotation = rotation;

            if move_dir != Vec3::ZERO {
                let move_dir = move_dir.normalize();
                // Move relative to camera orientation (but yaw-only for horizontal).
                let forward = Quat::from_rotation_y(yaw) * -Vec3::Z;
                let right = Quat::from_rotation_y(yaw) * Vec3::X;
                let up = Vec3::Y;

                transform.translation += forward * move_dir.z * speed * boost;
                transform.translation += right * move_dir.x * speed * boost;
                transform.translation += up * move_dir.y * speed * boost;
            }
        }
    }
}

/// Animation system: advances AnimationPlayer time and applies interpolated
/// transforms to named entities that match the clip's channel names.
fn animation_system(world: &World, dt: f32) {
    // Read the clip registry (immutable).
    let Some(registry) = world.try_resource::<AnimationClipRegistry>() else {
        return;
    };
    if registry.is_empty() {
        return;
    }

    // Per-frame cache of subtree name maps — built once per unique root entity,
    // reused across all animated entities sharing that root. Avoids rebuilding
    // the HashMap + BFS walk for every AnimationPlayer/Stack each frame.
    let mut subtree_cache: std::collections::HashMap<
        EntityId,
        std::collections::HashMap<FixedString, EntityId>,
    > = std::collections::HashMap::new();

    // Rebuild name→entity index only when entities have been added.
    let current_gen = world.next_entity_id();
    {
        let needs_rebuild = world
            .try_resource::<NameIndex>()
            .map(|idx| idx.generation != current_gen)
            .unwrap_or(true);
        if needs_rebuild {
            let name_query = match world.query::<Name>() {
                Some(q) => q,
                None => return,
            };
            let mut new_map = std::collections::HashMap::new();
            for (entity, name_comp) in name_query.iter() {
                new_map.insert(name_comp.0, entity);
            }
            drop(name_query);
            let mut idx = world.resource_mut::<NameIndex>();
            idx.map = new_map;
            idx.generation = current_gen;
        }
    }

    let Some(pool) = world.try_resource::<StringPool>() else {
        return;
    };
    let name_index = world.try_resource::<NameIndex>().unwrap();

    // Iterate all animation players and apply.
    let Some(player_query) = world.query_mut::<AnimationPlayer>() else {
        return;
    };
    let entities_with_players: Vec<_> = player_query.iter().map(|(e, _)| e).collect();
    drop(player_query);

    for entity in entities_with_players {
        // Get the player, advance time, then apply channels.
        let mut player_query = world.query_mut::<AnimationPlayer>().unwrap();
        let player = player_query.get_mut(entity).unwrap();

        let clip_handle = player.clip_handle;
        let root_entity_opt = player.root_entity;
        let Some(clip) = registry.get(clip_handle) else {
            continue;
        };

        advance_time(player, clip, dt);
        let current_time = player.local_time;
        drop(player_query);

        // Scoped name lookup — cached per root entity.
        let scoped_map = root_entity_opt.map(|root| {
            subtree_cache
                .entry(root)
                .or_insert_with(|| build_subtree_name_map(world, root))
                as &std::collections::HashMap<FixedString, EntityId>
        });
        let resolve_entity = |channel_name: &str| -> Option<EntityId> {
            let sym = pool.get(channel_name)?;
            if let Some(scoped) = scoped_map {
                scoped.get(&sym).copied()
            } else {
                name_index.map.get(&sym).copied()
            }
        };

        // Apply transform channels.
        let is_accum_root = |name: &str| -> bool { clip.accum_root_name.as_deref() == Some(name) };
        {
            let mut transform_query = world.query_mut::<Transform>().unwrap();
            let mut root_motion = Vec3::ZERO;
            for (channel_name, channel) in &clip.channels {
                let Some(target_entity) = resolve_entity(channel_name) else {
                    continue;
                };
                let Some(transform) = transform_query.get_mut(target_entity) else {
                    continue;
                };
                if let Some(pos) = sample_translation(channel, current_time) {
                    if is_accum_root(channel_name) {
                        // Split: vertical → animation, horizontal → root motion delta.
                        let (anim_pos, delta) = split_root_motion(pos);
                        transform.translation = anim_pos;
                        root_motion += delta;
                    } else {
                        transform.translation = pos;
                    }
                }
                if let Some(rot) = sample_rotation(channel, current_time) {
                    transform.rotation = rot;
                }
                if let Some(scale) = sample_scale(channel, current_time) {
                    transform.scale = scale;
                }
            }
            drop(transform_query);

            // Write root motion delta to the player entity.
            if root_motion != Vec3::ZERO {
                if let Some(mut rmq) = world.query_mut::<RootMotionDelta>() {
                    if let Some(rm) = rmq.get_mut(entity) {
                        rm.0 = root_motion;
                    }
                }
            }
        }

        // Apply float channels (alpha, UV params, shader floats).
        for (channel_name, channel) in &clip.float_channels {
            let Some(target_entity) = resolve_entity(channel_name) else {
                continue;
            };
            let value = sample_float_channel(channel, current_time);
            if channel.target == FloatTarget::Alpha {
                if let Some(mut aq) = world.query_mut::<AnimatedAlpha>() {
                    if let Some(a) = aq.get_mut(target_entity) {
                        a.0 = value;
                    } else {
                        drop(aq);
                        // Can't insert during system — would need &mut World.
                        // Components should be pre-attached during import.
                    }
                }
            }
            // UV and shader float channels are logged but not yet wired to rendering
            // (would require UvTransform / shader uniform components).
        }

        // Apply color channels.
        for (channel_name, channel) in &clip.color_channels {
            let Some(target_entity) = resolve_entity(channel_name) else {
                continue;
            };
            let value = sample_color_channel(channel, current_time);
            if let Some(mut cq) = world.query_mut::<AnimatedColor>() {
                if let Some(c) = cq.get_mut(target_entity) {
                    c.0 = value;
                }
            }
        }

        // Apply bool (visibility) channels.
        for (channel_name, channel) in &clip.bool_channels {
            let Some(target_entity) = resolve_entity(channel_name) else {
                continue;
            };
            let value = sample_bool_channel(channel, current_time);
            if let Some(mut vq) = world.query_mut::<AnimatedVisibility>() {
                if let Some(v) = vq.get_mut(target_entity) {
                    v.0 = value;
                }
            }
        }
    }

    // ── AnimationStack processing (multi-layer blending) ──────────────
    let Some(stack_query) = world.query_mut::<AnimationStack>() else {
        return;
    };
    let stack_entities: Vec<_> = stack_query.iter().map(|(e, _)| e).collect();
    drop(stack_query);

    for entity in stack_entities {
        // Advance all layers.
        {
            let mut sq = world.query_mut::<AnimationStack>().unwrap();
            let stack = sq.get_mut(entity).unwrap();
            advance_stack(stack, &registry, dt);
        }

        // Sample blended transforms for each channel name.
        let sq = world.query::<AnimationStack>().unwrap();
        let stack = sq.get(entity).unwrap();

        // Scoped name lookup for stacks — cached per root entity.
        let stack_scoped_map = stack.root_entity.map(|root| {
            subtree_cache
                .entry(root)
                .or_insert_with(|| build_subtree_name_map(world, root))
                as &std::collections::HashMap<FixedString, EntityId>
        });
        let stack_resolve = |channel_name: &str| -> Option<EntityId> {
            let sym = pool.get(channel_name)?;
            if let Some(scoped) = stack_scoped_map {
                scoped.get(&sym).copied()
            } else {
                name_index.map.get(&sym).copied()
            }
        };

        // Collect all channel names across all active layers.
        let mut channel_names: Vec<&str> = Vec::new();
        for layer in &stack.layers {
            if let Some(clip) = registry.get(layer.clip_handle) {
                for name in clip.channels.keys() {
                    channel_names.push(name.as_str());
                }
            }
        }
        channel_names.sort_unstable();
        channel_names.dedup();

        let mut updates: Vec<(EntityId, Vec3, Quat, f32)> = Vec::new();
        for channel_name in &channel_names {
            let Some(target_entity) = stack_resolve(channel_name) else {
                continue;
            };
            if let Some((pos, rot, scale)) =
                sample_blended_transform(stack, &registry, channel_name)
            {
                updates.push((target_entity, pos, rot, scale));
            }
        }
        drop(sq);

        // Apply blended transforms.
        let mut tq = world.query_mut::<Transform>().unwrap();
        for (target, pos, rot, scale) in updates {
            if let Some(transform) = tq.get_mut(target) {
                transform.translation = pos;
                transform.rotation = rot;
                transform.scale = scale;
            }
        }
    }
}

/// Transform propagation system: computes GlobalTransform from local Transform + parent chain.
///
/// For root entities (no Parent), GlobalTransform = Transform.
/// For child entities, GlobalTransform = parent.GlobalTransform ∘ child.Transform.
/// Must run after animation_system and before rendering.
/// Create the transform propagation system with reusable scratch buffers.
///
/// Returns a closure (FnMut) that captures `roots` and `queue` Vecs,
/// clearing and reusing them each frame instead of allocating new ones.
fn make_transform_propagation_system() -> impl FnMut(&World, f32) + Send + Sync {
    let mut roots: Vec<EntityId> = Vec::new();
    let mut queue: Vec<EntityId> = Vec::new();

    move |world: &World, _dt: f32| {
        roots.clear();
        queue.clear();

        // Phase 1: find root entities (have Transform but no Parent).
        {
            let Some(tq) = world.query::<Transform>() else {
                return;
            };
            let parent_q = world.query::<Parent>();

            for (entity, _) in tq.iter() {
                let is_root = parent_q
                    .as_ref()
                    .map(|pq| pq.get(entity).is_none())
                    .unwrap_or(true);
                if is_root {
                    roots.push(entity);
                }
            }
        }

        // Update root GlobalTransforms.
        {
            let tq = world.query::<Transform>().unwrap();
            let mut gq = match world.query_mut::<GlobalTransform>() {
                Some(q) => q,
                None => return,
            };
            for &entity in &roots {
                if let Some(t) = tq.get(entity) {
                    if let Some(g) = gq.get_mut(entity) {
                        g.translation = t.translation;
                        g.rotation = t.rotation;
                        g.scale = t.scale;
                    }
                }
            }
        }

        // Phase 2: propagate to children using BFS.
        let children_q = world.query::<Children>();
        let Some(ref cq) = children_q else { return };

        for &root in &roots {
            if let Some(children) = cq.get(root) {
                queue.extend_from_slice(&children.0);
            }
        }

        while let Some(entity) = queue.pop() {
            let parent_q = world.query::<Parent>().unwrap();
            let Some(parent) = parent_q.get(entity) else {
                continue;
            };
            let parent_id = parent.0;
            drop(parent_q);

            let gq_read = world.query::<GlobalTransform>().unwrap();
            let Some(parent_global) = gq_read.get(parent_id) else {
                continue;
            };
            let parent_global = *parent_global;
            drop(gq_read);

            let tq = world.query::<Transform>().unwrap();
            let local = tq.get(entity).copied().unwrap_or(Transform::IDENTITY);
            drop(tq);

            let composed = GlobalTransform::compose(
                &parent_global,
                local.translation,
                local.rotation,
                local.scale,
            );

            let mut gq_write = world.query_mut::<GlobalTransform>().unwrap();
            if let Some(g) = gq_write.get_mut(entity) {
                *g = composed;
            }
            drop(gq_write);

            if let Some(children) = cq.get(entity) {
                queue.extend_from_slice(&children.0);
            }
        }
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let debug_mode = args.iter().any(|a| a == "--debug");

    // Set up logging. --debug forces debug level.
    if debug_mode {
        std::env::set_var(
            "RUST_LOG",
            std::env::var("RUST_LOG").unwrap_or("debug".into()),
        );
    }
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("ByroRedux starting");
    log::info!("{}", byroredux_cxx_bridge::ffi::native_hello());

    // Headless --cmd mode: execute command and exit without creating a window.
    if let Some(cmd_idx) = args.iter().position(|a| a == "--cmd") {
        let input = args.get(cmd_idx + 1).map(|s| s.as_str()).unwrap_or("help");
        let mut world = World::new();
        world.insert_resource(DebugStats::default());
        world.insert_resource(EngineConfig {
            debug_logging: true,
            ..Default::default()
        });
        let registry = build_command_registry();
        world.insert_resource(SystemList(Vec::new()));
        world.insert_resource(registry);
        let reg = world.resource::<CommandRegistry>();
        let output = reg.execute(&world, input);
        drop(reg);
        for line in &output.lines {
            println!("{}", line);
        }
        return Ok(());
    }

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new(debug_mode);
    event_loop.run_app(&mut app)?;

    Ok(())
}

struct App {
    window: Option<Window>,
    renderer: Option<VulkanContext>,
    world: World,
    scheduler: Scheduler,
    last_frame: Instant,
    ui_manager: Option<UiManager>,
    /// Texture handle for the UI overlay (registered in the texture registry).
    ui_texture_handle: Option<u32>,
}

impl App {
    fn new(debug_mode: bool) -> Self {
        let mut world = World::new();

        // Register built-in resources.
        world.insert_resource(DeltaTime(0.0));
        world.insert_resource(TotalTime(0.0));
        world.insert_resource(EngineConfig {
            debug_logging: debug_mode || cfg!(debug_assertions),
            ..Default::default()
        });
        world.insert_resource(DebugStats::default());
        world.insert_resource(InputState::default());
        world.insert_resource(StringPool::new());
        world.insert_resource(AnimationClipRegistry::new());
        world.insert_resource(NameIndex::new());

        // Register scripting component storages.
        byroredux_scripting::register(&mut world);

        // Build the system schedule.
        let mut scheduler = Scheduler::new();
        scheduler.add(fly_camera_system);
        scheduler.add(animation_system);
        scheduler.add(make_transform_propagation_system());
        scheduler.add(spin_system);
        scheduler.add(byroredux_scripting::timer_tick_system);
        scheduler.add(log_stats_system);
        scheduler.add(byroredux_scripting::event_cleanup_system);

        // Store system names + console commands as resources.
        let system_names: Vec<String> = scheduler
            .system_names()
            .iter()
            .map(|s| s.to_string())
            .collect();
        world.insert_resource(SystemList(system_names));
        world.insert_resource(build_command_registry());

        Self {
            window: None,
            renderer: None,
            world,
            scheduler,
            last_frame: Instant::now(),
            ui_manager: None,
            ui_texture_handle: None,
        }
    }

    /// Called once after the renderer is ready — uploads meshes and spawns entities.
    fn setup_scene(&mut self) {
        let ctx = self.renderer.as_mut().unwrap();

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
                match cell_loader::load_cell(esm_path, cell_id, &mut self.world, ctx, &tex_provider)
                {
                    Ok(result) => {
                        cam_center = result.center;
                        has_nif_content = true;
                        // Store cell lighting for the renderer.
                        if let Some(ref lit) = result.lighting {
                            let (rx, ry) =
                                (lit.directional_rotation[0], lit.directional_rotation[1]);
                            // Convert Euler XY rotation to direction vector (Z-up → Y-up).
                            let dir_z_up = [ry.cos() * rx.cos(), ry.cos() * rx.sin(), -ry.sin()];
                            // Z-up to Y-up: (x, y, z) → (x, z, -y)
                            let dir = [dir_z_up[0], dir_z_up[2], -dir_z_up[1]];
                            self.world.insert_resource(CellLightingRes {
                                ambient: lit.ambient,
                                directional_color: lit.directional_color,
                                directional_dir: dir,
                            });
                            log::info!(
                                "Cell lighting: ambient={:?} directional={:?} dir={:?}",
                                lit.ambient,
                                lit.directional_color,
                                dir
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
                match cell_loader::load_exterior_cells(
                    esm_path,
                    cx,
                    cy,
                    1,
                    &mut self.world,
                    ctx,
                    &tex_provider,
                ) {
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
            let (nif_count, loaded_root) = load_nif_from_args(&mut self.world, ctx);
            has_nif_content = nif_count > 0;
            nif_root = loaded_root;
        }

        // Animation: --kf <path> loads a .kf file and starts playback.
        if let Some(kf_idx) = args.iter().position(|a| a == "--kf") {
            if let Some(kf_path) = args.get(kf_idx + 1).cloned() {
                match std::fs::read(&kf_path) {
                    Ok(kf_data) => {
                        match byroredux_nif::parse_nif(&kf_data) {
                            Ok(kf_scene) => {
                                let nif_clips = byroredux_nif::anim::import_kf(&kf_scene);
                                if nif_clips.is_empty() {
                                    log::warn!("No animation clips found in '{}'", kf_path);
                                } else {
                                    let mut registry =
                                        self.world.resource_mut::<AnimationClipRegistry>();
                                    for nif_clip in &nif_clips {
                                        let clip = convert_nif_clip(nif_clip);
                                        let handle = registry.add(clip);
                                        log::info!(
                                            "Loaded animation clip '{}' ({:.2}s, {} channels) → handle {}",
                                            nif_clip.name, nif_clip.duration,
                                            nif_clip.channels.len(), handle,
                                        );
                                    }
                                    let first_handle =
                                        registry.len() as u32 - nif_clips.len() as u32;
                                    drop(registry);

                                    // Spawn an AnimationPlayer scoped to the NIF subtree.
                                    let player_entity = self.world.spawn();
                                    let mut player = AnimationPlayer::new(first_handle);
                                    if let Some(root) = nif_root {
                                        player.root_entity = Some(root);
                                    }
                                    self.world.insert(player_entity, player);
                                    log::info!(
                                        "Animation playback started (clip handle {})",
                                        first_handle
                                    );
                                }
                            }
                            Err(e) => log::error!("Failed to parse KF '{}': {}", kf_path, e),
                        }
                    }
                    Err(e) => log::error!("Failed to read KF '{}': {}", kf_path, e),
                }
            }
        }

        // Only spawn demo primitives when no NIF content was loaded.
        if !has_nif_content {
            let alloc = ctx.allocator.as_ref().unwrap();
            let (verts, idxs) = cube_vertices();
            let queue = &ctx.graphics_queue;
            let pool = ctx.command_pool;
            let rt = ctx.device_caps.ray_query_supported;
            let cube_handle = ctx
                .mesh_registry
                .upload(&ctx.device, alloc, queue, pool, &verts, &idxs, rt)
                .expect("Failed to upload cube mesh");

            let (quad_verts, quad_idxs) = quad_vertices();
            let quad_handle = ctx
                .mesh_registry
                .upload(&ctx.device, alloc, queue, pool, &quad_verts, &quad_idxs, rt)
                .expect("Failed to upload quad mesh");

            let (red_verts, red_idxs) = triangle_vertices([1.0, 0.2, 0.2]);
            let red_handle = ctx
                .mesh_registry
                .upload(&ctx.device, alloc, queue, pool, &red_verts, &red_idxs, rt)
                .expect("Failed to upload red triangle mesh");

            let (blue_verts, blue_idxs) = triangle_vertices([0.2, 0.2, 1.0]);
            let blue_handle = ctx
                .mesh_registry
                .upload(&ctx.device, alloc, queue, pool, &blue_verts, &blue_idxs, rt)
                .expect("Failed to upload blue triangle mesh");

            let cube = self.world.spawn();
            self.world
                .insert(cube, Transform::from_translation(Vec3::new(-1.5, 0.0, 0.0)));
            self.world.insert(cube, GlobalTransform::IDENTITY);
            self.world.insert(cube, MeshHandle(cube_handle));
            self.world.insert(cube, Spinning);

            let quad = self.world.spawn();
            self.world
                .insert(quad, Transform::from_translation(Vec3::new(0.0, 0.0, -1.0)));
            self.world.insert(quad, GlobalTransform::IDENTITY);
            self.world.insert(quad, MeshHandle(quad_handle));
            self.world.insert(quad, Spinning);

            let red_tri = self.world.spawn();
            self.world.insert(
                red_tri,
                Transform::from_translation(Vec3::new(1.5, 0.0, 0.5)),
            );
            self.world.insert(red_tri, GlobalTransform::IDENTITY);
            self.world.insert(red_tri, MeshHandle(red_handle));
            self.world.insert(red_tri, Spinning);

            let blue_tri = self.world.spawn();
            self.world.insert(
                blue_tri,
                Transform::from_translation(Vec3::new(1.8, 0.0, -0.3)),
            );
            self.world.insert(blue_tri, GlobalTransform::IDENTITY);
            self.world.insert(blue_tri, MeshHandle(blue_handle));
            self.world.insert(blue_tri, Spinning);
        }

        // Spawn camera entity looking at the scene center.
        let cam = self.world.spawn();
        let cam_pos = if has_nif_content {
            cam_center + Vec3::new(0.0, 100.0, 200.0)
        } else {
            Vec3::new(0.0, 1.5, 4.0)
        };
        let cam_target = cam_center;
        let forward = (cam_target - cam_pos).normalize();
        let cam_rotation = Quat::from_rotation_arc(-Vec3::Z, forward);
        self.world
            .insert(cam, Transform::new(cam_pos, cam_rotation, 1.0));
        self.world
            .insert(cam, GlobalTransform::new(cam_pos, cam_rotation, 1.0));
        self.world.insert(cam, Camera::default());
        self.world.insert_resource(ActiveCamera(cam));

        // Initialize fly camera yaw/pitch from the initial look direction.
        {
            let mut input = self.world.resource_mut::<InputState>();
            input.yaw = forward.x.atan2(-forward.z);
            input.pitch = forward.y.asin();
        }

        let total_entities = self.world.next_entity_id();
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
                                    ctx.command_pool,
                                    w,
                                    h,
                                    &pixels,
                                ) {
                                    Ok(handle) => {
                                        self.ui_texture_handle = Some(handle);
                                        log::info!("UI texture registered (handle {})", handle);
                                    }
                                    Err(e) => log::error!("Failed to register UI texture: {e:#}"),
                                }
                                self.ui_manager = Some(ui);
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
}

/// Provides file data by searching BSA archives.
pub(crate) struct TextureProvider {
    texture_archives: Vec<byroredux_bsa::BsaArchive>,
    mesh_archives: Vec<byroredux_bsa::BsaArchive>,
}

impl TextureProvider {
    fn new() -> Self {
        Self {
            texture_archives: Vec::new(),
            mesh_archives: Vec::new(),
        }
    }

    /// Extract a texture (DDS) from texture BSAs.
    fn extract(&self, path: &str) -> Option<Vec<u8>> {
        for archive in &self.texture_archives {
            if let Ok(data) = archive.extract(path) {
                return Some(data);
            }
        }
        None
    }

    /// Extract a mesh (NIF) from mesh BSAs.
    pub(crate) fn extract_mesh(&self, path: &str) -> Option<Vec<u8>> {
        for archive in &self.mesh_archives {
            if let Ok(data) = archive.extract(path) {
                return Some(data);
            }
        }
        None
    }
}

/// Parse grid coordinates from a "x,y" string.
fn parse_grid_coords(s: &str) -> (i32, i32) {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() == 2 {
        let x = parts[0].trim().parse::<i32>().unwrap_or(0);
        let y = parts[1].trim().parse::<i32>().unwrap_or(0);
        (x, y)
    } else {
        log::warn!("Invalid grid format '{}', using (0,0)", s);
        (0, 0)
    }
}

/// Build a TextureProvider from CLI arguments.
fn build_texture_provider(args: &[String]) -> TextureProvider {
    let mut provider = TextureProvider::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--textures-bsa" => {
                if let Some(path) = args.get(i + 1) {
                    match byroredux_bsa::BsaArchive::open(path) {
                        Ok(a) => {
                            log::info!("Opened textures BSA: '{}'", path);
                            provider.texture_archives.push(a);
                        }
                        Err(e) => log::warn!("Failed to open textures BSA '{}': {}", path, e),
                    }
                    i += 2;
                    continue;
                }
            }
            "--bsa" => {
                if let Some(path) = args.get(i + 1) {
                    match byroredux_bsa::BsaArchive::open(path) {
                        Ok(a) => {
                            log::info!("Opened mesh BSA: '{}'", path);
                            provider.mesh_archives.push(a);
                        }
                        Err(e) => log::warn!("Failed to open mesh BSA '{}': {}", path, e),
                    }
                    i += 2;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
    provider
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
    let mut node_entities: Vec<EntityId> = Vec::with_capacity(imported.nodes.len());
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
        let vertices: Vec<Vertex> = (0..num_verts)
            .map(|i| {
                Vertex::new(
                    mesh.positions[i],
                    if i < mesh.colors.len() {
                        mesh.colors[i]
                    } else {
                        [1.0, 1.0, 1.0]
                    },
                    if i < mesh.normals.len() {
                        mesh.normals[i]
                    } else {
                        [0.0, 1.0, 0.0]
                    },
                    if i < mesh.uvs.len() {
                        mesh.uvs[i]
                    } else {
                        [0.0, 0.0]
                    },
                )
            })
            .collect();

        let alloc = ctx.allocator.as_ref().unwrap();
        let mesh_handle = match ctx.mesh_registry.upload(
            &ctx.device,
            alloc,
            &ctx.graphics_queue,
            ctx.command_pool,
            &vertices,
            &mesh.indices,
            ctx.device_caps.ray_query_supported,
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
                normal_map: mesh.normal_map.clone(),
            },
        );

        if let Some(ref name) = mesh.name {
            let mut pool = world.resource_mut::<StringPool>();
            let sym = pool.intern(name);
            drop(pool);
            world.insert(entity, Name(sym));
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

/// Resolve a texture path to a texture handle, with BSA lookup and caching.
fn resolve_texture(
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    tex_path: Option<&str>,
) -> u32 {
    let Some(tex_path) = tex_path else {
        return ctx.texture_registry.fallback();
    };
    if let Some(cached) = ctx.texture_registry.get_by_path(tex_path) {
        return cached;
    }
    if let Some(dds_bytes) = tex_provider.extract(tex_path) {
        let alloc = ctx.allocator.as_ref().unwrap();
        match ctx.texture_registry.load_dds(
            &ctx.device,
            alloc,
            &ctx.graphics_queue,
            ctx.command_pool,
            tex_path,
            &dds_bytes,
        ) {
            Ok(h) => {
                log::info!("Loaded DDS texture: '{}'", tex_path);
                return h;
            }
            Err(e) => {
                log::warn!("Failed to load DDS '{}': {}", tex_path, e);
            }
        }
    } else {
        log::debug!("Texture not found in BSA: '{}'", tex_path);
    }
    ctx.texture_registry.fallback()
}

/// Rotates only entities marked with the Spinning component.
fn spin_system(world: &World, dt: f32) {
    if let Some((sq, mut tq)) = world.query_2_mut::<Spinning, Transform>() {
        for (entity, _) in sq.iter() {
            if let Some(transform) = tq.get_mut(entity) {
                let rotation = Quat::from_rotation_y(dt * 1.0) * Quat::from_rotation_x(dt * 0.3);
                transform.rotation = rotation * transform.rotation;
            }
        }
    }
}

/// Logs engine stats once per second using DebugStats.
fn log_stats_system(world: &World, _dt: f32) {
    let config = world.resource::<EngineConfig>();
    if !config.debug_logging {
        return;
    }

    let total = world.resource::<TotalTime>().0;
    let dt = world.resource::<DeltaTime>().0;
    let prev = total - dt;

    if prev < 0.0 || total.floor() != prev.floor() {
        let stats = world.resource::<DebugStats>();
        log::info!(
            target: "engine::stats",
            "fps={:.0} avg={:.0} dt={:.2}ms entities={} meshes={} textures={} draws={}",
            stats.fps, stats.avg_fps(), stats.frame_time_ms,
            stats.entity_count, stats.mesh_count, stats.texture_count, stats.draw_call_count,
        );
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let config = WindowConfig::default();

        let win = match window::create_window(event_loop, &config) {
            Ok(w) => w,
            Err(e) => {
                log::error!("Failed to create window: {e:#}");
                event_loop.exit();
                return;
            }
        };

        let size = win.inner_size();
        let (display, window_handle) = match window::raw_handles(&win) {
            Ok(h) => h,
            Err(e) => {
                log::error!("Failed to get raw handles: {e:#}");
                event_loop.exit();
                return;
            }
        };

        match VulkanContext::new(display, window_handle, [size.width, size.height]) {
            Ok(ctx) => {
                self.renderer = Some(ctx);
                self.window = Some(win);
                self.last_frame = Instant::now();
                self.setup_scene();
                log::info!("Engine ready — entering game loop");
            }
            Err(e) => {
                log::error!("Vulkan init failed: {e:#}");
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                log::info!("Close requested — shutting down");
                self.renderer.take();
                self.window.take();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(ref mut ctx) = self.renderer {
                    if size.width > 0 && size.height > 0 {
                        if let Err(e) = ctx.recreate_swapchain([size.width, size.height]) {
                            log::error!("Swapchain recreate failed: {e:#}");
                            event_loop.exit();
                        }
                        // Update camera aspect ratio.
                        if let Some(active) = self.world.try_resource::<ActiveCamera>() {
                            let cam_entity = active.0;
                            drop(active);
                            if let Some(mut q) = self.world.query_mut::<Camera>() {
                                if let Some(cam) = q.get_mut(cam_entity) {
                                    cam.aspect = size.width as f32 / size.height as f32;
                                }
                            }
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(ref mut ctx) = self.renderer {
                    let (view_proj, draw_commands, gpu_lights, camera_pos, ambient) =
                        build_render_data(&self.world);

                    // Record draw call count for diagnostics.
                    world_resource_set::<DebugStats>(&self.world, |s| {
                        s.draw_call_count = draw_commands.len() as u32;
                    });

                    // Tick and render the UI overlay (Ruffle SWF player).
                    let mut ui_tex = None;
                    if let Some(ref mut ui) = self.ui_manager {
                        let dt = self
                            .world
                            .try_resource::<DeltaTime>()
                            .map(|d| d.0 as f64)
                            .unwrap_or(1.0 / 60.0);
                        let ui_w = ui.width;
                        let ui_h = ui.height;
                        ui.tick(dt);

                        if let Some(pixels) = ui.render() {
                            if let Some(handle) = self.ui_texture_handle {
                                let allocator = ctx.allocator.as_ref().unwrap();
                                if let Err(e) = ctx.texture_registry.update_rgba(
                                    &ctx.device,
                                    allocator,
                                    &ctx.graphics_queue,
                                    ctx.command_pool,
                                    handle,
                                    ui_w,
                                    ui_h,
                                    pixels,
                                ) {
                                    log::error!("UI texture update failed: {e:#}");
                                }
                                ui_tex = Some(handle);
                            }
                        } else if self.ui_texture_handle.is_some() {
                            // Not dirty, but still draw the last frame.
                            ui_tex = self.ui_texture_handle;
                        }
                    }

                    let color = Color::CORNFLOWER_BLUE;
                    match ctx.draw_frame(
                        color.as_array(),
                        &view_proj,
                        &draw_commands,
                        &gpu_lights,
                        camera_pos,
                        ambient,
                        ui_tex,
                    ) {
                        Ok(needs_recreate) => {
                            if needs_recreate {
                                if let Some(ref win) = self.window {
                                    let size = win.inner_size();
                                    if size.width > 0 && size.height > 0 {
                                        if let Err(e) =
                                            ctx.recreate_swapchain([size.width, size.height])
                                        {
                                            log::error!("Swapchain recreate failed: {e:#}");
                                            event_loop.exit();
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("Draw failed: {e:#}");
                            event_loop.exit();
                        }
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    let mut input = self.world.resource_mut::<InputState>();
                    match event.state {
                        ElementState::Pressed => {
                            // Escape toggles mouse capture.
                            if code == KeyCode::Escape && !event.repeat {
                                let captured = !input.mouse_captured;
                                input.mouse_captured = captured;
                                drop(input);
                                if let Some(ref win) = self.window {
                                    if captured {
                                        let _ =
                                            win.set_cursor_grab(CursorGrabMode::Confined).or_else(
                                                |_| win.set_cursor_grab(CursorGrabMode::Locked),
                                            );
                                        win.set_cursor_visible(false);
                                    } else {
                                        let _ = win.set_cursor_grab(CursorGrabMode::None);
                                        win.set_cursor_visible(true);
                                    }
                                }
                            } else {
                                input.keys_held.insert(code);
                            }
                        }
                        ElementState::Released => {
                            input.keys_held.remove(&code);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event {
            let mut input = self.world.resource_mut::<InputState>();
            if input.mouse_captured {
                let sensitivity = input.look_sensitivity;
                input.yaw -= delta.0 as f32 * sensitivity;
                input.pitch -= delta.1 as f32 * sensitivity;
                // Clamp pitch to avoid flipping.
                input.pitch = input.pitch.clamp(
                    -std::f32::consts::FRAC_PI_2 + 0.01,
                    std::f32::consts::FRAC_PI_2 - 0.01,
                );
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;

        // Update time resources.
        world_resource_set::<DeltaTime>(&self.world, |r| r.0 = dt);
        world_resource_set::<TotalTime>(&self.world, |r| r.0 += dt);

        // Update debug stats.
        {
            let mut stats = self.world.resource_mut::<DebugStats>();
            stats.push_frame_time(dt);
            stats.entity_count = self.world.next_entity_id();
            if let Some(ref ctx) = self.renderer {
                stats.mesh_count = ctx.mesh_registry.len() as u32;
                stats.texture_count = ctx.texture_registry.len() as u32;
            }
        }

        // Run all systems.
        self.scheduler.run(&self.world, dt);

        // Update window title with stats (throttled: every 16 frames ≈ 4×/sec at 60fps).
        let config_debug = self.world.resource::<EngineConfig>().debug_logging;
        if config_debug {
            let stats = self.world.resource::<DebugStats>();
            if stats.frame_index().is_multiple_of(16) {
                if let Some(ref win) = self.window {
                    win.set_title(&format!(
                        "ByroRedux | {:.0} FPS | {:.1}ms | {} entities | {} meshes | {} textures | {} draws",
                        stats.avg_fps(), stats.frame_time_ms,
                        stats.entity_count, stats.mesh_count, stats.texture_count, stats.draw_call_count,
                    ));
                }
            }
        }

        if let Some(ref win) = self.window {
            win.request_redraw();
        }
    }
}

// ── Console commands ────────────────────────────────────────────────────

struct HelpCommand;
impl ConsoleCommand for HelpCommand {
    fn name(&self) -> &str {
        "help"
    }
    fn description(&self) -> &str {
        "List all available commands"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let registry = world.resource::<CommandRegistry>();
        let mut lines = vec!["Available commands:".to_string()];
        for (name, desc) in registry.list() {
            lines.push(format!("  {:16} {}", name, desc));
        }
        CommandOutput::lines(lines)
    }
}

struct StatsCommand;
impl ConsoleCommand for StatsCommand {
    fn name(&self) -> &str {
        "stats"
    }
    fn description(&self) -> &str {
        "Show engine performance statistics"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let stats = world.resource::<DebugStats>();
        let (min_dt, max_dt) = stats.min_max_frame_time();
        CommandOutput::lines(vec![
            format!("FPS:       {:.0} (avg {:.0})", stats.fps, stats.avg_fps()),
            format!(
                "Frame:     {:.2}ms (min {:.2}ms, max {:.2}ms)",
                stats.frame_time_ms,
                min_dt * 1000.0,
                max_dt * 1000.0
            ),
            format!("Entities:  {}", stats.entity_count),
            format!("Meshes:    {}", stats.mesh_count),
            format!("Textures:  {}", stats.texture_count),
            format!("Draws:     {}", stats.draw_call_count),
        ])
    }
}

struct EntitiesCommand;
impl ConsoleCommand for EntitiesCommand {
    fn name(&self) -> &str {
        "entities"
    }
    fn description(&self) -> &str {
        "Show entity count and component breakdown"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let total = world.next_entity_id();
        let mut lines = vec![format!("Total entities spawned: {}", total)];
        lines.push(format!("  Transform:     {}", world.count::<Transform>()));
        lines.push(format!("  MeshHandle:    {}", world.count::<MeshHandle>()));
        lines.push(format!(
            "  TextureHandle: {}",
            world.count::<TextureHandle>()
        ));
        lines.push(format!("  Camera:        {}", world.count::<Camera>()));
        CommandOutput::lines(lines)
    }
}

struct SystemsCommand;
impl ConsoleCommand for SystemsCommand {
    fn name(&self) -> &str {
        "systems"
    }
    fn description(&self) -> &str {
        "List registered ECS systems"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        if let Some(list) = world.try_resource::<SystemList>() {
            let mut lines = vec![format!("Registered systems ({}):", list.0.len())];
            for (i, name) in list.0.iter().enumerate() {
                lines.push(format!("  [{}] {}", i, name));
            }
            CommandOutput::lines(lines)
        } else {
            CommandOutput::line("No system list available")
        }
    }
}

fn build_command_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    registry.register(HelpCommand);
    registry.register(StatsCommand);
    registry.register(EntitiesCommand);
    registry.register(SystemsCommand);
    registry
}

/// Build the view-projection matrix and draw command list from ECS queries.
fn build_render_data(
    world: &World,
) -> (
    [f32; 16],
    Vec<DrawCommand>,
    Vec<byroredux_renderer::GpuLight>,
    [f32; 3],
    [f32; 3],
) {
    use byroredux_core::math::Mat4;

    // Get camera view-projection.
    let view_proj = if let Some(active) = world.try_resource::<ActiveCamera>() {
        let cam_entity = active.0;
        drop(active);

        let cam_q = world.query::<Camera>();
        let transform_q = world.query::<Transform>();

        let vp = match (cam_q, transform_q) {
            (Some(cq), Some(tq)) => {
                let cam = cq.get(cam_entity);
                let t = tq.get(cam_entity);
                match (cam, t) {
                    (Some(c), Some(t)) => c.projection_matrix() * Camera::view_matrix(t),
                    _ => Mat4::IDENTITY,
                }
            }
            _ => Mat4::IDENTITY,
        };
        vp.to_cols_array()
    } else {
        Mat4::IDENTITY.to_cols_array()
    };

    // Collect draw commands from entities with (GlobalTransform, MeshHandle).
    // TextureHandle is optional — entities without one use the fallback (0).
    let mut draw_commands = Vec::new();
    if let Some((tq, mq)) = world.query_2_mut::<GlobalTransform, MeshHandle>() {
        let tex_q = world.query::<TextureHandle>();
        let alpha_q = world.query::<AlphaBlend>();
        let two_sided_q = world.query::<TwoSided>();
        let decal_q = world.query::<Decal>();
        let vis_q = world.query::<AnimatedVisibility>();
        for (entity, mesh) in mq.iter() {
            // Skip entities hidden by animation.
            let visible = vis_q
                .as_ref()
                .and_then(|q| q.get(entity))
                .map(|v| v.0)
                .unwrap_or(true);
            if !visible {
                continue;
            }

            if let Some(transform) = tq.get(entity) {
                let tex_handle = tex_q
                    .as_ref()
                    .and_then(|q| q.get(entity))
                    .map(|t| t.0)
                    .unwrap_or(0);
                let alpha_blend = alpha_q
                    .as_ref()
                    .map(|q| q.get(entity).is_some())
                    .unwrap_or(false);
                let two_sided = two_sided_q
                    .as_ref()
                    .map(|q| q.get(entity).is_some())
                    .unwrap_or(false);
                let is_decal = decal_q
                    .as_ref()
                    .map(|q| q.get(entity).is_some())
                    .unwrap_or(false);
                draw_commands.push(DrawCommand {
                    mesh_handle: mesh.0,
                    texture_handle: tex_handle,
                    model_matrix: transform.to_matrix().to_cols_array(),
                    alpha_blend,
                    two_sided,
                    is_decal,
                });
            }
        }
    }
    // Sort: opaque first, then alpha-blended; within each group sort by two_sided then texture.
    // Sort: opaque → decal → alpha; decals drawn after base geometry at same depth.
    draw_commands.sort_unstable_by_key(|cmd| {
        (
            cmd.alpha_blend,
            cmd.is_decal,
            cmd.two_sided,
            cmd.texture_handle,
        )
    });

    // Collect lights from ECS.
    let mut gpu_lights = Vec::new();

    // Add cell directional light (primary interior illumination).
    if let Some(cell_lit) = world.try_resource::<CellLightingRes>() {
        gpu_lights.push(byroredux_renderer::GpuLight {
            position_radius: [0.0, 0.0, 0.0, 0.0],
            color_type: [
                cell_lit.directional_color[0],
                cell_lit.directional_color[1],
                cell_lit.directional_color[2],
                2.0,
            ], // 2 = directional
            direction_angle: [
                cell_lit.directional_dir[0],
                cell_lit.directional_dir[1],
                cell_lit.directional_dir[2],
                0.0,
            ],
        });
    }

    // Add placed point lights from LIGH records.
    if let Some((tq, lq)) = world.query_2_mut::<GlobalTransform, LightSource>() {
        for (entity, light) in lq.iter() {
            if let Some(t) = tq.get(entity) {
                gpu_lights.push(byroredux_renderer::GpuLight {
                    position_radius: [
                        t.translation.x,
                        t.translation.y,
                        t.translation.z,
                        light.radius,
                    ],
                    color_type: [light.color[0], light.color[1], light.color[2], 0.0], // 0 = point
                    direction_angle: [0.0, 0.0, 0.0, 0.0],
                });
            }
        }
    }

    // Log light count once.
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        static LOGGED: AtomicBool = AtomicBool::new(false);
        if !LOGGED.swap(true, Ordering::Relaxed) {
            log::info!(
                "Lights collected: {} (first 3: {:?})",
                gpu_lights.len(),
                gpu_lights
                    .iter()
                    .take(3)
                    .map(|l| (l.position_radius, l.color_type))
                    .collect::<Vec<_>>(),
            );
        }
    }

    // Camera position.
    let camera_pos = if let Some(active) = world.try_resource::<ActiveCamera>() {
        let cam_entity = active.0;
        drop(active);
        let tq = world.query::<Transform>();
        tq.and_then(|q| {
            q.get(cam_entity)
                .map(|t| [t.translation.x, t.translation.y, t.translation.z])
        })
        .unwrap_or([0.0; 3])
    } else {
        [0.0; 3]
    };

    // Cell ambient color (or default).
    let ambient = world
        .try_resource::<CellLightingRes>()
        .map(|l| l.ambient)
        .unwrap_or([0.08, 0.08, 0.08]);

    (view_proj, draw_commands, gpu_lights, camera_pos, ambient)
}

/// Build a scoped name→entity map by walking the subtree rooted at `root`.
fn build_subtree_name_map(
    world: &World,
    root: EntityId,
) -> std::collections::HashMap<FixedString, EntityId> {
    let mut map = std::collections::HashMap::new();

    // Include the root itself.
    if let Some(nq) = world.query::<Name>() {
        if let Some(name) = nq.get(root) {
            map.insert(name.0, root);
        }
    }

    // BFS through children.
    let children_q = world.query::<Children>();
    let name_q = world.query::<Name>();
    let Some(ref cq) = children_q else { return map };

    let mut queue = vec![root];
    while let Some(entity) = queue.pop() {
        if let Some(children) = cq.get(entity) {
            for &child in &children.0 {
                if let Some(ref nq) = name_q {
                    if let Some(name) = nq.get(child) {
                        map.insert(name.0, child);
                    }
                }
                queue.push(child);
            }
        }
    }

    map
}

/// Add a child entity to a parent's Children component, creating it if needed.
fn add_child(world: &mut World, parent: EntityId, child: EntityId) {
    let has_children = world
        .query::<Children>()
        .map(|q| q.get(parent).is_some())
        .unwrap_or(false);

    if has_children {
        let mut cq = world.query_mut::<Children>().unwrap();
        cq.get_mut(parent).unwrap().0.push(child);
    } else {
        world.insert(parent, Children(vec![child]));
    }
}

fn world_resource_set<R: byroredux_core::ecs::Resource>(world: &World, f: impl FnOnce(&mut R)) {
    let mut guard = world.resource_mut::<R>();
    f(&mut guard);
}

/// Convert a NIF animation clip (byroredux_nif types) to a core animation clip (glam types).
fn convert_nif_clip(nif: &byroredux_nif::anim::AnimationClip) -> AnimationClip {
    use byroredux_nif::anim as na;

    let cycle_type = match nif.cycle_type {
        na::CycleType::Clamp => CycleType::Clamp,
        na::CycleType::Loop => CycleType::Loop,
        na::CycleType::Reverse => CycleType::Reverse,
    };

    let channels = nif
        .channels
        .iter()
        .map(|(name, ch)| {
            let convert_key_type = |kt: byroredux_nif::blocks::interpolator::KeyType| match kt {
                byroredux_nif::blocks::interpolator::KeyType::Linear => KeyType::Linear,
                byroredux_nif::blocks::interpolator::KeyType::Quadratic => KeyType::Quadratic,
                byroredux_nif::blocks::interpolator::KeyType::Tbc => KeyType::Tbc,
                byroredux_nif::blocks::interpolator::KeyType::XyzRotation => KeyType::Linear,
                byroredux_nif::blocks::interpolator::KeyType::Constant => KeyType::Linear,
            };

            let translation_keys = ch
                .translation_keys
                .iter()
                .map(|k| TranslationKey {
                    time: k.time,
                    value: Vec3::from_array(k.value),
                    forward: Vec3::from_array(k.forward),
                    backward: Vec3::from_array(k.backward),
                    tbc: k.tbc,
                })
                .collect();

            let rotation_keys = ch
                .rotation_keys
                .iter()
                .map(|k| RotationKey {
                    time: k.time,
                    value: Quat::from_xyzw(k.value[0], k.value[1], k.value[2], k.value[3]),
                    tbc: k.tbc,
                })
                .collect();

            let scale_keys = ch
                .scale_keys
                .iter()
                .map(|k| ScaleKey {
                    time: k.time,
                    value: k.value,
                    forward: k.forward,
                    backward: k.backward,
                    tbc: k.tbc,
                })
                .collect();

            (
                name.clone(),
                TransformChannel {
                    translation_keys,
                    translation_type: convert_key_type(ch.translation_type),
                    rotation_keys,
                    rotation_type: convert_key_type(ch.rotation_type),
                    scale_keys,
                    scale_type: convert_key_type(ch.scale_type),
                    priority: ch.priority,
                },
            )
        })
        .collect();

    let convert_float_target = |t: na::FloatTarget| match t {
        na::FloatTarget::Alpha => FloatTarget::Alpha,
        na::FloatTarget::UvOffsetU => FloatTarget::UvOffsetU,
        na::FloatTarget::UvOffsetV => FloatTarget::UvOffsetV,
        na::FloatTarget::UvScaleU => FloatTarget::UvScaleU,
        na::FloatTarget::UvScaleV => FloatTarget::UvScaleV,
        na::FloatTarget::UvRotation => FloatTarget::UvRotation,
        na::FloatTarget::ShaderFloat => FloatTarget::ShaderFloat,
    };

    let convert_color_target = |t: na::ColorTarget| match t {
        na::ColorTarget::Diffuse => ColorTarget::Diffuse,
        na::ColorTarget::Ambient => ColorTarget::Ambient,
        na::ColorTarget::Specular => ColorTarget::Specular,
        na::ColorTarget::Emissive => ColorTarget::Emissive,
        na::ColorTarget::ShaderColor => ColorTarget::ShaderColor,
    };

    let float_channels = nif
        .float_channels
        .iter()
        .map(|(name, ch)| {
            (
                name.clone(),
                FloatChannel {
                    target: convert_float_target(ch.target),
                    keys: ch
                        .keys
                        .iter()
                        .map(|k| AnimFloatKey {
                            time: k.time,
                            value: k.value,
                        })
                        .collect(),
                },
            )
        })
        .collect();

    let color_channels = nif
        .color_channels
        .iter()
        .map(|(name, ch)| {
            (
                name.clone(),
                ColorChannel {
                    target: convert_color_target(ch.target),
                    keys: ch
                        .keys
                        .iter()
                        .map(|k| AnimColorKey {
                            time: k.time,
                            value: Vec3::from_array(k.value),
                        })
                        .collect(),
                },
            )
        })
        .collect();

    let bool_channels = nif
        .bool_channels
        .iter()
        .map(|(name, ch)| {
            (
                name.clone(),
                BoolChannel {
                    keys: ch
                        .keys
                        .iter()
                        .map(|k| AnimBoolKey {
                            time: k.time,
                            value: k.value,
                        })
                        .collect(),
                },
            )
        })
        .collect();

    AnimationClip {
        name: nif.name.clone(),
        duration: nif.duration,
        cycle_type,
        frequency: nif.frequency,
        weight: nif.weight,
        accum_root_name: nif.accum_root_name.clone(),
        channels,
        float_channels,
        color_channels,
        bool_channels,
        text_keys: nif.text_keys.clone(),
    }
}
