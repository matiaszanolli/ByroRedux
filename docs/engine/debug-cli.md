# Debug CLI (`byro-dbg`)

An external debugger for live ECS inspection. Connects to the running engine
over TCP and lets you query, modify, screenshot, queue asset loads, and watch
live metrics — without restarting the engine. Two front-ends share one wire
protocol: a line-oriented REPL (default) and a ratatui live dashboard
(`--tui`). Query expressions reuse the Papyrus expression syntax; dotted
console commands (`tex.missing`, `mesh.info`, …) dispatch through the engine's
`CommandRegistry`.

**Crates:** `crates/debug-protocol/`, `crates/debug-server/`, `tools/byro-dbg/`
**Tests:** 6 wire round-trip (`crates/debug-protocol/src/wire.rs`) + 6 evaluator
/ `CommandRegistry` dispatch (`crates/debug-server/src/evaluator.rs`, #518) + 5
listener tests + 12 console-command tests (`byroredux/src/commands_tests.rs`)

> Last reconciled 2026-05-28 (Session 42 closeout). The doc was substantially
> rewritten 2026-05-11 (`478b9c0`); since then the debug-UI plan (Phases 1–5)
> added the `Metrics` / `LoadNif` / `Load*Cell` / `ListGameProfiles` /
> `ListLoadedAssets` protocol surface, the `--tui` dashboard, the `near` /
> `pick` / `door.teleport` / `script.activate` console commands, an enriched
> `mesh.info` (PBR + parent-chain + FormID + markers), and a set of robustness
> fixes (#1006–#1011 owner-tagged screenshot bridge + command-queue cap +
> shutdown side-channel; #1173 listener sleep 50 ms → 5 ms).

## Why an External Debugger?

As scenes grow in complexity (exterior cells, 1500+ entities, RT lighting),
`println!` debugging breaks down. The launch-only `--cmd` console commands
are fire-and-forget. We need:

1. **Live inspection** — query any entity, component, or resource while the
   engine is running
2. **Mutation** — tweak transforms, materials, and light properties without
   recompiling
3. **Screenshots** — capture the composited frame from the CLI for automated
   visual regression or issue reporting
4. **Asset loads on demand** — queue a NIF / interior / exterior load against
   a *running* engine without a relaunch (debug-UI Phases 1–5)
5. **Live metrics** — CPU / RAM / VRAM gauges + per-pass GPU times in a TUI
6. **Separation** — a standalone process that doesn't pollute the engine's
   hot path, with near-zero cost when no debugger is connected

## Architecture

```
┌─────────────┐    TCP (JSON)    ┌──────────────────────────┐
│  byro-dbg   │◄───────────────►│  debug-server            │
│ REPL / TUI  │  localhost:9876  │  Late-stage exclusive sys │
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
2. Incoming requests are pushed into a shared `Arc<Mutex<Vec<PendingCommand>>>`
   queue (`CommandQueue` in `crates/debug-server/src/listener.rs`). The queue
   is capped at `MAX_QUEUED_COMMANDS = 64` in-flight commands (#1010); a client
   whose enqueue would exceed the cap gets an immediate "server busy" error
   instead of unbounded buffering.
3. The **drain system** (`DebugDrainSystem`, exclusive, `Stage::Late`,
   registered via `scheduler.add_exclusive(Stage::Late, …)` in
   `crates/debug-server/src/lib.rs`) pops the queue once per frame after all
   gameplay systems have run.
4. The **evaluator** dispatches by request kind:
   - `Ping`, `Stats`, `Metrics`, `ListComponents`, `ListSystems`,
     `FindEntity`, `ListEntities`, `GetComponent`, `SetField`, `WalkEntity`,
     `InspectSkinnedMesh`, `Inspect`, `Screenshot`, `ListGameProfiles`,
     `ListLoadedAssets` — direct resource/query access.
   - `LoadNif`, `LoadInteriorCell`, `LoadExteriorCell` — queued for the
     engine binary to drain on a frame where it holds both `&mut World` and
     `&mut VulkanContext` (mirrors the existing `PendingCellTransition`
     pattern); the request returns `Ok` immediately.
   - `Eval` — first checks if the leading whitespace-delimited token matches a
     name in the engine's `CommandRegistry` resource (so dotted console
     commands like `tex.missing` / `mesh.cache` dispatch through
     `reg.execute(world, expr)`, #518); otherwise falls through to the Papyrus
     expression parser, walks the AST against `&World` + `ComponentRegistry`,
     and returns JSON. See `crates/debug-server/src/evaluator.rs`.
5. The response travels back through an `mpsc` channel → writer thread → TCP.

### Why not tokio?

A human-speed REPL issuing a few requests per second doesn't need async I/O.
`std::net` threads + a mutex queue is simpler, adds no dependency, and keeps
the render thread completely free of async runtime overhead. The TUI polls
`Metrics` at a fixed cadence (the engine refreshes the underlying snapshot at
~2 Hz) and is still comfortably inside the same model.

## Wire Protocol

Length-prefixed JSON over TCP: `[4-byte big-endian length][UTF-8 JSON payload]`.

- Max message: 16 MB (sanity check, not a practical limit)
- Human-debuggable with `netcat` or `socat`
- Serialization: `serde_json` (workspace dependency)
- Encode/decode in `crates/debug-protocol/src/wire.rs` (6 round-trip tests)

Request/response types are defined in `crates/debug-protocol/src/lib.rs`. The
enum is tagged with `#[serde(tag = "cmd", rename_all = "snake_case")]`.

| Request | What it does |
|---------|-------------|
| `Eval { expr }` | Parse + evaluate a Papyrus expression OR dispatch a dotted `CommandRegistry` command (#518) |
| `ListEntities { component? }` | List entities (optionally filtered by component) |
| `GetComponent { entity, component }` | Get full component data as JSON |
| `SetField { entity, component, path, value }` | Modify a single field |
| `ListComponents` | List all registered inspectable types |
| `ListSystems` | List ECS systems in stage order |
| `Stats` | FPS, frame time, entity/mesh/texture counts + draw-pipeline counts |
| `FindEntity { name }` | Find entity by `Name` component |
| `Screenshot { path? }` | Capture the composited frame as PNG (saved server-side if `path` is set, else returns base64 PNG) |
| `WalkEntity { entity, max_depth }` | Depth-first hierarchy walk — each visited node's id, depth, parent, children, world + local translation/rotation, and `has_skinned_mesh` / `has_mesh_handle` markers. Inspects runtime trees (NPC spawn chains) without per-component serde derives. |
| `InspectSkinnedMesh { entity }` | Dump a `SkinnedMesh`'s skeleton root, per-bone resolved entity + `GlobalTransform`, bind-inverses, the per-skin global transform, and the computed palette. Pairs with `WalkEntity` for the M41 Phase 1b.x palette-formula investigation (#841). |
| `Inspect { entity? }` | Per-entity component dump — every registered component on the entity as `(name, JSON value)`. `entity: None` reads the world's `SelectedRef` (see "Picked-Ref Workflow"). The inspection half of the Bethesda-console `prid` + introspection pattern. |
| `Metrics` | Live runtime metrics: CPU %, RAM used/total, process RSS, VRAM used/reserved/budget, and per-pass GPU times. Reads the engine's ~2 Hz snapshot without forcing a refresh. Drives the TUI dashboard. |
| `LoadNif { path, label? }` | Queue a loose-or-archive NIF load against the running engine. Returns `Ok` immediately; the engine drains the queue between frames. |
| `LoadInteriorCell { esm, cell, masters, bsas, textures_bsas }` | Queue an interior cell load by editor ID (same async-via-queue semantics). |
| `LoadExteriorCell { esm, grid_x, grid_y, radius, worldspace?, masters, bsas, textures_bsas }` | Queue an exterior grid load (radius clamped `1..=7` engine-side). |
| `ListGameProfiles` | Enumerate configured game profiles from `assets/debug_profiles.toml` + `~/.byroredux/profiles.toml` (debug-UI Phase 5). |
| `ListLoadedAssets { kind }` | Enumerate loaded asset handles. `kind` ∈ `Meshes` / `Textures` / `NifCache` (`AssetKind`). |
| `Ping` | Keep-alive / connection check |

Notable response variants (`#[serde(tag = "kind", rename_all = "snake_case")]`):
`Value`, `EntityList`, `ComponentList`, `SystemList`, `Stats`, `Screenshot`
(base64 PNG), `ScreenshotSaved`, `Ok`, `Pong`, `Hierarchy`, `SkinnedMesh`,
`Inspect`, `Metrics`, `GameProfiles`, `AssetList`, `Error`.

### `Stats` field breakdown (#1258 / PERF-D3-NEW-03 + #637 / FNV-D5-02)

The `Stats` response now distinguishes registry-wide from scene-scoped counts
and separates the three stages of the draw pipeline:

- `mesh_count` / `texture_count` — registry-wide (never drops on cell unload).
- `meshes_in_use` / `textures_in_use` — distinct non-zero handles held by live
  ECS entities. Scene-scoped; a leak that holds the last reference past unload
  shows up as `<registry>` larger than `<in_use>`.
- `draw_command_count` — pre-batch `DrawCommand` count input to the batcher
  (renamed from the misleading `draw_call_count` in #1258).
- `batch_count` — post-merge `DrawBatch` count from the main raster pass
  (upper bound on GPU draw calls).
- `indirect_call_count` — the actual `cmd_draw_indexed` +
  `cmd_draw_indexed_indirect` invocations — the real "draws" cost.

## Component Registry

The ECS is compile-time typed — there's no runtime reflection. The
`ComponentRegistry` bridges this gap with type-erased closures. The concrete
struct lives in `crates/debug-protocol/src/registry.rs`:

```rust
pub struct ComponentDescriptor {
    pub name: &'static str,
    pub field_names: Vec<&'static str>,
    pub get_json:      Box<dyn Fn(&dyn Any, u32) -> Option<Value> + Send + Sync>,
    pub set_json:      Box<dyn Fn(&dyn Any, u32, Value) -> Result<(), String> + Send + Sync>,
    pub list_entities: Box<dyn Fn(&dyn Any) -> Vec<u32> + Send + Sync>,
    pub get_field:     Box<dyn Fn(&dyn Any, u32, &str) -> Option<Value> + Send + Sync>,
    pub set_field:     Box<dyn Fn(&dyn Any, u32, &str, Value) -> Result<(), String> + Send + Sync>,
}
```

The closures take `&dyn std::any::Any` (downcast to `&World` inside) rather
than `&World` directly, so the protocol crate doesn't depend on core. The
registry is a `BTreeMap<String, ComponentDescriptor>` keyed by name (lookups
are exact-match first, then case-insensitive), which gives stable name-ordered
iteration for `ListComponents` and `Inspect`.

Each component is registered in `crates/debug-server/src/registration.rs`
using the generic `register_component::<T>()` helper, called from
`register_all(&mut ComponentRegistry)`. Components must derive
`Serialize + DeserializeOwned` (gated behind the `inspect` feature on
`byroredux-core`). The registry is owned by the `DebugDrainSystem`
(`DebugDrainSystem.registry`), not stored as a World resource.

### Currently registered (23 components)

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
| `MeshHandle` | (tuple: `"0"`) |
| `TextureHandle` | (tuple: `"0"`) |
| `BSXFlags` | (tuple: `"0"`) |
| `BSBound` | center, half_extents |
| `AnimatedVisibility` | (tuple: `"0"`) |
| `AnimatedAlpha` | (tuple: `"0"`) |
| `AnimatedDiffuseColor` | (tuple) — `NiMaterialColorController` target 0 |
| `AnimatedAmbientColor` | (tuple) — `NiMaterialColorController` target 1 |
| `AnimatedSpecularColor` | (tuple) — `NiMaterialColorController` target 2 |
| `AnimatedEmissiveColor` | (tuple) — `NiMaterialColorController` target 3 (neon signs, plasma glow, muzzle flashes) |
| `AnimatedShaderColor` | (tuple) — `BSEffect/BSLightingShaderPropertyColorController` |
| `AnimationPlayer` | clip_handle, local_time, playing, speed, reverse_direction, root_entity, prev_time (#486 ping-pong snapshot) |
| `AnimationStack` | layers, root_entity |
| `Inventory` | items — M41 Phase 2 equip slice (#896 / be4663b), surfaces NPC outfit contents to byro-dbg |
| `EquipmentSlots` | occupants — biped-slot bitmask coverage, pairs with `Inventory` for the M41 smoke-test workflow |

Post-#517 the single `AnimatedColor` slot is split into one component per
target. An entity with both a diffuse and an emissive controller now carries
both components side-by-side instead of colliding last-write-wins on a shared
RGB field.

`AnimationPlayer` snapshots include `reverse_direction` and the blend-in/out
timers (#486) so reloading mid-pingpong restores the fold direction instead of
stepping backward across the boundary.

The M41 Phase 2 `Inventory` + `EquipmentSlots` registration is load-bearing
for the `m41-equip.sh` smoke test — `entities Inventory` lights up every actor
with a populated outfit and `inspect <id>` shows the resolved biped slots.

To add a new component: derive `Serialize`/`Deserialize` (behind
`#[cfg_attr(feature = "inspect", ...)]`), then add a `register_component::<T>()`
call in `register_all` (`registration.rs`).

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

The engine's in-process `CommandRegistry` resource (defined in
`crates/core/src/console.rs`) also dispatches through the evaluator: when the
first whitespace-delimited token of an `Eval` request matches a registered
command name, the request is handed to `reg.execute(world, expr)` and the
output lines are returned as a newline-joined `Value` string. Pre-#518 these
commands were unreachable from `byro-dbg` because `tex.missing` parsed as
`Ident("tex") . member("missing")` → `find_by_name("tex")` →
`no entity named 'tex'`.

The console commands are registered in `byroredux/src/commands.rs`
(`build_command_registry()`). Current registered commands (**23**) grouped by
purpose:

```
# Engine state
help                        → list every registered command
stats                       → FPS / frame time / entity / mesh / texture counts
entities [<Component>]      → list entities (optionally filtered by component)
systems                     → registered ECS systems in execution order
sys.accesses                → declared-access conflict report (R7) — pre-flight
                             for M27 parallel scheduler (now also covers
                             exclusive systems, #1236 / #1237)

# Picked reference + spatial selection
prid <entity_id>            → pick a reference for follow-up commands
prid                        → print the currently-picked reference
near [radius=300]           → list entities within `radius` of the camera,
                             sorted by distance (id / name / tex-or-mat path
                             / position; nearest 30)
pick [count=10]             → ray-cast from camera-forward, list entities whose
                             WorldBound (or synthetic 32-unit) sphere the ray
                             pierces, closest-first. Synthetic hits flagged `~`

# Camera control (fly-camera entity, ActiveCamera resource)
cam.where                   → print active camera position + yaw/pitch
cam.pos <x> <y> <z>         → teleport camera to absolute world position
cam.tp <entity_id>          → teleport camera to over-the-shoulder framing of
                              the entity (200 back + 50 up, look-at).
                              No-arg form uses the picked ref (prid)

# Asset / texture diagnostics
tex.missing                 → entities with fallback texture + expected paths
tex.loaded                  → unique loaded textures + fallback count
mesh.info <entity_id>       → mesh / texture / material / transform / parent
                             chain / FormID / markers / aux-component dump
                             (see below)
mesh.cache                  → NIF import cache stats. `mesh.cache failed`
                             enumerates every failed-parse path

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

# World streaming + scripting
door.teleport <entity_id>   → inspect a door's XTEL destination (FormID,
                             Z-up position/rotation, resolved parent cell).
                             M40 Phase 2 door-teleport triage
script.activate <entity_id> → emit an `ActivateEvent` on the entity (M47.0
                             console-activate path; activator = PlayerEntity
                             resource or 0)
```

The dispatcher (#518) only kicks in for the *first whitespace-delimited
token* — args after the name pass through to the command's
`execute(world, args)` so existing `mesh.info 42` / `prid 42` shapes work
without parser changes. The dotted names are deliberately not valid Papyrus
identifiers, so they can't collide with member-access chains.

`help` also works in-engine through the console; the two namespaces are the
same `CommandRegistry`. New commands added via `CommandRegistry::register` on
the engine side automatically become reachable from `byro-dbg` with no
protocol change.

### `mesh.info` — the "what is this entity?" dump

`mesh.info <id>` is the workhorse for diagnosing a single entity. It now
prints (in `byroredux/src/commands.rs::MeshInfoCommand`):

- **Transform.local** — translation, ZYX-extracted Euler in degrees (for human
  reading; the quat is canonical, ZYX matches the FNVEdit/CK REFR DATA
  convention), and uniform scale.
- **Transform.global** — world translation.
- **Parent chain** — walks up `Parent` links (capped at 32 to guard cycles)
  and reports the `id -> parent -> …` chain. The FormID is attached only at the
  REFR placement root (#1212), so a mesh sub-entity finds it by walking up.
- **FormID** — runtime `FormId` resolved to the plugin-local `LocalFormId`
  (the 24-bit xEdit handle) via the `FormIdPool` resource.
- **Mesh / Texture / Material paths** — including `material_path` when
  `texture_path` is absent (correct FO4 behaviour — the real material lives in
  the external BGSM/BGEM file).
- **material_kind** — shader-type enum (0 = Default lit … 20 vanilla; 100+
  synthesized for GLASS / FX).
- **pbr metal/rough/gloss** — the canonical `metalness` / `roughness` scalars
  (resolved once at the `translate_material` boundary — BGSM-authored or
  keyword-classified, with no render-time fallback after the NIFAL canonical
  material pass, 3ce98db8) alongside the legacy `glossiness` input. This is the
  diagnostic the canonical-material sweep reads to compare each game's material
  convention in-engine (FO4 BGSM gives real per-material PBR; the FNV keyword
  classifier collapses surfaces — see `docs/engine/material-abstraction.md`
  §3a, 9d7e9eea).
- **alpha / emissive / effect_flags / env / vcm** — the remaining material
  fields that hint at the importer path even when no texture resolved.
- **Markers** — `AlphaBlend(src,dst)`, `TwoSided`, `IsFxMesh`, `RenderLayer`,
  `SceneFlags`, `DoorTeleport(→FormID)`.
- **Aux components** — `CollisionShape`, `RigidBodyData`, `LightSource`,
  `ParticleEmitter`, `SkinnedMesh`. Surfaces load-bearing non-mesh entities so
  they stop looking like orphan ghosts.

## Picked-Ref Workflow (`prid` + `inspect`)

Bethesda-console heritage. The original console paradigm is "pick a reference,
then run commands against the picked ref" — `prid 0001A332` selects a target,
then `getpos x`, `getav health`, etc. all operate on it implicitly. byro-dbg
mirrors this with a `SelectedRef` world resource so commands across the console
and the wire protocol read the same selection state.

### `prid` — pick a reference (console command)

```
byro> prid 42
selected: entity 42 (DocMitchell)
byro> prid                    # no arg = print current
selected: entity 42 (DocMitchell)
```

Implementation in `byroredux/src/commands.rs` (`PridCommand`):

- Writes the `byroredux_core::ecs::SelectedRef` resource (world-scoped, not
  per-TCP-client — single-developer-at-a-time is the dev-tool reality).
- Validates the target has a `Transform` *or* `GlobalTransform` before setting.
  Bone-only entities with only a hierarchy parent pass the `GlobalTransform`
  check; orphans without either are rejected with a helpful error.
- Resolves the entity's `Name` through `StringPool` for the output line — same
  path as `entities` uses.
- Not implicitly cleared on cell unload. Bethesda's original `prid` has the
  same sharp edge.

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
    { ... }
  Inventory:
    { "items": [...] }
  EquipmentSlots:
    { "occupants": [...] }
(4 components)
```

Implementation in `crates/debug-server/src/evaluator.rs` (the `Inspect`
request handler):

- Reads either the explicit `entity` arg or the `SelectedRef` resource when the
  arg is `None`. Empty `SelectedRef` + no arg returns a friendly error pointing
  at the `prid <id>` workflow.
- Iterates `ComponentRegistry::iter()` and calls each descriptor's `get_json`
  closure (the same closure that powers `42.Transform` expression access).
  Components the entity doesn't carry return `None` and are skipped.
- Output is registry order (BTreeMap-sorted by name) for stable diffs.

`inspect` is intentionally a **wire-protocol command** (`DebugRequest::Inspect`),
not a `CommandRegistry` console command — the `ComponentRegistry` lives on the
debug-server side (`DebugDrainSystem.registry`), not in the World, and reusing
it via the request variant avoids moving 23 closures across crate boundaries.
The console-side `prid` mutates `SelectedRef` (a world resource) so the
protocol-side `inspect` reads the same picked state.

### Composing with other commands

Commands that previously required an explicit `<entity_id>` argument fall back
to `SelectedRef` when called with no arg. Today:

- **`cam.tp`** (no arg) — frames the picked ref. Empty `SelectedRef` + no arg
  prints a usage hint pointing at the `prid` workflow.

Future Bethesda-console additions (`getpos`, `getav`, `setav`) layer onto the
same `SelectedRef` + `ComponentRegistry` foundation — adding them is a matter
of writing a new `ConsoleCommand` whose `execute` reads `SelectedRef`.

## Spatial Selection (`near` / `pick`)

Two ways to find "the thing I'm looking at" without an in-engine click:

- **`near [radius=300]`** — lists every entity within `radius` units of the
  active camera (`ActiveCamera` resource → its `Transform`), sorted by distance.
  Columns: distance, id, name, tex-or-material path, position. Shows the
  nearest 30. Good for "what's around me here?"
- **`pick [count=10]`** — ray-casts from the camera forward
  (`forward = R_y(yaw)·R_x(pitch)·-Z` derived from `InputState`) and returns
  entities whose `WorldBound` sphere the ray pierces, closest-first by
  ray-parameter. Entities with a degenerate `WorldBound::default()` (zero
  center/radius — the NIF importer didn't surface a usable local sphere) get a
  synthetic 32-unit sphere at their `GlobalTransform.translation`; those hits
  are flagged with `~` in the radius column so the operator knows they're
  approximate. Pair with `mesh.info <id>` on the top hit. Caveat: bounding
  spheres only — a big sphere's edge can register before a small geometry
  inside it, so the first 2–3 hits are usually what you want.

## Camera Control (`cam.*`)

The `cam.*` console commands move the active fly-camera entity from byro-dbg.
Use them to frame a workload (an NPC, a corner of a cell) before reading
per-frame telemetry like `skin.coverage` against a known viewpoint.

| Command | What it does |
|---------|-------------|
| `cam.where` | Print `ActiveCamera` entity ID, world position, yaw/pitch in radians + degrees |
| `cam.pos <x> <y> <z>` | Teleport to an absolute world position (renderer Y-up). Leaves rotation untouched |
| `cam.tp <entity_id>` | Teleport over-the-shoulder of an entity (200 units back + 50 up, look-at). No-arg form uses the picked ref (`prid`) |

### Look-at math

`cam.tp` computes a fly-camera-compatible `(yaw, pitch)` pair via the
`look_at_yaw_pitch(from, to)` helper in `byroredux/src/commands.rs`. The fly
camera composes rotation as `Q_y(yaw) * Q_x(pitch)` and treats `-Z` as forward;
the look-at inverse is:

```rust
let dir = (to - from).normalize();
let pitch = dir.y.asin();
let yaw = (-dir.x).atan2(-dir.z);
```

The round-trip is verified in `byroredux/src/commands_tests.rs` — `look_at`
cardinal-axis round-trips through the actual glam quat composition, an
offset-origin case, a degenerate zero-distance case, and the
`cam_tp_no_args_uses_selected_ref` / `..._no_selection_reports_usage` pair
(12 console-command tests total). Analytic sign-convention errors are caught
at test compile time.

### Survives `fly_camera_system` overwrite

`fly_camera_system` early-returns when `InputState.mouse_captured` is false —
the default state under `--bench-hold` headless smoke runs. So `cam.pos` /
`cam.tp` values persist across frames without fighting the input loop.

Under active mouse capture (interactive play), the fly camera reads yaw/pitch
from `InputState` each frame and overwrites `Transform.rotation`. `cam.tp`
defensively updates `InputState.yaw` and `.pitch` alongside the rotation so the
orientation survives the next tick — `cam.pos` does not (rotation untouched, so
it doesn't need to).

## Renderer Telemetry

Observability resources are refreshed each frame by the engine binary after
`Scheduler::run` and surfaced via console commands (and, for metrics, the
`Metrics` protocol request + TUI dashboard).

### `ctx.scratch` — scratch-buffer growth (R6)

`ScratchTelemetry` snapshots every persistent `Vec` scratch in the renderer
(gpu_instances, batches, indirect_draws, terrain_tile, tlas_instances) plus the
R1 / #780 material-table dedup ratio. Read to catch unbounded growth across
long sessions or M40 cell streaming where a `Vec::reserve` driven by an outlier
frame would pin capacity at the high-water mark indefinitely with zero
observability.

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

`SkinCoverageStats` records per-frame dispatches / first-sight / refit counters
+ slot-pool gauges. The green-bar is `coverage: full`
(`refits_succeeded == dispatches_total && slots_failed == 0`). PARTIAL output
names the miss count and lists sampled failed entity IDs for follow-up
`inspect <id>`.

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

A regression — for example, a slot-pool exhaustion that drops two NPCs from
refit — surfaces as:

```
  coverage: PARTIAL — 2 of 6 visible skinned entities missed this frame
  failed_entity_ids (sample): [128, 142]
```

See `crates/core/src/ecs/resources.rs` (`SkinCoverageStats`) for the canonical
schema and `crates/renderer/src/vulkan/context/draw.rs` for the per-frame
increments.

### `sys.accesses` — scheduler access conflicts (R7)

`SchedulerAccessReport` runs at startup against the registered systems and
reports declared-access conflicts (Conflict / Unknown). Pre-flight for M27
parallel dispatch — flip the parallel scheduler on only once `Unknown` rows hit
zero. As of #1236 / #1237 the report also covers exclusive systems (previously
only the parallel-eligible lane), so the conflict picture is complete.

```
byro> sys.accesses
Stage Late:
  fly_camera_system            declared: read InputState, write Transform
  spin_system                  declared: write Transform
  Conflict: fly_camera_system ↔ spin_system on Transform (both write)
  ...
```

### `Metrics` — live process / GPU metrics (debug-UI)

The `Metrics` request returns the engine's ~2 Hz sampled snapshot (refreshed by
`metrics_sample_system`): whole-process `cpu_pct`, system `ram_used_mb` /
`ram_total_mb`, engine `process_ram_mb` (RSS), `vram_used_mb` /
`vram_reserved_mb` / `vram_budget_mb` (gpu-allocator), and `gpu_pass_ms`
(per-pass GPU elapsed times — surfaces `SkinCoverageStats::gpu_*_ms` today:
`"skin"`, `"skin_blas_refit"`, `"taa"`). The wire type is hand-mirrored from
`byroredux_core::ecs::MetricsSnapshot` and kept in lockstep with the core type
by hand. This drives the TUI **Metrics** tab.

## On-Demand Asset Loads (debug-UI Phases 1–5)

The `LoadNif` / `LoadInteriorCell` / `LoadExteriorCell` requests queue a load
against the running engine and return `Ok` immediately. The engine drains the
load queue between frames where it holds both `&mut World` and
`&mut VulkanContext` (the same lane as `PendingCellTransition`). This lets you
swap content into a live session without a relaunch.

- `LoadNif { path, label? }` — `path` is either an absolute loose-file path or
  an archive-relative `meshes\foo.nif` resolved through the active BSA/BA2 set.
  `label` defaults to the basename and becomes the entity `Name` when the NIF
  carries none.
- `LoadInteriorCell` / `LoadExteriorCell` — carry the `esm`, `masters`,
  `bsas`, `textures_bsas` set (and grid + clamped `radius` + optional
  `worldspace` for exterior) so the load matches what the CLI flags would
  produce.

`ListGameProfiles` (Phase 5) enumerates configured games from
`assets/debug_profiles.toml` (engine defaults) merged with
`~/.byroredux/profiles.toml` (per-user overrides). A `GameProfile` carries a
stable `key`, display `name`, data `root`, main `esm`, default mesh/texture
archives, and curated `sample_cells` the TUI offers as one-click quick-loads.

`ListLoadedAssets { kind }` enumerates loaded handles — `Meshes` (MeshRegistry),
`Textures` (TextureRegistry), or `NifCache` (the `NifImportRegistry` parse
cache). Each `AssetItem` carries a `handle` plus optional `path` / `bytes` /
`summary`.

## TUI Dashboard (`--tui`)

`cargo run -p byro-dbg -- --tui` launches a ratatui live dashboard
(`tools/byro-dbg/src/tui.rs`, debug-UI Phase 3) instead of the line REPL. Four
tabs (cycle with Tab/Shift-Tab):

- **Metrics** — CPU / RAM / VRAM gauges + per-pass GPU times, polled via the
  `Metrics` request at a fixed cadence.
- **Entities** — list of every named entity (one-shot fetch on tab activation;
  `r` refetches).
- **Loader** — a form panel for queueing a `LoadNif` (path + label fields
  today; cell-load fields are the planned extension).
- **Console** — free-form text input → `DebugRequest::Eval` → response, so the
  full dotted-command + expression surface is reachable inside the TUI.

## Canonical Workflows

End-to-end recipes that compose the commands above.

### "Why isn't NPC X getting RT shadows?" (M41 + M29.3)

```
byro> entities Inventory                  # list every actor with equip state
byro> prid 142                            # pick the NPC by id
byro> inspect                             # confirm Inventory / EquipmentSlots
byro> cam.tp                              # frame them
byro> skin.coverage                       # verify dispatches_total includes them
byro> skin.dump 142                       # if PARTIAL, dump per-bone palette
```

Closure bar: `coverage: full` AND `inspect` shows `SkinnedMesh`, `Inventory`,
`EquipmentSlots` on the entity, AND `skin.dump` shows zero identity-dropout
palette slots.

### "Did a recent commit regress scratch growth across M40 streams?"

```
byro> ctx.scratch                         # baseline snapshot
... wait through a cell transition ...
byro> ctx.scratch                         # compare wasted_bytes
```

Regression bar: any row's `wasted` should return to a low multiple of
`bytes_used` after the high-water settles. Sustained multi-MB `wasted` across
cell transitions = a `Vec::reserve` that pins on an outlier frame.

### "What's the entity I'm looking at / standing near?"

```
byro> pick                                # ray-cast camera-forward, closest-first
byro> near 200                            # everything within 200 units, by dist
byro> mesh.info 142                       # full dump on the chosen hit
```

`pick` is the closer of the two for "the thing in my crosshair"; `near` is the
radial-sweep version. Both pair naturally with `prid <id>` + `inspect`.

### "Where does this door go?" (M40 Phase 2)

```
byro> pick                                # find the door REFR in the crosshair
byro> door.teleport 88                    # XTEL FormID + position + parent cell
```

### "M41 Phase 2 smoke test"

See [`docs/smoke-tests/m41-equip.sh`](../smoke-tests/m41-equip.sh) for the
canonical scripted version. The interactive form:

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

Field-level set uses the `SetField` protocol message. The evaluator reads the
component → serializes to JSON → modifies the field → deserializes back →
writes via `query_mut` (interior mutability on `&World`). Whole-component
replacement (`set_json`) is **not** supported — it would need `&mut World`,
which the exclusive `Stage::Late` system doesn't have; the closure returns a
"use field-level set" error.

## Screenshot Capture

The `screenshot` command captures the composited frame from the Vulkan
swapchain and encodes it as PNG. The capture spans frames:

```
Frame N:   drain system receives screenshot request
           → sets ScreenshotBridge.requested (AtomicBool), owner-tagged

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

### Robustness (#1006 / #1007 / #1011)

The screenshot path is shared between the in-engine `--cmd screenshot` flow
and the debug server, so the result slot is **owner-tagged**:

- `ScreenshotBridge::take_result_for(owner)` only hands back bytes the matching
  owner requested (#1006), so the debug server never steals a PNG intended for
  the engine's own capture.
- The per-client thread's 5 s `recv_timeout` can outrace the engine's 10-frame
  capture ceiling on a paused / GPU-stalled engine. The drain system honours an
  abandonment `cancel` flag (#1007): it cancels the in-flight GPU capture and
  clears its bookkeeping rather than leaking a straggler PNG into the result
  slot.
- A drain-timeout also cancels the bridge state (#1011).

### Implementation details

- Swapchain images have `TRANSFER_SRC` usage flag (added alongside
  `COLOR_ATTACHMENT` at creation).
- Staging buffer is `GpuToCpu` memory (host-visible, GPU-writable), allocated on
  first screenshot and reused.
- Copy happens inside the same command buffer as rendering — no extra GPU
  submission or sync.
- Swapchain format is `B8G8R8A8_SRGB` — pixels are converted to RGBA during
  readback.

### Relevant files

| File | Role |
|------|------|
| `crates/renderer/src/vulkan/context/screenshot.rs` | Copy commands, staging buffer, PNG encode |
| `crates/renderer/src/vulkan/context/mod.rs` | `ScreenshotHandle` (Arc-shared request/result) |
| `crates/core/src/ecs/resources.rs` | `ScreenshotBridge` (Resource, owner-tagged, bridges renderer↔server) |
| `crates/debug-server/src/system.rs` | Multi-frame screenshot flow in drain system |

## Feature Gating

Everything is behind feature flags to ensure near-zero cost in release builds:

| Feature | Crate | What it gates |
|---------|-------|--------------|
| `inspect` | `byroredux-core` | `serde::Serialize + Deserialize` on components, `serde`/`serde_json`/`glam/serde` deps |
| `debug-server` | `byroredux` (binary) | `byroredux-debug-server` dep, startup code in `main.rs` |

`debug-server` is **on by default** in the binary. Disable with
`--no-default-features` for release builds.

### Per-frame cost when enabled but idle

1. One `Mutex::lock()` + `Vec::is_empty()` check (the command queue)
2. One `AtomicBool::load()` (screenshot request flag)
3. Non-blocking `TcpListener::accept()` on the background thread, sleeping 5 ms
   between polls when there's no pending connection (down from 50 ms in #1173 —
   ~200 wakeups/s of an otherwise-idle thread, negligible CPU, and it cuts
   worst-case connect latency)

Total render-thread cost: sub-microsecond.

## File Layout

```
crates/debug-protocol/
  src/
    lib.rs                  DebugRequest / DebugResponse enums, AssetKind,
                            GameProfile, AssetItem, HierarchyNode, EntityInfo
    wire.rs                 Length-prefixed JSON encode/decode (6 tests)
    registry.rs             ComponentDescriptor, ComponentRegistry

crates/debug-server/
  src/
    lib.rs                  start() entry point; add_exclusive(Stage::Late, …)
    listener.rs             TcpListener, per-client threads, capped CommandQueue
    system.rs               DebugDrainSystem (Late-stage exclusive) + screenshot
    evaluator.rs            Papyrus AST → ECS query evaluation + CommandRegistry
                            dispatch (#518) + Inspect / Metrics / Load* handlers
    registration.rs         register_component::<T>() / register_all (23 types)

tools/byro-dbg/
  src/
    main.rs                 TCP client, REPL loop, shorthand parsing, --tui dispatch
    display.rs              Pretty-print responses
    tui.rs                  ratatui dashboard — Metrics / Entities / Loader / Console
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
cargo run -p byro-dbg                       # REPL, connect to localhost:9876
cargo run -p byro-dbg -- --tui              # ratatui live dashboard
BYRO_DEBUG_PORT=8080 cargo run -p byro-dbg  # custom port
BYRO_DEBUG_HOST=10.0.0.5 cargo run -p byro-dbg  # remote host
```

### Fragment-shader bypass / viz bits — `BYROREDUX_RENDER_DEBUG`

Parsed once at engine boot by
[`parse_render_debug_flags_env()`](../../crates/renderer/src/vulkan/context/mod.rs)
and piped into the fragment shader via `GpuCamera.jitter[2]`. Each bit collapses
to a free no-op when the env var is unset (zero-overhead in release builds).
Accepts plain decimal (`8`) or hex (`0x8`). Constants are defined in
`crates/renderer/shaders/include/shader_constants.glsl` (and used in
`triangle.frag`).

| Bit    | Constant (in `shader_constants.glsl`) | Effect |
|--------|---------------------------------|--------|
| `0x1`  | `DBG_BYPASS_POM`                | Skip parallax-occlusion ray-march; `sampleUV = baseUV`. |
| `0x2`  | `DBG_BYPASS_DETAIL`             | Skip detail-map modulation. |
| `0x4`  | `DBG_VIZ_NORMALS`               | Output the post-perturb world-space normal as RGB and exit (also written to G-buffer). |
| `0x8`  | `DBG_VIZ_TANGENT`               | Color fragments by tangent presence: green = authored or synthesized tangent reaches `perturbNormal` Path 1, red = zero tangent → screen-space derivative Path 2 fallback. |
| `0x10` | `DBG_BYPASS_NORMAL_MAP`         | Skip `perturbNormal(...)` entirely; lighting uses the geometric vertex normal. Use to bisect whether an artifact comes from the TBN reconstruction or from downstream specular / ambient code. |

Combine bits with bitwise-OR — e.g. `BYROREDUX_RENDER_DEBUG=0x14` runs the
normals visualization *with* the normal-map perturbation skipped, showing pure
geometric N. The startup log line confirms the parsed mask:

```
BYROREDUX_RENDER_DEBUG = 0x10 (POM bypass=false, detail bypass=false,
                               normals viz=false, tangent viz=false,
                               normal-map bypass=true)
```

#### Diagnostic recipe — "chrome posterized" / banded specular / noisy plaster

Standard order, in increasing cost:

1. **`tex.missing`** (via `byro-dbg`) — if the count is non-trivial (>5 unique
   paths or >20 entities), the artifact is almost certainly the magenta-checker
   placeholder × a (correctly loaded) bump map. Diagnose the asset path, not the
   lighting math. Closed the entire "chrome walls" arc in Session 27 (commit
   `b2354a4`); see [HISTORY.md](../../HISTORY.md) for the full path.
2. **`BYROREDUX_RENDER_DEBUG=0x10`** + relaunch. Same camera, same cell. If the
   bypass + baseline screenshots are pixel-identical, `perturbNormal` is
   innocent — investigate specular / ambient / fog.
3. **`BYROREDUX_RENDER_DEBUG=0x4`** — visualize the post-perturb N. Adjacent
   fragments rendering arbitrary directions (yellow next to cyan next to
   lavender) point at TBN discontinuity; smooth gradients point at
   correctly-perturbed normals.
4. **`BYROREDUX_RENDER_DEBUG=0x8`** — confirm tangent presence. Should be
   all-green on Bethesda content (authored `NiBinaryExtraData` blob on
   Skyrim+/FO4 + the `synthesize_tangents` fallback on FO3/FNV/Oblivion both
   feed `vertexTangent.xyz`). Red fragments mean the import path didn't produce
   a tangent for that mesh — investigate the NIF parser for that specific block.

### Headless `--cmd` (no TCP, no window)

```bash
cargo run -- --cmd help                     # execute one command, exit
cargo run -- --cmd stats                    # (fresh empty World — see limitation)
```

The `--cmd` path boots an empty `World`, registers the `CommandRegistry`, runs
one command, and exits without creating a window. Useful for `help` and other
world-agnostic commands. **Does NOT inspect a running engine** — every
world-dependent command (`tex.missing`, `mesh.cache`, `entities`, `mesh.info`)
reports zero because the World was never populated. For live-world inspection
use `byro-dbg` against a running engine instance.

The `--bench-hold` flag is the companion: a `--bench-frames N --bench-hold` run
executes the bench, prints the summary, and then **keeps the engine open** so
`byro-dbg` can attach (port 9876) and drive console commands against the loaded
scene. Without `--bench-hold` the bench exits immediately and the debug server
isn't reachable.

### Example session

```
$ cargo run -p byro-dbg
Connecting to ByroRedux at 127.0.0.1:9876...
Connected.

byro> stats
FPS:       60 (avg 60)
Frame:     16.61ms (min 15.90ms, max 19.20ms)
Entities:  1547
...

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
(23 components)

byro> entities Inventory
  Entity 12 "saadia"
  Entity 19 "brenuin"
  Entity 23 "mikael"
(3 entities)

byro> prid 12
selected: entity 12 (saadia)

byro> cam.tp
Camera teleported to look at entity 12 ...

byro> inspect
Entity 12 "saadia":
  Transform:        { ... }
  GlobalTransform:  { ... }
  Inventory:        { "items": [...] }
  EquipmentSlots:   { "occupants": [...] }
(4 components)

byro> near 200
camera at (...) — N entities within 200.0 units (showing nearest 30):
   dist      id  name                          tex/mat path  position
   ...

byro> pick
... ray-cast hits, closest-first ...

byro> mesh.info 12
Entity 12 (saadia):
  Transform.local:   pos (...)
  ...
  pbr metal/rough/gloss: 0.00 / 0.80 / 30

byro> skin.coverage
Skinned BLAS coverage (last frame):
  ...
  coverage: full

byro> tex.missing
... unique missing textures ...

byro> mesh.cache
NIF import cache:
  ...

byro> systems
  [n] fly_camera_system
  ... (illustrative — the live list depends on which gameplay/scripting
       systems main.rs registered; debug_drain_system is last) ...

byro> screenshot /tmp/debug_frame.png
Screenshot saved: /tmp/debug_frame.png

byro> quit
```

> The `systems` list above is illustrative. The engine registers a
> game-dependent set (camera/player controller, weather, animation, particles,
> footsteps, metrics sampling, transform/world-bound propagation, scripting
> dispatch systems, …) in `byroredux/src/main.rs`; the debug drain system is
> always the last (`Stage::Late` exclusive). Run `systems` against your live
> session for the authoritative order.

### Client-side commands (no network round-trip)

| Command | Action |
|---------|--------|
| `.help` | Print help text |
| `.quit` / `.exit` / `.q` | Exit the CLI |
| `quit` / `exit` / `q` | Exit the CLI (bare forms, post-#518) |

The REPL recognizes a few **shorthands** that map straight to protocol requests
(`tools/byro-dbg/src/main.rs::parse_shorthand`): `ping`, `stats`, `components`,
`systems`, `screenshot [path]`, `entities [Component]`, `skin <id>`
(→ `InspectSkinnedMesh`), and `inspect [<id>]`. Anything else is sent as an
`Eval` expression (which also reaches the dotted `CommandRegistry` commands).

## References

- [ECS](ecs.md) — World, Component, Query, Resource APIs
- [Vulkan Renderer](renderer.md) — draw_frame(), swapchain, composite pass
- [Papyrus Parser](papyrus-parser.md) — expression parser reused as query language
- [Scripting Architecture](scripting.md) — ECS-native scripting that the debugger complements
- [Material Abstraction](material-abstraction.md) — the canonical `translate_material` boundary `mesh.info`'s PBR line reads from
