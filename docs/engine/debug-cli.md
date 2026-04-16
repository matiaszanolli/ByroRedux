# Debug CLI (`byro-dbg`)

An external debugger for live ECS inspection. Connects to the running engine
over TCP and lets you query, modify, and screenshot the scene using
Papyrus-style expression syntax — without restarting the engine.

**Crates:** `crates/debug-protocol/`, `crates/debug-server/`, `tools/byro-dbg/` | **Tests:** 4 (wire round-trip)

## Why an External Debugger?

As scenes grow in complexity (exterior cells, 1500+ entities, RT lighting),
`println!` debugging breaks down. The existing `--cmd` console commands
(`stats`, `entities`, `systems`) are fire-and-forget at launch. We need:

1. **Live inspection** — query any entity, component, or resource while the
   engine is running
2. **Mutation** — tweak transforms, materials, and light properties without
   recompiling
3. **Screenshots** — capture the composited frame from the CLI for automated
   visual regression or issue reporting
4. **Separation** — a standalone process that doesn't pollute the engine's
   hot path, with zero cost when no debugger is connected

## Architecture

```
┌─────────────┐    TCP (JSON)    ┌──────────────────────────┐
│  byro-dbg   │◄───────────────►│  debug-server            │
│  (CLI REPL) │  localhost:9876  │  Late-stage exclusive sys │
└─────────────┘                  └──────────┬───────────────┘
                                            │ &World
                                 ┌──────────▼───────────────┐
                                 │  ComponentRegistry        │
                                 │  (type-erased get/set/list)│
                                 └──────────────────────────┘
```

### Data flow

1. The **TCP listener** runs on a background thread (`std::net`, non-blocking
   accept). Each client gets a reader/writer thread pair.
2. Incoming requests are pushed into a `Mutex<Vec<PendingCommand>>` queue.
3. The **drain system** (exclusive, `Stage::Late`) pops the queue once per
   frame after all gameplay systems have run.
4. The **evaluator** parses the expression via `byroredux_papyrus::parse_expr()`,
   walks the AST against `&World` + `ComponentRegistry`, and returns JSON.
5. The response travels back through `mpsc` channel → writer thread → TCP.

### Why not tokio?

A human-speed REPL issuing 1-2 requests per second doesn't need async I/O.
`std::net` threads + a mutex queue is simpler, adds no dependency, and
keeps the render thread completely free of async runtime overhead.

## Wire Protocol

Length-prefixed JSON over TCP: `[4-byte big-endian length][UTF-8 JSON payload]`.

- Max message: 16 MB (sanity check, not a practical limit)
- Human-debuggable with `netcat` or `socat`
- Serialization: `serde_json` (workspace dependency)

Request/response types are defined in `crates/debug-protocol/src/lib.rs`:

| Request | What it does |
|---------|-------------|
| `Eval { expr }` | Parse + evaluate a Papyrus expression |
| `ListEntities { component? }` | List entities (optionally filtered by component) |
| `GetComponent { entity, component }` | Get full component data as JSON |
| `SetField { entity, component, path, value }` | Modify a single field |
| `ListComponents` | List all registered inspectable types |
| `ListSystems` | List ECS systems in stage order |
| `Stats` | FPS, frame time, entity/mesh/texture/draw counts |
| `FindEntity { name }` | Find entity by `Name` component |
| `Screenshot { path? }` | Capture the composited frame as PNG |
| `Ping` | Keep-alive / connection check |

## Component Registry

The ECS is compile-time typed — there's no runtime reflection. The
`ComponentRegistry` bridges this gap with type-erased closures:

```rust
struct ComponentDescriptor {
    name: &'static str,
    field_names: Vec<&'static str>,
    get_json:   Box<dyn Fn(&World, EntityId) -> Option<Value>>,
    set_field:  Box<dyn Fn(&World, EntityId, &str, Value) -> Result<()>>,
    list_entities: Box<dyn Fn(&World) -> Vec<EntityId>>,
    // ...
}
```

Each component is registered in `crates/debug-server/src/registration.rs`
using the generic `register_component::<T>()` helper. Components must derive
`Serialize + Deserialize` (gated behind the `inspect` feature on
`byroredux-core`).

### Currently registered (15 components)

| Component | Fields |
|-----------|--------|
| `Transform` | translation, rotation, scale |
| `GlobalTransform` | translation, rotation, scale |
| `Camera` | fov_y, near, far, aspect |
| `LightSource` | radius, color, flags |
| `Material` | emissive_color, emissive_mult, specular_color, specular_strength, glossiness, uv_offset, uv_scale, alpha, env_map_scale, normal_map, texture_path, glow_map, detail_map, gloss_map, dark_map, vertex_color_mode, alpha_test, alpha_threshold, alpha_test_func |
| `LocalBound` | center, radius |
| `WorldBound` | center, radius |
| `Billboard` | mode |
| `MeshHandle` | (tuple: u32) |
| `TextureHandle` | (tuple: u32) |
| `BSXFlags` | (tuple: u32) |
| `BSBound` | center, half_extents |
| `AnimatedVisibility` | (tuple: bool) |
| `AnimatedAlpha` | (tuple: f32) |
| `AnimatedColor` | (tuple: Vec3) |

To add a new component: derive `Serialize`/`Deserialize` (behind `#[cfg_attr(feature = "inspect", ...)]`),
then add a `register_component::<T>()` call in `registration.rs`.

## Expression Language

The evaluator reuses the Papyrus expression parser. Supported patterns:

### Entity resolution

```
42                          → entity by ID
find("TorchSconce01")      → entity by Name
"TorchSconce01"             → entity by Name (shorthand)
TorchSconce01               → entity by Name (identifier)
```

### Component access (member access chains)

```
42.Transform                → full component as JSON
42.Transform.translation    → specific field
42.Transform.translation.x  → nested field (glam x/y/z/w aliases)
```

The evaluator flattens nested `MemberAccess` AST nodes into a chain:
`[root, component_name, field, subfield, ...]`. The root resolves to an
entity ID, the first member resolves via `ComponentRegistry`, and remaining
members drill into the JSON value.

### Built-in functions

```
find("name")               → entity lookup by Name
entities(Transform)         → list entities with component
count(LightSource)          → count entities with component
stats                       → engine performance stats
systems                     → list registered ECS systems
components                  → list inspectable component types
tex_missing()               → textures referenced but never loaded
tex_loaded()                → currently-resident textures + byte size
```

Session-10 console commands (server-side, not evaluator):

```
tex.missing                 → same as tex_missing() but human formatted
tex.loaded                  → same as tex_loaded(), sorted by size
mesh.info <entity_id>       → material + texture paths + BGSM reference
                             (shows material_path when texture_path is absent —
                              correct FO4 behaviour since the real material
                              lives in the external BGSM/BGEM file)
```

### Mutation

Field-level set uses the `SetField` protocol message. The evaluator reads
the component → serializes to JSON → modifies the field → deserializes back
→ writes via `query_mut`. Full-component replacement is not yet supported.

## Screenshot Capture

The `screenshot` command captures the composited frame from the Vulkan
swapchain and encodes it as PNG. The capture spans two frames:

```
Frame N:   drain system receives screenshot request
           → sets ScreenshotBridge.requested (AtomicBool)

Frame N+1: draw_frame()
           → fence wait
           → screenshot_finish_readback() [if pending from earlier]
           → render composite to swapchain image
           → screenshot_record_copy():
               barrier PRESENT_SRC → TRANSFER_SRC
               vkCmdCopyImageToBuffer → host-visible staging
               barrier TRANSFER_SRC → PRESENT_SRC
           → submit + present

Frame N+2: draw_frame()
           → fence wait (GPU done with copy)
           → screenshot_finish_readback():
               map staging buffer
               BGRA→RGBA conversion
               PNG encode (image crate)
               store in ScreenshotBridge.result

           drain system polls result → save to file → respond to client
```

### Implementation details

- Swapchain images have `TRANSFER_SRC` usage flag (added alongside
  `COLOR_ATTACHMENT` at creation)
- Staging buffer is `GpuToCpu` memory (host-visible, GPU-writable),
  allocated on first screenshot and reused
- Copy happens inside the same command buffer as rendering — no extra
  GPU submission or sync
- Swapchain format is `B8G8R8A8_SRGB` — pixels are converted to RGBA
  during readback

### Relevant files

| File | Role |
|------|------|
| `crates/renderer/src/vulkan/context/screenshot.rs` | Copy commands, staging buffer, PNG encode |
| `crates/renderer/src/vulkan/context/mod.rs` | `ScreenshotHandle` (Arc-shared request/result) |
| `crates/core/src/ecs/resources.rs` | `ScreenshotBridge` (Resource, bridges renderer↔server) |
| `crates/debug-server/src/system.rs` | Multi-frame screenshot flow in drain system |

## Feature Gating

Everything is behind feature flags to ensure zero cost in release builds:

| Feature | Crate | What it gates |
|---------|-------|--------------|
| `inspect` | `byroredux-core` | `serde::Serialize + Deserialize` on components, `serde`/`serde_json`/`glam/serde` deps |
| `debug-server` | `byroredux` (binary) | `byroredux-debug-server` dep, startup code in `main.rs` |

`debug-server` is **on by default** in the binary. Disable with
`--no-default-features` for release builds.

### Per-frame cost when enabled but idle

1. One `Mutex::lock()` + `Vec::is_empty()` check (the command queue)
2. One `AtomicBool::load()` (screenshot request flag)
3. Non-blocking `TcpListener::accept()` on the background thread (50ms sleep between polls)

Total: sub-microsecond on the render thread.

## File Layout

```
crates/debug-protocol/
  src/
    lib.rs                  DebugRequest, DebugResponse enums
    wire.rs                 Length-prefixed JSON encode/decode (4 tests)
    registry.rs             ComponentDescriptor, ComponentRegistry

crates/debug-server/
  src/
    lib.rs                  start() entry point
    listener.rs             TcpListener, per-client threads, command queue
    system.rs               DebugDrainSystem (Late-stage exclusive)
    evaluator.rs            Papyrus AST → ECS query evaluation
    registration.rs         register_component::<T>() for 15 types

tools/byro-dbg/
  src/
    main.rs                 TCP client, REPL loop, shorthand parsing
    display.rs              Pretty-print responses
```

## Usage

### Starting the engine with debug server

```bash
cargo run                                   # default port 9876
BYRO_DEBUG_PORT=8080 cargo run              # custom port
cargo run --no-default-features             # disabled (release)
```

### Connecting with the CLI

```bash
cargo run -p byro-dbg                       # connect to localhost:9876
BYRO_DEBUG_PORT=8080 cargo run -p byro-dbg  # custom port
```

### Example session

```
$ cargo run -p byro-dbg
Connected to ByroRedux at 127.0.0.1:9876

byro> stats
FPS: 60.2 (avg 59.8) | Frame: 16.61ms | Entities: 1547 | Meshes: 342 | Textures: 128 | Draws: 286

byro> find("TorchSconce01")
  Entity 142 "TorchSconce01"
(1 entities)

byro> 142.Transform
{
  "translation": [1024.0, 512.0, 128.0],
  "rotation": [0.0, 0.0, 0.0, 1.0],
  "scale": 1.0
}

byro> 142.Transform.translation.x
1024.0

byro> 142.LightSource
{
  "radius": 512.0,
  "color": [1.0, 0.8, 0.6],
  "flags": 0
}

byro> entities(LightSource)
  Entity 10 "CandleFlame"
  Entity 142 "TorchSconce01"
  Entity 305 "FlameBrazier"
(3 entities)

byro> count(Transform)
1547

byro> components
  AnimatedAlpha
  AnimatedColor
  AnimatedVisibility
  BSBound
  BSXFlags
  Billboard
  Camera
  GlobalTransform
  LightSource
  LocalBound
  Material
  MeshHandle
  TextureHandle
  Transform
  WorldBound
(15 components)

byro> systems
  [0] fly_camera_system
  [1] weather_system
  [2] byroredux_scripting::timer_tick_system
  [3] animation_system
  [4] spin_system
  [5] make_transform_propagation_system
  [6] billboard_system
  [7] make_world_bound_propagation_system
  [8] physics_sync_system
  [9] log_stats_system
  [10] event_cleanup_system
  [11] debug_drain_system

byro> screenshot /tmp/debug_frame.png
Screenshot saved: /tmp/debug_frame.png

byro> .quit
```

### Client-side commands (no network round-trip)

| Command | Action |
|---------|--------|
| `.help` | Print help text |
| `.quit` / `.exit` / `.q` | Exit the CLI |

## References

- [ECS](ecs.md) — World, Component, Query, Resource APIs
- [Vulkan Renderer](renderer.md) — draw_frame(), swapchain, composite pass
- [Papyrus Parser](papyrus-parser.md) — expression parser reused as query language
- [Scripting Architecture](scripting.md) — ECS-native scripting that the debugger complements
