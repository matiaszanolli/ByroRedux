//! Gamebyro Redux — ECS-driven game loop with Vulkan clear pass.

use anyhow::Result;
use gamebyro_core::ecs::{DeltaTime, EngineConfig, Scheduler, TotalTime, World};
use gamebyro_core::types::Color;
use gamebyro_platform::window::{self, WindowConfig};
use gamebyro_renderer::VulkanContext;
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
        scheduler.add(log_stats_system);

        Self {
            window: None,
            renderer: None,
            world,
            scheduler,
            last_frame: Instant::now(),
        }
    }
}

/// Logs TotalTime and dt once per second.
fn log_stats_system(world: &World, _dt: f32) {
    let total = world.resource::<TotalTime>().0;
    let dt = world.resource::<DeltaTime>().0;

    // Log once per second: fire when we cross an integer boundary.
    let prev = total - dt;
    if prev < 0.0 || total.floor() != prev.floor() {
        let config = world.resource::<EngineConfig>();
        if config.debug_logging {
            log::info!(
                "[stats] total={:.1}s  dt={:.2}ms",
                total,
                dt * 1000.0,
            );
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
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(ref mut ctx) = self.renderer {
                    let color = Color::CORNFLOWER_BLUE;
                    match ctx.draw_clear_frame(color.as_array()) {
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
        // ── Per-frame tick: update time resources, run systems, then render ──

        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;

        // Update time resources.
        world_resource_set::<DeltaTime>(&self.world, |r| r.0 = dt);
        world_resource_set::<TotalTime>(&self.world, |r| r.0 += dt);

        // Run all systems.
        self.scheduler.run(&self.world, dt);

        // Request redraw to trigger the Vulkan clear pass.
        if let Some(ref win) = self.window {
            win.request_redraw();
        }
    }
}

/// Helper: mutate a resource through the RwLock via &World.
fn world_resource_set<R: gamebyro_core::ecs::Resource>(
    world: &World,
    f: impl FnOnce(&mut R),
) {
    let mut guard = world.resource_mut::<R>();
    f(&mut guard);
}
