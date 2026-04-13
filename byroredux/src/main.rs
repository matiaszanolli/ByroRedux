//! ByroRedux — ECS-driven game loop with Vulkan rendering.

mod anim_convert;
mod asset_provider;
mod cell_loader;
mod commands;
mod components;
mod helpers;
mod render;
mod scene;
mod systems;

use anyhow::Result;
use byroredux_core::animation::AnimationClipRegistry;
use byroredux_core::console::CommandRegistry;
use byroredux_core::ecs::{
    ActiveCamera, Camera, DebugStats, DeltaTime, EngineConfig, Scheduler, Stage, TotalTime, World,
};
use byroredux_core::string::StringPool;
use byroredux_platform::window::{self, WindowConfig};
use byroredux_renderer::vulkan::context::DrawCommand;
use byroredux_renderer::VulkanContext;
use byroredux_ui::UiManager;
use std::collections::HashMap;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

use crate::commands::build_command_registry;
use crate::components::{InputState, NameIndex, SubtreeCache, SystemList};
use crate::helpers::world_resource_set;
use crate::render::build_render_data;
use crate::systems::{
    animation_system, billboard_system, fly_camera_system, log_stats_system,
    make_transform_propagation_system, make_world_bound_propagation_system, spin_system,
};

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
    /// Reusable per-frame draw command buffer (cleared each frame, allocation retained).
    draw_commands: Vec<DrawCommand>,
    /// Reusable per-frame light buffer (cleared each frame, allocation retained).
    gpu_lights: Vec<byroredux_renderer::GpuLight>,
    /// Reusable per-frame bone palette (column-major mat4 entries; slot 0
    /// always identity). Walked by `build_render_data` for every
    /// SkinnedMesh entity and uploaded once per frame.
    bone_palette: Vec<[[f32; 4]; 4]>,
    /// Reusable per-frame entity → bone-offset map. Populated by the
    /// skinned-mesh pass in `build_render_data` and read during draw
    /// command emission. Retained across frames so the HashMap's bucket
    /// allocation persists — see #253.
    skin_offsets: HashMap<byroredux_core::ecs::EntityId, u32>,
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
        world.insert_resource(SubtreeCache::new());
        world.insert_resource(byroredux_physics::PhysicsWorld::new());

        // Pre-register component storages that the physics sync system
        // queries on the first frame (before anything has been inserted).
        world.register::<byroredux_physics::RapierHandles>();

        // Register scripting component storages.
        byroredux_scripting::register(&mut world);

        // Build the system schedule — stages run sequentially, systems
        // within each stage run in parallel via rayon.
        let mut scheduler = Scheduler::new();
        scheduler.add_to(Stage::Early, fly_camera_system);
        scheduler.add_to(Stage::Early, byroredux_scripting::timer_tick_system);
        scheduler.add_to(Stage::Update, animation_system);
        scheduler.add_to(Stage::Update, spin_system);
        scheduler.add_to(Stage::PostUpdate, make_transform_propagation_system());
        // Billboards must run AFTER transform propagation so they can
        // overwrite the computed world rotation. Registered as exclusive
        // so the scheduler sequences it after the PostUpdate parallel
        // batch. See issue #225.
        scheduler.add_exclusive(Stage::PostUpdate, billboard_system);
        // Bound propagation runs last in PostUpdate so it sees final
        // world transforms (including billboard rotations). See #217.
        scheduler.add_exclusive(Stage::PostUpdate, make_world_bound_propagation_system());
        scheduler.add_to(Stage::Physics, byroredux_physics::physics_sync_system);
        scheduler.add_to(Stage::Late, log_stats_system);
        scheduler.add_exclusive(Stage::Late, byroredux_scripting::event_cleanup_system);

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
            draw_commands: Vec::new(),
            gpu_lights: Vec::new(),
            bone_palette: Vec::new(),
            skin_offsets: HashMap::new(),
        }
    }

    /// Called once after the renderer is ready — delegates to scene::setup_scene.
    fn setup_scene(&mut self) {
        let ctx = self.renderer.as_mut().unwrap();
        scene::setup_scene(
            &mut self.world,
            ctx,
            &mut self.ui_manager,
            &mut self.ui_texture_handle,
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
                self.renderer.as_ref().unwrap().log_memory_usage();
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
                    let (view_proj, camera_pos, ambient, fog_color, fog_near, fog_far) = build_render_data(
                        &self.world,
                        &mut self.draw_commands,
                        &mut self.gpu_lights,
                        &mut self.bone_palette,
                        &mut self.skin_offsets,
                    );

                    // Rebuild the global geometry SSBO if new meshes were
                    // loaded since the last build (cell transitions, late
                    // streaming). See #258.
                    if ctx.mesh_registry.is_geometry_dirty() {
                        if let Err(e) = ctx.mesh_registry.rebuild_geometry_ssbo(
                            &ctx.device,
                            ctx.allocator.as_ref().unwrap(),
                            &ctx.graphics_queue,
                            ctx.transfer_pool,
                            None, // TODO: thread StagingPool through frame loop (#242)
                        ) {
                            log::warn!("Failed to rebuild geometry SSBO: {e}");
                        }
                    }

                    // Record draw call count for diagnostics.
                    world_resource_set::<DebugStats>(&self.world, |s| {
                        s.draw_call_count = self.draw_commands.len() as u32;
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
                                    ctx.transfer_pool,
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

                    let color = byroredux_core::types::Color::CORNFLOWER_BLUE;
                    match ctx.draw_frame(
                        color.as_array(),
                        &view_proj,
                        &self.draw_commands,
                        &self.gpu_lights,
                        &self.bone_palette,
                        camera_pos,
                        ambient,
                        fog_color,
                        fog_near,
                        fog_far,
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
