//! Gamebyro Redux — ECS-driven game loop with spinning cube.

use anyhow::Result;
use gamebyro_core::ecs::{
    ActiveCamera, Camera, DeltaTime, EngineConfig, MeshHandle, Scheduler, TotalTime, Transform,
    World,
};
use gamebyro_core::math::{Quat, Vec3};
use gamebyro_core::types::Color;
use gamebyro_platform::window::{self, WindowConfig};
use gamebyro_renderer::vulkan::context::DrawCommand;
use gamebyro_renderer::{cube_vertices, quad_vertices, triangle_vertices, VulkanContext};
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("Gamebyro Redux starting");

    // Verify C++ bridge is linked.
    log::info!("{}", gamebyro_cxx_bridge::ffi::native_hello());

    // Initialize scripting placeholder.
    gamebyro_scripting::init();

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new();
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
    fn new() -> Self {
        let mut world = World::new();

        // Register built-in resources.
        world.insert_resource(DeltaTime(0.0));
        world.insert_resource(TotalTime(0.0));
        world.insert_resource(EngineConfig::default());

        // Build the system schedule.
        let mut scheduler = Scheduler::new();
        scheduler.add(spin_system);
        scheduler.add(log_stats_system);

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

        // Spawn cube entity (still spinning, now textured).
        let cube = self.world.spawn();
        self.world
            .insert(cube, Transform::from_translation(Vec3::new(-1.5, 0.0, 0.0)));
        self.world.insert(cube, MeshHandle(cube_handle));

        // Spawn textured quad — checkerboard visible.
        let quad = self.world.spawn();
        self.world.insert(
            quad,
            Transform::from_translation(Vec3::new(0.0, 0.0, -1.0)),
        );
        self.world.insert(quad, MeshHandle(quad_handle));

        // Spawn red triangle — closer to camera (Z = 0.5), offset right.
        let red_tri = self.world.spawn();
        self.world.insert(
            red_tri,
            Transform::from_translation(Vec3::new(1.5, 0.0, 0.5)),
        );
        self.world.insert(red_tri, MeshHandle(red_handle));

        // Spawn blue triangle — farther from camera (Z = -0.3), overlapping.
        let blue_tri = self.world.spawn();
        self.world.insert(
            blue_tri,
            Transform::from_translation(Vec3::new(1.8, 0.0, -0.3)),
        );
        self.world.insert(blue_tri, MeshHandle(blue_handle));

        // Spawn camera entity looking at the origin.
        let cam = self.world.spawn();
        let cam_pos = Vec3::new(0.0, 1.5, 4.0);
        let cam_target = Vec3::ZERO;
        let forward = (cam_target - cam_pos).normalize();
        let cam_rotation = Quat::from_rotation_arc(-Vec3::Z, forward);
        self.world.insert(
            cam,
            Transform::new(cam_pos, cam_rotation, 1.0),
        );
        self.world.insert(cam, Camera::default());
        self.world.insert_resource(ActiveCamera(cam));

        log::info!("Scene ready: 1 textured cube, 1 textured quad, 2 triangles, 1 camera");
    }
}

/// Rotates entities that have both Transform and MeshHandle (not cameras).
fn spin_system(world: &World, dt: f32) {
    if let Some((mq, mut tq)) = world.query_2_mut::<MeshHandle, Transform>() {
        for (entity, _mesh) in mq.iter() {
            if let Some(transform) = tq.get_mut(entity) {
                let rotation =
                    Quat::from_rotation_y(dt * 1.0) * Quat::from_rotation_x(dt * 0.3);
                transform.rotation = rotation * transform.rotation;
            }
        }
    }
}

/// Logs TotalTime and dt once per second.
fn log_stats_system(world: &World, _dt: f32) {
    let total = world.resource::<TotalTime>().0;
    let dt = world.resource::<DeltaTime>().0;

    let prev = total - dt;
    if prev < 0.0 || total.floor() != prev.floor() {
        let config = world.resource::<EngineConfig>();
        if config.debug_logging {
            log::info!("[stats] total={:.1}s  dt={:.2}ms", total, dt * 1000.0);
        }
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
                    // Build view-projection matrix from camera.
                    let (view_proj, draw_commands) = build_render_data(&self.world);

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
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;

        // Update time resources.
        world_resource_set::<DeltaTime>(&self.world, |r| r.0 = dt);
        world_resource_set::<TotalTime>(&self.world, |r| r.0 += dt);

        // Run all systems (spin, stats).
        self.scheduler.run(&self.world, dt);

        if let Some(ref win) = self.window {
            win.request_redraw();
        }
    }
}

/// Build the view-projection matrix and draw command list from ECS queries.
fn build_render_data(world: &World) -> ([f32; 16], Vec<DrawCommand>) {
    use gamebyro_core::math::Mat4;

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
    let mut draw_commands = Vec::new();
    if let Some((tq, mq)) = world.query_2_mut::<Transform, MeshHandle>() {
        // Iterate the smaller set (meshes), look up transforms.
        for (entity, mesh) in mq.iter() {
            if let Some(transform) = tq.get(entity) {
                draw_commands.push(DrawCommand {
                    mesh_handle: mesh.0,
                    model_matrix: transform.to_matrix().to_cols_array(),
                });
            }
        }
    }

    (view_proj, draw_commands)
}

fn world_resource_set<R: gamebyro_core::ecs::Resource>(
    world: &World,
    f: impl FnOnce(&mut R),
) {
    let mut guard = world.resource_mut::<R>();
    f(&mut guard);
}
