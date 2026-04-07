# Game Loop

The game loop lives in the binary crate at [`byroredux/src/main.rs`](../../byroredux/src/main.rs).
It uses winit's `ApplicationHandler` trait for event-driven execution. CLI
arguments choose between several scene-loading entry points; the default
is the spinning-cube demo. Once a scene is loaded, the per-frame tick runs
the same set of ECS systems regardless of how the scene was created.

## CLI entry points

| Args | Scene loaded |
|---|---|
| (none) | Spinning cube demo |
| `path/to/mesh.nif` | Loose NIF file |
| `path/to/mesh.nif --kf path/to/anim.kf` | NIF + KF animation playback |
| `--bsa path.bsa --mesh meshes\foo.nif` | Single NIF extracted from a BSA |
| `--bsa meshes.bsa --mesh meshes\foo.nif --textures-bsa textures.bsa` | + textures |
| `--esm FalloutNV.esm --cell CellID --bsa Meshes.bsa --textures-bsa Textures.bsa` | Interior cell from an ESM |
| `--swf path/to/menu.swf` | Scaleform SWF menu overlay |
| `--debug` | Enable per-frame stats in window title |
| `--cmd "console command"` | Run one console command at startup |

The CLI parser is in [`byroredux/src/main.rs`](../../byroredux/src/main.rs);
each entry point lives in a small module ([`scene.rs`](../../byroredux/src/scene.rs),
[`cell_loader.rs`](../../byroredux/src/cell_loader.rs)) that builds the
ECS world before the renderer is created.

## Startup Sequence

```
1. Parse CLI args (cli::Args::parse_from_env)
2. Initialize env_logger (RUST_LOG)
3. Verify C++ bridge (native_hello())
4. Initialize scripting (event types, timer system)
5. Create World with resources:
   - DeltaTime(0.0)
   - TotalTime(0.0)
   - EngineConfig::default()
   - StringPool::default()
   - AnimationClipRegistry::default()
   - DebugStats::default()
6. Build the asset provider (BSA / BA2 readers as needed)
7. Build the chosen scene (loose NIF / cell / SWF / demo cube)
8. Create the Scheduler with the standard system list:
   - input_system          (mouse + keyboard → camera move)
   - fly_camera_system     (apply input to camera transform)
   - timer_tick_system     (advance script timers, fire TimerExpired)
   - animation_system      (advance animation players, write transforms)
   - transform_propagate_system (parent → child global transform)
   - spin_system           (demo: spin entities with the Spinning marker)
   - event_cleanup_system  (drop transient marker components)
   - debug_stats_system    (collect per-frame timings into DebugStats)
9. Enter the winit event loop
```

The scheduler runs the same way whether the scene is a single sweetroll or
a full FNV interior — the systems list isn't customised per entry point.

## Event Flow

The `App` struct implements `winit::application::ApplicationHandler` so
each event lands on a method:

### `resumed` (once, on first window creation)

```
1. Create the winit Window (1280×720 default, "ByroRedux" title)
2. Get raw display + window handles
3. Initialize VulkanContext (full init chain — see renderer.md)
4. If a scene-on-renderer step is queued (mesh / texture upload), run it now
5. Record last_frame = Instant::now()
```

Vulkan needs the window handles, so context creation has to wait until
`resumed`. Mesh and texture uploads also need the device, so any "load
this NIF and upload it to the GPU" work runs here, not at startup. The
ECS world itself is built before the renderer exists, so non-GPU state is
ready immediately.

### `about_to_wait` (every frame, before events are polled)

This is the per-frame tick — the heart of the game loop:

```
1. now = Instant::now(); dt = now - last_frame; last_frame = now
2. Update DeltaTime + TotalTime resources via interior mutability
3. scheduler.run(&world, dt)    ← all systems execute here
4. window.request_redraw()      ← triggers RedrawRequested
```

### `RedrawRequested` (every frame, after systems)

```
1. Build per-frame render data from the ECS (build_render_data)
2. ctx.draw_frame(world, render_data, dt)
3. Handle swapchain-out-of-date via recreate_swapchain
4. Update window title with FPS/entity count if --debug is on
```

The `build_render_data` step in [`byroredux/src/render.rs`](../../byroredux/src/render.rs)
walks the ECS once per frame to collect visible meshes, their `MeshHandle`s,
their world transforms, materials, lights, and decal flags. The output is
plain owned data, so the renderer can chew on it without holding any ECS
locks during command recording.

### `WindowEvent::Resized`

```
1. ctx.recreate_swapchain(new_size)
```

See [Vulkan Renderer — Resize](renderer.md#resize) for the atomic-handoff
details.

### `WindowEvent::CloseRequested`

```
1. Drop the VulkanContext (waits for device idle, tears down in reverse)
2. Drop the window
3. event_loop.exit()
```

### Input events

`KeyboardInput`, `MouseMotion`, and `MouseButtonInput` are forwarded into
the `Input` resource, which the `input_system` consumes on the next frame
to drive the fly camera. Mouse motion is captured (cursor grabbed) when
the user presses **Escape** the first time.

## Resource update pattern

`about_to_wait` only has `&mut self` on `App`, but `World` queries use
`&self`. The two-line helper threads the mutation through interior
mutability:

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
    asset_provider: Arc<TextureProvider>,
    scene_setup: Option<Box<dyn FnOnce(&mut World, &mut VulkanContext, ...) + Send>>,
    last_frame: Instant,
    input: InputState,
    debug_args: DebugArgs,
}
```

`window` and `renderer` are `Option` because they're created in `resumed`
(winit requires the event loop to be running before window creation).
`world`, `scheduler`, and `asset_provider` are constructed in `App::new`
before the event loop starts. `scene_setup` is the deferred closure that
runs as soon as the renderer is up — that's where the cell loader, the
NIF demo, and the SWF menu loader all hook in.

## Per-frame timing reference

On the reference machine (RTX 4070 Ti, Ryzen 9 7950X) loading the FNV
Prospector Saloon (789 entities, 25 point lights + RT shadows):

| Stage | Cost |
|---|---|
| input + camera + transform propagation | <0.1 ms |
| animation tick (no clips active) | <0.05 ms |
| `build_render_data` | ~0.3 ms |
| TLAS rebuild | ~0.5 ms |
| Frame submit + present | ~10 ms total at 85 FPS |

Most of the wall-clock time is in the GPU path; the CPU side is well
under a millisecond per frame for cells of this size.
