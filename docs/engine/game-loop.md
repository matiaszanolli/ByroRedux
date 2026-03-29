# Game Loop

The game loop lives in the binary crate at `byroredux/src/main.rs`.
It uses winit's `ApplicationHandler` trait for event-driven execution.

## Startup Sequence

```
1. Initialize env_logger
2. Verify C++ bridge (native_hello())
3. Initialize scripting placeholder
4. Create World with resources:
   - DeltaTime(0.0)
   - TotalTime(0.0)
   - EngineConfig::default()
5. Create Scheduler with demo system (log_stats_system)
6. Enter winit event loop
```

## Event Flow

### `resumed` (once, on first window creation)

```
1. Create window (1280x720, "ByroRedux")
2. Get raw display + window handles
3. Initialize VulkanContext (full 11-step chain)
4. Record last_frame = Instant::now()
```

### `about_to_wait` (every frame, before events are polled)

This is the per-frame tick — the heart of the game loop:

```
1. Compute dt = now - last_frame
2. Update DeltaTime resource (via RwLock, &self)
3. Accumulate TotalTime resource
4. scheduler.run(&world, dt)    ← all systems execute here
5. window.request_redraw()      ← triggers RedrawRequested
```

### `RedrawRequested` (every frame, after systems)

```
1. draw_clear_frame(CORNFLOWER_BLUE)
2. Handle swapchain recreation if needed
```

### `CloseRequested`

```
1. Drop renderer (Vulkan teardown)
2. Drop window
3. Exit event loop
```

### `Resized`

```
1. Recreate swapchain with new dimensions
```

## Demo System: log_stats_system

Proves the game loop is live without spamming stdout. Fires once per
second by detecting integer-boundary crossings in `TotalTime`:

```rust
fn log_stats_system(world: &World, _dt: f32) {
    let total = world.resource::<TotalTime>().0;
    let dt = world.resource::<DeltaTime>().0;

    let prev = total - dt;
    if prev < 0.0 || total.floor() != prev.floor() {
        // Log once per second
        log::info!("[stats] total={:.1}s  dt={:.2}ms", total, dt * 1000.0);
    }
}
```

## Resource Update Pattern

Resources are updated through interior mutability — `about_to_wait`
only has `&mut self` on `App`, but `World` queries use `&self`:

```rust
fn world_resource_set<R: Resource>(world: &World, f: impl FnOnce(&mut R)) {
    let mut guard = world.resource_mut::<R>();
    f(&mut guard);
}

// Usage:
world_resource_set::<DeltaTime>(&self.world, |r| r.0 = dt);
world_resource_set::<TotalTime>(&self.world, |r| r.0 += dt);
```

## App Struct

```rust
struct App {
    window: Option<Window>,
    renderer: Option<VulkanContext>,
    world: World,
    scheduler: Scheduler,
    last_frame: Instant,
}
```

`window` and `renderer` are `Option` because they're created in `resumed`
(winit requires this — the event loop must be running before window creation).
`world` and `scheduler` are created immediately in `App::new()`.
