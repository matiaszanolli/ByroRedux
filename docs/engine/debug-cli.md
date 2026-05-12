# Debug CLI (`byro-dbg`)

An external debugger for live ECS inspection. Connects to the running engine
over TCP and lets you query, modify, and screenshot the scene using
Papyrus-style expression syntax — without restarting the engine.

**Crates:** `crates/debug-protocol/`, `crates/debug-server/`, `tools/byro-dbg/` | **Tests:** 4 wire round-trip + 3 `CommandRegistry` dispatch (#518)

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
4. The **evaluator** dispatches by request kind:
   - `Ping`, `Stats`, `ListComponents`, `ListSystems`, `FindEntity`,
     `ListEntities`, `GetComponent`, `SetField`, `Screenshot` — direct
     resource/query access.
   - `Eval` — first checks if the leading token matches a name in the
     engine's `CommandRegistry` resource (so dotted console commands
     like `tex.missing` / `mesh.cache` dispatch through
     `registry.execute(...)`, #518); otherwise falls through to the
     Papyrus expression parser, walks the AST against `&World` +
     `ComponentRegistry`, and returns JSON.
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
| `Eval { expr }` | Parse + evaluate a Papyrus expression OR dispatch a dotted `CommandRegistry` command (#518) |
| `ListEntities { component? }` | List entities (optionally filtered by component) |
| `GetComponent { entity, component }` | Get full component data as JSON |
| `SetField { entity, component, path, value }` | Modify a single field |
| `ListComponents` | List all registered inspectable types |
| `ListSystems` | List ECS systems in stage order |
| `Stats` | FPS, frame time, entity/mesh/texture/draw counts |
| `FindEntity { name }` | Find entity by `Name` component |
| `Screenshot { path? }` | Capture the composited frame as PNG |
| `WalkEntity { entity, max_depth }` | Depth-first hierarchy walk — returns each visited node's id, name, parent, children, and world translation. Used to inspect runtime trees (NPC spawn chains) without per-component serde derives. |
| `InspectSkinnedMesh { entity }` | Dump a `SkinnedMesh`'s skeleton root, per-bone resolved entity + `GlobalTransform`, bind-inverses, and computed palette. Pairs with `WalkEntity` for the M41 Phase 1b.x palette-formula investigation (#841). |
| `Inspect { entity? }` | Per-entity component dump — every registered component on the entity as `(name, JSON value)`. `entity: None` reads the world's `SelectedRef` (see "Picked-Ref Workflow" below). The inspection half of the Bethesda-console `prid` + introspection pattern. |
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

### Currently registered (24 components)

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
| `AnimatedDiffuseColor` | (tuple: Vec3) — `NiMaterialColorController` target 0 |
| `AnimatedAmbientColor` | (tuple: Vec3) — `NiMaterialColorController` target 1 |
| `AnimatedSpecularColor` | (tuple: Vec3) — `NiMaterialColorController` target 2 |
| `AnimatedEmissiveColor` | (tuple: Vec3) — `NiMaterialColorController` target 3 (neon signs, plasma glow, muzzle flashes) |
| `AnimatedShaderColor` | (tuple: Vec3) — `BSEffect/BSLightingShaderPropertyColorController` |
| `AnimationPlayer` | clip_handle, local_time, playing, speed, reverse_direction, root_entity, prev_time (#486 ping-pong snapshot) |
| `AnimationStack` | layers, root_entity |
| `Inventory` | items — M41 Phase 2 equip slice (#896 / be4663b), surfaces NPC outfit contents to byro-dbg |
| `EquipmentSlots` | occupants — biped-slot bitmask coverage, pairs with `Inventory` for the M41 smoke-test workflow |

Post-#517 the single `AnimatedColor` slot is split into one component
per target. An entity with both a diffuse and an emissive controller
now carries both components side-by-side instead of colliding
last-write-wins on a shared RGB field.

`AnimationPlayer` snapshots include `reverse_direction` and the
blend-in/out timers (#486) so reloading mid-pingpong restores the
fold direction instead of stepping backward across the boundary.

The M41 Phase 2 `Inventory` + `EquipmentSlots` registration is
load-bearing for the `m41-equip.sh` smoke test — `entities Inventory`
lights up every actor with a populated outfit and `inspect <id>`
shows the resolved biped slots.

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

### CommandRegistry dispatch (dotted names)

The engine's in-process `CommandRegistry` resource also dispatches
through the evaluator: when the first whitespace-delimited token of an
`Eval` request matches a registered command name, the request is
handed to `registry.execute(...)` and the output lines are returned
as a newline-joined `Value` string. Pre-#518 these commands were
unreachable from `byro-dbg` because `tex.missing` parsed as
`Ident("tex") . member("missing")` → `find_by_name("tex")` →
`no entity named 'tex'`.

Current registered commands (19) grouped by purpose:

```
# Engine state
help                        → list every registered command
stats                       → FPS / frame time / entity / mesh / texture counts
entities [<Component>]      → list entities (optionally filtered by component)
systems                     → registered ECS systems in execution order
sys.accesses                → declared-access conflict report (R7) — pre-flight
                             for M27 parallel scheduler

# Picked reference + per-entity inspection
prid <entity_id>            → pick a reference for follow-up commands
prid                        → print the currently-picked reference

# Camera control (fly-camera entity, ActiveCamera resource)
cam.where                   → print active camera position + yaw/pitch
cam.pos <x> <y> <z>         → teleport camera to absolute world position
cam.tp <entity_id>          → teleport camera to over-the-shoulder framing of
                              the entity (200 back + 50 up, look-at).
                              No-arg form uses the picked ref (prid)

# Asset / texture diagnostics
tex.missing                 → entities with fallback texture + expected paths
tex.loaded                  → unique loaded textures + fallback count
mesh.info <entity_id>       → MeshHandle / TextureHandle / Material paths
                             (shows material_path when texture_path is absent —
                              correct FO4 behaviour since the real material
                              lives in the external BGSM/BGEM file)
mesh.cache                  → NIF import cache stats (size, hit rate, misses)

# Renderer telemetry (R6 / M29.3 / R7 instrumentation)
ctx.scratch                 → per-Vec capacity/len/heap-bytes for every persistent
                             CPU-side scratch buffer (R6 — catches unbounded
                             growth across M40 cell streaming)
skin.coverage               → last-frame skinned-BLAS coverage — dispatches /
                             first-sight / refit counters + slot-pool gauges.
                             Green-bar: `coverage: full` (M29.3 closure)
mem.frag                    → GPU memory fragmentation report

# Skinning + lighting diagnostics
skin.list                   → list SkinnedMesh entities + slot status
skin.dump <entity_id>       → full SkinnedMesh dump — per-bone bind / world /
                             palette matrices, identity-dropout flagging (#841)
light.dump                  → list active LightSource entities + radius / color
```

The dispatcher (#518) only kicks in for the *first whitespace-delimited
token* — args after the name pass through to the command's
`execute(world, args)` so existing `mesh.info 42` / `prid 42` shapes
work without parser changes. The dotted names are deliberately not
valid Papyrus identifiers, so they can't collide with member-access
chains.

`help` also works in-engine through `/cmd help`; the two namespaces
are the same `CommandRegistry`. New commands added via
`CommandRegistry::register` on the engine side automatically become
reachable from `byro-dbg` with no protocol change.

## Picked-Ref Workflow (`prid` + `inspect`)

Bethesda-console heritage. The original console paradigm is "pick a
reference, then run commands against the picked ref" — `prid 0001A332`
selects a target, then `getpos x`, `getav health`, etc. all operate
on it implicitly. byro-dbg mirrors this with a `SelectedRef` world
resource so commands across the console and the wire protocol read
the same selection state.

### `prid` — pick a reference (console command)

```
byro> prid 42
selected: entity 42 (DocMitchell)
byro> prid                    # no arg = print current
selected: entity 42 (DocMitchell)
```

Implementation in `byroredux/src/commands.rs` (`PridCommand`):

- Writes the `byroredux_core::ecs::SelectedRef` resource (world-scoped,
  not per-TCP-client — single-developer-at-a-time is the dev-tool
  reality).
- Validates the target has a `Transform` *or* `GlobalTransform` before
  setting. Bone-only entities with only a hierarchy parent pass the
  `GlobalTransform` check; orphans without either are rejected with
  a helpful error.
- Resolves the entity's `Name` through `StringPool` for the output
  line — same path as `entities` uses.
- Not implicitly cleared on cell unload. Bethesda's original `prid`
  has the same sharp edge; M40 streaming will eventually wire an
  explicit clear-on-unload pass.

### `inspect [<entity_id>]` — dump every registered component (protocol command)

```
byro> prid 42
selected: entity 42 (DocMitchell)
byro> inspect
Entity 42 "DocMitchell":
  Transform:
    {
      "translation": [3128.5, -148.0, 280.0],
      "rotation": [0.0, 0.0, 0.0, 1.0],
      "scale": 1.0
    }
  GlobalTransform:
    {
      "translation": [3128.5, -148.0, 280.0],
      "rotation": [0.0, 0.0, 0.0, 1.0],
      "scale": 1.0
    }
  Inventory:
    { "items": [...] }
  EquipmentSlots:
    { "occupants": [...] }
  AnimationPlayer:
    { "clip_handle": ..., "playing": true, ... }
(5 components)
```

Implementation in `crates/debug-server/src/evaluator.rs::eval_inspect`:

- Reads either the explicit `entity` arg or the `SelectedRef` resource
  when the arg is `None`. Empty `SelectedRef` + no arg returns a
  friendly error pointing at the `prid <id>` workflow.
- Iterates `ComponentRegistry::iter()` and calls each descriptor's
  `get_json` closure (the same closure that powers `42.Transform`
  expression access). Components the entity doesn't carry return
  `None` and are skipped.
- Output is registry order (BTreeMap-sorted by name) for stable diffs
  across sessions.

`inspect` is intentionally a **wire-protocol command**, not a
`CommandRegistry` console command — the `ComponentRegistry` lives on
the debug-server side (`DebugDrainSystem.registry`), not in the
World, and reusing it via a `DebugRequest::Inspect` variant avoids
moving 24 closures across crate boundaries. The console-side `prid`
mutates `SelectedRef` (a world resource) so the protocol-side
`inspect` reads the same picked state.

### Composing with other commands

Commands that previously required an explicit `<entity_id>` argument
should fall back to `SelectedRef` when called with no arg. Today:

- **`cam.tp`** (no arg) — frames the picked ref. Empty `SelectedRef`
  + no arg prints a usage hint pointing at the `prid` workflow.

Future Bethesda-console additions (`getpos`, `getav`, `setav`,
in-game console post-M47.0) layer onto the same `SelectedRef` +
`ComponentRegistry` foundation — adding them is a matter of writing
a new `ConsoleCommand` whose `execute` reads `SelectedRef`.

## Camera Control (`cam.*`)

The `cam.*` console commands move the active fly-camera entity from
byro-dbg. Use them to frame a workload (an NPC, a corner of a cell)
before reading per-frame telemetry like `skin.coverage` against a
known viewpoint.

| Command | What it does |
|---------|-------------|
| `cam.where` | Print `ActiveCamera` entity ID, world position, yaw/pitch in radians + degrees |
| `cam.pos <x> <y> <z>` | Teleport to an absolute world position (renderer Y-up). Leaves rotation untouched |
| `cam.tp <entity_id>` | Teleport over-the-shoulder of an entity (200 units back + 50 up, look-at). No-arg form uses the picked ref (`prid`) |

### Look-at math

`cam.tp` computes a fly-camera-compatible `(yaw, pitch)` pair from
the camera-to-target direction. The fly camera composes rotation as
`Q_y(yaw) * Q_x(pitch)` and treats `-Z` as forward; the look-at
inverse is:

```rust
let dir = (target - camera).normalize();
let pitch = dir.y.asin();
let yaw = (-dir.x).atan2(-dir.z);
```

Four unit tests in `byroredux/src/commands.rs::tests` verify the
round-trip on all six cardinal axes through the actual glam quat
composition — analytic sign-convention errors are caught at compile
time of the test.

### Survives `fly_camera_system` overwrite

`fly_camera_system` early-returns when `InputState.mouse_captured`
is false — the default state under `--bench-hold` headless smoke
runs. So `cam.pos` / `cam.tp` values persist across frames without
fighting the input loop.

Under active mouse capture (interactive play), the fly camera reads
yaw/pitch from `InputState` each frame and overwrites the
`Transform.rotation`. `cam.tp` defensively updates `InputState.yaw`
and `.pitch` alongside the rotation so the orientation survives the
next tick — `cam.pos` does not (rotation untouched, so it doesn't
need to).

## Renderer Telemetry

Three observability resources are refreshed each frame by the engine
binary after `Scheduler::run` and surfaced via console commands.

### `ctx.scratch` — scratch-buffer growth (R6)

`ScratchTelemetry` snapshots every persistent `Vec` scratch in the
renderer (gpu_instances, batches, indirect_draws, terrain_tile,
tlas_instances) plus the R1 / #780 material-table dedup ratio. Read
to catch unbounded growth across long sessions or M40 cell streaming
where a `Vec::reserve` driven by an outlier frame would pin capacity
at the high-water mark indefinitely with zero observability.

```
byro> ctx.scratch
VulkanContext scratch buffers (R6):
  name                           len   capacity   bytes_used       wasted
  gpu_instances_scratch         2562       3072    344064 B     57344 B
  batches_scratch                 87         96      4644 B       480 B
  indirect_draws_scratch        2562       3072     61440 B     12240 B
  terrain_tile_scratch             0          0         0 B         0 B
  tlas_instances_scratch        2562       3072    267264 B     44544 B
  total: 677412 bytes used, 114608 bytes wasted across 5 scratches
  materials: 142 unique / 2562 interned (18.0× dedup)
```

### `skin.coverage` — skinned BLAS refit coverage (M29.3 closure)

`SkinCoverageStats` records per-frame dispatches / first-sight /
refit counters + slot-pool gauges. The green-bar is `coverage:
full` (`refits_succeeded == dispatches_total && slots_failed == 0`).
PARTIAL output names the miss count and lists sampled failed entity
IDs for follow-up `inspect <id>`.

```
byro> skin.coverage
Skinned BLAS coverage (last frame):
  dispatches_total       = 6   (visible skinned entities)
  slots_active           = 6 / 64  (pool 9% full)
  slots_failed           = 0   (suppressed until LRU eviction)
  first_sight_attempted  = 0
  first_sight_succeeded  = 0
  refits_attempted       = 6
  refits_succeeded       = 6
  coverage: full
```

A regression — for example, a slot-pool exhaustion that drops two
NPCs from refit — surfaces as:

```
  coverage: PARTIAL — 2 of 6 visible skinned entities missed this frame
  failed_entity_ids (sample): [128, 142]
```

See `crates/core/src/ecs/resources.rs::SkinCoverageStats` for the
canonical schema and `crates/renderer/src/vulkan/context/draw.rs`
for the per-frame increments.

### `sys.accesses` — scheduler access conflicts (R7)

`SchedulerAccessReport` runs once at startup against the registered
systems and reports declared-access conflicts (Conflict / Unknown).
Pre-flight for M27 parallel dispatch — flip the
`parallel-scheduler` feature on only once `Unknown` rows hit zero.

```
byro> sys.accesses
Stage Late:
  fly_camera_system            declared: read InputState, write Transform
  spin_system                  declared: write Transform
  Conflict: fly_camera_system ↔ spin_system on Transform (both write)
  ...
```

## Canonical Workflows

End-to-end recipes that compose the commands above. Each maps to a
real engineering bar from the current roadmap.

### "Why isn't NPC X getting RT shadows?" (M41 + M29.3)

```
byro> entities Inventory                  # list every actor with equip state
byro> prid 142                            # pick the NPC by id
byro> inspect                             # confirm Inventory / EquipmentSlots
byro> cam.tp                              # frame them
byro> skin.coverage                       # verify dispatches_total includes them
byro> skin.dump 142                       # if PARTIAL, dump per-bone palette
```

Closure bar: `coverage: full` AND `inspect` shows `SkinnedMesh`,
`Inventory`, `EquipmentSlots` on the entity, AND `skin.dump` shows
zero identity-dropout palette slots.

### "Did a recent commit regress scratch growth across M40 streams?"

```
byro> ctx.scratch                         # baseline snapshot
... wait through a cell transition ...
byro> ctx.scratch                         # compare wasted_bytes
```

Regression bar: any row's `wasted_bytes` should return to a low
multiple of `bytes_used` after the high-water settles. Sustained
multi-MB `wasted` across cell transitions = a `Vec::reserve` that
pins on an outlier frame.

### "What's the entity I clicked / I'm looking at?"

byro-dbg has no in-engine raycast yet, but the closest substitute
is `find` + `inspect`:

```
byro> find("DocMitchell")
  Entity 42 "DocMitchell"
byro> inspect 42
```

A future addition would be a `cam.aim` command that ray-casts from
the active camera forward and prints the hit entity — natural pair
for `prid`.

### "M41 Phase 2 smoke test"

See [`docs/smoke-tests/m41-equip.sh`](../smoke-tests/m41-equip.sh)
for the canonical scripted version. The interactive form:

```
$ cargo run --release -- --esm Skyrim.esm --cell WhiterunBanneredMare \
    --bsa Skyrim-Meshes0.bsa --textures-bsa Skyrim-Textures0.bsa \
    --bench-frames 300 --bench-hold

$ cargo run -p byro-dbg
byro> entities Inventory
  Entity 12 "saadia"
  Entity 19 "brenuin"
  Entity 23 "mikael"
  ...
byro> prid 12
byro> cam.tp
byro> inspect
byro> skin.coverage
```

## Mutation

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
    evaluator.rs            Papyrus AST → ECS query evaluation + CommandRegistry dispatch (#518)
    registration.rs         register_component::<T>() for 19 types

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

### Fragment-shader bypass / viz bits — `BYROREDUX_RENDER_DEBUG`

Parsed once at engine boot by
[`parse_render_debug_flags_env()`](../../crates/renderer/src/vulkan/context/mod.rs)
and piped into the fragment shader via `GpuCamera.jitter[2]`. Each bit
collapses to a free no-op when the env var is unset (zero-overhead in
release builds). Accept plain decimal (`8`) or hex (`0x8`).

| Bit    | Constant in `triangle.frag`     | Effect |
|--------|---------------------------------|--------|
| `0x1`  | `DBG_BYPASS_POM`                | Skip parallax-occlusion ray-march; `sampleUV = baseUV`. |
| `0x2`  | `DBG_BYPASS_DETAIL`             | Skip detail-map modulation. |
| `0x4`  | `DBG_VIZ_NORMALS`               | Output the post-perturb world-space normal as RGB and exit (also written to G-buffer). |
| `0x8`  | `DBG_VIZ_TANGENT`               | Color fragments by tangent presence: green = authored or synthesized tangent reaches `perturbNormal` Path 1, red = zero tangent → screen-space derivative Path 2 fallback. |
| `0x10` | `DBG_BYPASS_NORMAL_MAP`         | Skip `perturbNormal(...)` entirely; lighting uses the geometric vertex normal. Use to bisect whether an artifact comes from the TBN reconstruction or from downstream specular / ambient code. |

Combine bits with bitwise-OR — e.g. `BYROREDUX_RENDER_DEBUG=0x14` runs
the normals visualization *with* the normal-map perturbation skipped,
showing pure geometric N. The startup log line confirms the parsed
mask:

```
BYROREDUX_RENDER_DEBUG = 0x10 (POM bypass=false, detail bypass=false,
                               normals viz=false, tangent viz=false,
                               normal-map bypass=true)
```

#### Diagnostic recipe — "chrome posterized" / banded specular / noisy plaster

Standard order, in increasing cost:

1. **`tex.missing`** (via `byro-dbg`) — if the count is non-trivial
   (>5 unique paths or >20 entities), the artifact is almost certainly
   the magenta-checker placeholder × a (correctly loaded) bump map.
   Diagnose the asset path, not the lighting math. Closed the entire
   "chrome walls" arc in Session 27 (commit `b2354a4`); see
   [HISTORY.md](../../HISTORY.md) for the full path.
2. **`BYROREDUX_RENDER_DEBUG=0x10`** + relaunch. Same camera, same
   cell. If the bypass + baseline screenshots are pixel-identical,
   `perturbNormal` is innocent — investigate specular / ambient / fog.
3. **`BYROREDUX_RENDER_DEBUG=0x4`** — visualize the post-perturb N.
   Adjacent fragments rendering arbitrary directions (yellow next to
   cyan next to lavender) point at TBN discontinuity; smooth gradients
   point at correctly-perturbed normals.
4. **`BYROREDUX_RENDER_DEBUG=0x8`** — confirm tangent presence.
   Should be all-green on Bethesda content (authored
   `NiBinaryExtraData` blob on Skyrim+/FO4 + the `synthesize_tangents`
   nifly fallback on FO3/FNV/Oblivion both feed `vertexTangent.xyz`).
   Red fragments mean the import path didn't produce a tangent for
   that mesh — investigate the NIF parser for that specific block.

### Headless `--cmd` (no TCP, no window)

```bash
cargo run -- --cmd help                     # execute one command, exit
cargo run -- --cmd stats                    # (fresh empty World — see limitation)
```

The `--cmd` path boots an empty `World`, registers the
`CommandRegistry`, runs one command, and exits without creating a
window. Useful for `help` and other world-agnostic commands. **Does
NOT inspect a running engine** — every world-dependent command
(`tex.missing`, `mesh.cache`, `entities`, `mesh.info`) reports zero
because the World was never populated. For live-world inspection
use `byro-dbg` against a running engine instance.

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
  AnimatedAmbientColor
  AnimatedDiffuseColor
  AnimatedEmissiveColor
  AnimatedShaderColor
  AnimatedSpecularColor
  AnimatedVisibility
  AnimationPlayer
  AnimationStack
  BSBound
  BSXFlags
  Billboard
  Camera
  EquipmentSlots
  GlobalTransform
  Inventory
  LightSource
  LocalBound
  Material
  MeshHandle
  TextureHandle
  Transform
  WorldBound
(24 components)

byro> entities Inventory
  Entity 12 "saadia"
  Entity 19 "brenuin"
  Entity 23 "mikael"
(3 entities)

byro> prid 12
selected: entity 12 (saadia)

byro> cam.tp
Camera teleported to look at entity 12 at (3128.50, -148.00, 280.00)
  camera now at (3128.50, -98.00, 480.00) yaw 0.0° pitch -14.0°

byro> inspect
Entity 12 "saadia":
  Transform:
    {
      "translation": [3128.5, -148.0, 280.0],
      "rotation": [0.0, 0.0, 0.0, 1.0],
      "scale": 1.0
    }
  GlobalTransform:
    { ... }
  Inventory:
    { "items": [...] }
  EquipmentSlots:
    { "occupants": [...] }
(4 components)

byro> skin.coverage
Skinned BLAS coverage (last frame):
  dispatches_total       = 6   (visible skinned entities)
  slots_active           = 6 / 64  (pool 9% full)
  slots_failed           = 0   (suppressed until LRU eviction)
  first_sight_attempted  = 0
  first_sight_succeeded  = 0
  refits_attempted       = 6
  refits_succeeded       = 6
  coverage: full

byro> tex.missing
17 unique missing textures:
   128x  textures/clutter/bottles/nukacola01_d.dds
    94x  textures/armor/leatherarmor_d.dds
   ...

byro> mesh.cache
NIF import cache:
  entries:       342 (341 parsed, 1 failed)
  lifetime hits: 9143
  lifetime miss: 342
  hit rate:      96.4%

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

byro> quit
```

### Client-side commands (no network round-trip)

| Command | Action |
|---------|--------|
| `.help` | Print help text |
| `.quit` / `.exit` / `.q` | Exit the CLI |
| `quit` / `exit` / `q` | Exit the CLI (bare forms, post-#518) |

## References

- [ECS](ecs.md) — World, Component, Query, Resource APIs
- [Vulkan Renderer](renderer.md) — draw_frame(), swapchain, composite pass
- [Papyrus Parser](papyrus-parser.md) — expression parser reused as query language
- [Scripting Architecture](scripting.md) — ECS-native scripting that the debugger complements
