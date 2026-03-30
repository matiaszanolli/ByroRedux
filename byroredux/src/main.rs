//! ByroRedux — ECS-driven game loop with Vulkan rendering.

mod cell_loader;

use anyhow::Result;
use byroredux_core::console::{CommandOutput, CommandRegistry, ConsoleCommand};
use byroredux_core::ecs::{
    ActiveCamera, Camera, Component, DebugStats, DeltaTime, EngineConfig, MeshHandle, Scheduler,
    SparseSetStorage, TextureHandle, TotalTime, Transform, World,
};
use byroredux_core::ecs::Resource;
use byroredux_core::math::{Mat3, Quat, Vec3};
use byroredux_core::types::Color;
use byroredux_platform::window::{self, WindowConfig};
use byroredux_renderer::vulkan::context::DrawCommand;
use byroredux_renderer::{cube_vertices, quad_vertices, triangle_vertices, Vertex, VulkanContext};
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
impl Component for Spinning { type Storage = SparseSetStorage<Self>; }

/// Marker component for entities that use alpha blending.
#[derive(Debug, Clone, Copy)]
struct AlphaBlend;
impl Component for AlphaBlend { type Storage = SparseSetStorage<Self>; }

/// System names stored as a resource for the `systems` console command.
struct SystemList(Vec<String>);
impl Resource for SystemList {}

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
    let Some(active) = world.try_resource::<ActiveCamera>() else { return };
    let cam_entity = active.0;
    drop(active);

    let Some(input) = world.try_resource::<InputState>() else { return };
    if !input.mouse_captured {
        return;
    }

    let speed = input.move_speed * dt;
    let yaw = input.yaw;
    let pitch = input.pitch;

    // Build movement vector from held keys.
    let mut move_dir = Vec3::ZERO;
    if input.keys_held.contains(&KeyCode::KeyW) { move_dir.z += 1.0; }
    if input.keys_held.contains(&KeyCode::KeyS) { move_dir.z -= 1.0; }
    if input.keys_held.contains(&KeyCode::KeyA) { move_dir.x -= 1.0; }
    if input.keys_held.contains(&KeyCode::KeyD) { move_dir.x += 1.0; }
    if input.keys_held.contains(&KeyCode::Space) { move_dir.y += 1.0; }
    if input.keys_held.contains(&KeyCode::ShiftLeft) { move_dir.y -= 1.0; }

    // Speed boost with Ctrl.
    let boost = if input.keys_held.contains(&KeyCode::ControlLeft) { 3.0 } else { 1.0 };
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

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let debug_mode = args.iter().any(|a| a == "--debug");

    // Set up logging. --debug forces debug level.
    if debug_mode {
        std::env::set_var("RUST_LOG", std::env::var("RUST_LOG").unwrap_or("debug".into()));
    }
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("ByroRedux starting");
    log::info!("{}", byroredux_cxx_bridge::ffi::native_hello());

    // Headless --cmd mode: execute command and exit without creating a window.
    if let Some(cmd_idx) = args.iter().position(|a| a == "--cmd") {
        let input = args.get(cmd_idx + 1).map(|s| s.as_str()).unwrap_or("help");
        let mut world = World::new();
        world.insert_resource(DebugStats::default());
        world.insert_resource(EngineConfig { debug_logging: true, ..Default::default() });
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

        // Register scripting component storages.
        byroredux_scripting::register(&mut world);

        // Build the system schedule.
        let mut scheduler = Scheduler::new();
        scheduler.add(fly_camera_system);
        scheduler.add(spin_system);
        scheduler.add(byroredux_scripting::timer_tick_system);
        scheduler.add(log_stats_system);
        scheduler.add(byroredux_scripting::event_cleanup_system);

        // Store system names + console commands as resources.
        let system_names: Vec<String> = scheduler.system_names().iter().map(|s| s.to_string()).collect();
        world.insert_resource(SystemList(system_names));
        world.insert_resource(build_command_registry());

        Self {
            window: None,
            renderer: None,
            world,
            scheduler,
            last_frame: Instant::now(),
        }
    }

    /// Called once after the renderer is ready — uploads meshes and spawns entities.
    fn setup_scene(&mut self) {
        let ctx = self.renderer.as_mut().unwrap();
        let alloc = ctx.allocator.as_ref().unwrap();

        // Upload cube mesh.
        let (verts, idxs) = cube_vertices();
        let cube_handle = ctx
            .mesh_registry
            .upload(&ctx.device, alloc, &verts, &idxs)
            .expect("Failed to upload cube mesh");

        // Upload a textured quad.
        let (quad_verts, quad_idxs) = quad_vertices();
        let quad_handle = ctx
            .mesh_registry
            .upload(&ctx.device, alloc, &quad_verts, &quad_idxs)
            .expect("Failed to upload quad mesh");

        // Upload two triangle meshes with different colors.
        let (red_verts, red_idxs) = triangle_vertices([1.0, 0.2, 0.2]);
        let red_handle = ctx
            .mesh_registry
            .upload(&ctx.device, alloc, &red_verts, &red_idxs)
            .expect("Failed to upload red triangle mesh");

        let (blue_verts, blue_idxs) = triangle_vertices([0.2, 0.2, 1.0]);
        let blue_handle = ctx
            .mesh_registry
            .upload(&ctx.device, alloc, &blue_verts, &blue_idxs)
            .expect("Failed to upload blue triangle mesh");

        // Spawn cube entity (spinning demo).
        let cube = self.world.spawn();
        self.world
            .insert(cube, Transform::from_translation(Vec3::new(-1.5, 0.0, 0.0)));
        self.world.insert(cube, MeshHandle(cube_handle));
        self.world.insert(cube, Spinning);

        // Spawn textured quad — checkerboard visible.
        let quad = self.world.spawn();
        self.world.insert(
            quad,
            Transform::from_translation(Vec3::new(0.0, 0.0, -1.0)),
        );
        self.world.insert(quad, MeshHandle(quad_handle));
        self.world.insert(quad, Spinning);

        // Spawn red triangle — closer to camera (Z = 0.5), offset right.
        let red_tri = self.world.spawn();
        self.world.insert(
            red_tri,
            Transform::from_translation(Vec3::new(1.5, 0.0, 0.5)),
        );
        self.world.insert(red_tri, MeshHandle(red_handle));
        self.world.insert(red_tri, Spinning);

        // Spawn blue triangle — farther from camera (Z = -0.3), overlapping.
        let blue_tri = self.world.spawn();
        self.world.insert(
            blue_tri,
            Transform::from_translation(Vec3::new(1.8, 0.0, -0.3)),
        );
        self.world.insert(blue_tri, MeshHandle(blue_handle));
        self.world.insert(blue_tri, Spinning);

        // Load content from CLI: cell, loose NIF, or BSA NIF.
        let args: Vec<String> = std::env::args().collect();
        let mut cam_center = Vec3::ZERO;
        let mut has_nif_content = false;

        // Cell loading mode: --esm <path> --cell <editor_id>
        if let Some(esm_idx) = args.iter().position(|a| a == "--esm") {
            if let (Some(esm_path), Some(cell_id)) = (
                args.get(esm_idx + 1),
                args.iter().position(|a| a == "--cell").and_then(|i| args.get(i + 1)),
            ) {
                let tex_provider = build_texture_provider(&args);
                match cell_loader::load_cell(esm_path, cell_id, &mut self.world, ctx, &tex_provider) {
                    Ok(result) => {
                        cam_center = result.center;
                        has_nif_content = true;
                        log::info!("Cell '{}' ready: {} entities", result.cell_name, result.entity_count);
                    }
                    Err(e) => log::error!("Failed to load cell: {:#}", e),
                }
            } else {
                log::error!("--esm requires --cell <editor_id>");
            }
        } else {
            // NIF loading mode: loose file or BSA extraction.
            let nif_count = load_nif_from_args(&mut self.world, ctx);
            has_nif_content = nif_count > 0;
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
        self.world.insert(
            cam,
            Transform::new(cam_pos, cam_rotation, 1.0),
        );
        self.world.insert(cam, Camera::default());
        self.world.insert_resource(ActiveCamera(cam));

        // Initialize fly camera yaw/pitch from the initial look direction.
        {
            let mut input = self.world.resource_mut::<InputState>();
            input.yaw = forward.x.atan2(-forward.z);
            input.pitch = forward.y.asin();
        }

        let total_entities = self.world.entity_count();
        log::info!("Scene ready: {} entities, 1 camera. Press Escape to capture mouse for fly camera.", total_entities);
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
fn load_nif_from_args(world: &mut World, ctx: &mut VulkanContext) -> usize {
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
            None => { log::error!("--bsa requires an archive path"); return 0; }
        };
        let mesh_path = match args.iter().position(|a| a == "--mesh").and_then(|i| args.get(i + 1)) {
            Some(p) => p,
            None => { log::error!("--bsa requires --mesh <path>"); return 0; }
        };

        let archive = match byroredux_bsa::BsaArchive::open(bsa_path) {
            Ok(a) => a,
            Err(e) => { log::error!("Failed to open BSA '{}': {}", bsa_path, e); return 0; }
        };
        let data = match archive.extract(mesh_path) {
            Ok(d) => d,
            Err(e) => { log::error!("Failed to extract '{}': {}", mesh_path, e); return 0; }
        };
        log::info!("Extracted {} bytes from BSA '{}'", data.len(), mesh_path);
        load_nif_bytes(world, ctx, &data, mesh_path, &tex_provider)
    } else if let Some(nif_path) = args.get(1) {
        if nif_path.starts_with("--") {
            return 0; // Skip flags that aren't NIF paths
        }
        // Loose file mode: <path.nif>
        let data = match std::fs::read(nif_path) {
            Ok(d) => d,
            Err(e) => { log::error!("Failed to read NIF file '{}': {}", nif_path, e); return 0; }
        };
        load_nif_bytes(world, ctx, &data, nif_path, &tex_provider)
    } else {
        0
    }
}

/// Parse NIF bytes, import meshes, upload to GPU, load textures, and spawn ECS entities.
pub(crate) fn load_nif_bytes(
    world: &mut World,
    ctx: &mut VulkanContext,
    data: &[u8],
    label: &str,
    tex_provider: &TextureProvider,
) -> usize {
    let scene = match byroredux_nif::parse_nif(data) {
        Ok(s) => s,
        Err(e) => {
            log::error!("Failed to parse NIF '{}': {}", label, e);
            return 0;
        }
    };

    let imported = byroredux_nif::import::import_nif(&scene);
    let alloc = ctx.allocator.as_ref().unwrap();
    let mut count = 0;

    for mesh in &imported {
        // Build renderer Vertex array from imported data
        let num_verts = mesh.positions.len();
        let vertices: Vec<Vertex> = (0..num_verts)
            .map(|i| {
                Vertex::new(
                    mesh.positions[i],
                    if i < mesh.colors.len() { mesh.colors[i] } else { [1.0, 1.0, 1.0] },
                    if i < mesh.normals.len() { mesh.normals[i] } else { [0.0, 1.0, 0.0] },
                    if i < mesh.uvs.len() { mesh.uvs[i] } else { [0.0, 0.0] },
                )
            })
            .collect();

        let mesh_handle = match ctx.mesh_registry.upload(&ctx.device, alloc, &vertices, &mesh.indices) {
            Ok(h) => h,
            Err(e) => {
                log::warn!("Failed to upload NIF mesh '{}': {}", mesh.name.as_deref().unwrap_or("?"), e);
                continue;
            }
        };

        // Load DDS texture if the mesh has a texture path.
        let tex_handle = match &mesh.texture_path {
            Some(tex_path) => {
                // Check cache first.
                if let Some(cached) = ctx.texture_registry.get_by_path(tex_path) {
                    cached
                } else if let Some(dds_bytes) = tex_provider.extract(tex_path) {
                    match ctx.texture_registry.load_dds(
                        &ctx.device,
                        alloc,
                        ctx.graphics_queue,
                        ctx.command_pool,
                        tex_path,
                        &dds_bytes,
                    ) {
                        Ok(h) => {
                            log::info!("Loaded DDS texture: '{}'", tex_path);
                            h
                        }
                        Err(e) => {
                            log::warn!("Failed to load DDS '{}': {}", tex_path, e);
                            ctx.texture_registry.fallback()
                        }
                    }
                } else {
                    log::debug!("Texture not found in BSA: '{}'", tex_path);
                    ctx.texture_registry.fallback()
                }
            }
            None => ctx.texture_registry.fallback(),
        };

        // Convert NiTransform to ECS Transform
        let rotation = Mat3::from_cols(
            Vec3::new(mesh.rotation[0][0], mesh.rotation[1][0], mesh.rotation[2][0]),
            Vec3::new(mesh.rotation[0][1], mesh.rotation[1][1], mesh.rotation[2][1]),
            Vec3::new(mesh.rotation[0][2], mesh.rotation[1][2], mesh.rotation[2][2]),
        );
        let quat = Quat::from_mat3(&rotation);
        let translation = Vec3::new(mesh.translation[0], mesh.translation[1], mesh.translation[2]);

        let entity = world.spawn();
        world.insert(entity, Transform::new(translation, quat, mesh.scale));
        world.insert(entity, MeshHandle(mesh_handle));
        world.insert(entity, TextureHandle(tex_handle));
        if mesh.has_alpha {
            world.insert(entity, AlphaBlend);
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

    log::info!("Imported {} meshes from '{}'", count, label);
    count
}

/// Rotates only entities marked with the Spinning component.
fn spin_system(world: &World, dt: f32) {
    if let Some((sq, mut tq)) = world.query_2_mut::<Spinning, Transform>() {
        for (entity, _) in sq.iter() {
            if let Some(transform) = tq.get_mut(entity) {
                let rotation =
                    Quat::from_rotation_y(dt * 1.0) * Quat::from_rotation_x(dt * 0.3);
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
                    let (view_proj, draw_commands) = build_render_data(&self.world);

                    // Record draw call count for diagnostics.
                    world_resource_set::<DebugStats>(&self.world, |s| {
                        s.draw_call_count = draw_commands.len() as u32;
                    });

                    let color = Color::CORNFLOWER_BLUE;
                    match ctx.draw_frame(color.as_array(), &view_proj, &draw_commands) {
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
                                        let _ = win.set_cursor_grab(CursorGrabMode::Confined)
                                            .or_else(|_| win.set_cursor_grab(CursorGrabMode::Locked));
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
            stats.entity_count = self.world.entity_count() as u32;
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
            if stats.frame_index() % 16 == 0 {
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
    fn name(&self) -> &str { "help" }
    fn description(&self) -> &str { "List all available commands" }
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
    fn name(&self) -> &str { "stats" }
    fn description(&self) -> &str { "Show engine performance statistics" }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let stats = world.resource::<DebugStats>();
        let (min_dt, max_dt) = stats.min_max_frame_time();
        CommandOutput::lines(vec![
            format!("FPS:       {:.0} (avg {:.0})", stats.fps, stats.avg_fps()),
            format!("Frame:     {:.2}ms (min {:.2}ms, max {:.2}ms)", stats.frame_time_ms, min_dt * 1000.0, max_dt * 1000.0),
            format!("Entities:  {}", stats.entity_count),
            format!("Meshes:    {}", stats.mesh_count),
            format!("Textures:  {}", stats.texture_count),
            format!("Draws:     {}", stats.draw_call_count),
        ])
    }
}

struct EntitiesCommand;
impl ConsoleCommand for EntitiesCommand {
    fn name(&self) -> &str { "entities" }
    fn description(&self) -> &str { "Show entity count and component breakdown" }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let total = world.entity_count();
        let mut lines = vec![format!("Total entities spawned: {}", total)];
        lines.push(format!("  Transform:     {}", world.count::<Transform>()));
        lines.push(format!("  MeshHandle:    {}", world.count::<MeshHandle>()));
        lines.push(format!("  TextureHandle: {}", world.count::<TextureHandle>()));
        lines.push(format!("  Camera:        {}", world.count::<Camera>()));
        CommandOutput::lines(lines)
    }
}

struct SystemsCommand;
impl ConsoleCommand for SystemsCommand {
    fn name(&self) -> &str { "systems" }
    fn description(&self) -> &str { "List registered ECS systems" }
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
fn build_render_data(world: &World) -> ([f32; 16], Vec<DrawCommand>) {
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

    // Collect draw commands from entities with (Transform, MeshHandle).
    // TextureHandle is optional — entities without one use the fallback (0).
    let mut draw_commands = Vec::new();
    if let Some((tq, mq)) = world.query_2_mut::<Transform, MeshHandle>() {
        let tex_q = world.query::<TextureHandle>();
        let alpha_q = world.query::<AlphaBlend>();
        for (entity, mesh) in mq.iter() {
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
                draw_commands.push(DrawCommand {
                    mesh_handle: mesh.0,
                    texture_handle: tex_handle,
                    model_matrix: transform.to_matrix().to_cols_array(),
                    alpha_blend,
                });
            }
        }
    }
    // Sort: opaque first (alpha_blend=false), then by texture to minimize rebinds.
    draw_commands.sort_unstable_by_key(|cmd| (cmd.alpha_blend, cmd.texture_handle));

    (view_proj, draw_commands)
}

fn world_resource_set<R: byroredux_core::ecs::Resource>(
    world: &World,
    f: impl FnOnce(&mut R),
) {
    let mut guard = world.resource_mut::<R>();
    f(&mut guard);
}
