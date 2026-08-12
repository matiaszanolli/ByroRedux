# UI System — Scaleform / SWF via Ruffle

Bethesda's Creation Engine uses Scaleform GFx (Adobe Flash / SWF) for
every menu — main menu, pause menu, HUD, container UI, dialogue boxes,
even the Pip-Boy interface in Fallout. Skyrim ships ~34 SWF menus;
Fallout 4 ships even more.

ByroRedux integrates [**Ruffle**](https://github.com/ruffle-rs/ruffle)
(the open-source Flash player written in Rust) as a library, so we can
load and run those SWF menus without writing a Flash interpreter from
scratch and without linking to Adobe's GFx runtime.

Source: [`crates/ui/src/`](../../crates/ui/src/)

> Status note (2026-07-26): R4 selected pinned Ruffle plus ByroRedux-owned
> Bethesda profiles. The first M48 slice now includes profile detection,
> a bidirectional ExternalInterface bridge, and a pinned 74-method
> Skyrim/SkyUI host catalog. The second slice adds Fallout 4's
> `BGSCodeObj` lifecycle, a 138-method installed-corpus catalog, and
> an injected AVM2 forwarding adapter. The third slice adds a BSA/BA2-backed
> Ruffle navigator and executor-driven `ImportAssets` preload; the installed
> Fallout 4 HUD now resolves `fonts_en.swf` and reaches frame 1. HUD and
> Pip-Boy readiness/destruction are asserted against the installed BA2, and
> Atomic Command contributes nine cataloged holotape methods. The fourth slice
> adds focused keyboard, pointer, wheel, text, and IME routing through Ruffle
> for both VMs, with modal capture ahead of world controls. Method behavior,
> remaining GFx compatibility, menu-stack policy, and full-menu visual fidelity
> remain integration work.

## At a glance

| | |
|---|---|
| Crate                  | `byroredux-ui` |
| External engine        | Ruffle (Flash / ActionScript 1, 2, 3), git-pinned (see `crates/ui/Cargo.toml`) |
| Ruffle render backend  | `ruffle_render_wgpu` on its **own** wgpu/Vulkan device (separate from the engine's `ash` Vulkan) |
| Render path            | Ruffle → wgpu offscreen `TextureTarget` → `capture_frame()` CPU RGBA → Vulkan texture upload → fullscreen quad |
| Lifetime               | `UiManager` is **not** an ECS resource — Ruffle's `Player` is not `Send + Sync`; it lives in the main loop alongside `VulkanContext` |
| Status                 | Loose SWF demo working (`--swf path.swf`); AVM1/Skyrim and AVM2/Fallout 4 profiles; bidirectional host bridge; Skyrim `GameDelegate` and Fallout 4 `BGSCodeObj` contracts; BSA/BA2-relative `ImportAssets` loading; focused winit input routing |
| Pending                | Host-method behavior, remaining GFx stubs, Papyrus↔UI bridge, menu-stack/focus policy, font fidelity, full menu pack |

## Why Ruffle?

The legacy Bethesda menus run on Scaleform GFx 4.x / 5.x, which is
proprietary middleware Adobe acquired and Autodesk later sunset. There is
no open-source Scaleform runtime. We have three options:

1. **Reverse-engineer Scaleform GFx** — months of work and a permanent
   drag-along of legacy ActionScript dialect quirks
2. **Reimplement every menu in a modern UI library** (egui, imgui, ...)
   — fastest start, but throws away every modder's existing SWF mods
3. **Use Ruffle** — open-source Flash player written in Rust, already
   handles AS1/2/3, with a ByroRedux-owned Bethesda Scaleform host layer

Option 3 is what M20 picked. The bet is that "Bethesda menu" ≈ "Flash file
that uses a small set of Scaleform extensions" and that those extensions
can be stubbed in a few hundred lines of glue. So far that bet is holding:
simple AS2 menus render, and the pinned Ruffle API provides both sides of
the host transport: `ExternalInterfaceProvider::call_method` for
ActionScript → engine and `Player::call_internal_interface` for engine →
registered ActionScript callbacks. ByroRedux now installs that transport
for both Skyrim AVM1 and Fallout 4 AVM2 movies. The remaining work is the
Bethesda behavior behind the catalog and Fallout 4's native-object
surface, not a new Flash VM.

## Module map

```
crates/ui/src/
├── avm2_host.rs Fallout 4 BGSCodeObj ABC adapter and SWF injection
├── catalog.rs   Profile-specific known host-method/object inventory
├── host.rs      ScaleformHostBridge — ExternalInterface call queue (bounded,
│                drained per frame by the main loop), callback discovery,
│                typed values, diagnostics/responses
├── input.rs     Platform-neutral keyboard, pointer, text, IME, and focus events
├── lib.rs       UiManager — top-level handle: owns the active SwfPlayer,
│                visibility/input-focus/viewport state, load/tick/render/close
├── navigator.rs Archive-backed relative URL resolution, resource diagnostics,
│                Ruffle future executor, and ImportAssets preload compatibility
├── player.rs    SwfPlayer — Ruffle wrapper, own wgpu/Vulkan device,
│                offscreen TextureTarget, capture_frame() → cached RGBA buffer
└── profile.rs   Skyrim AVM1 / Fallout 4 AVM2 host profiles and detection
```

The catalog, player, profile, host bridge, host call/dispatch status, and
typed value are re-exported from `byroredux_ui`; `UiManager` remains
defined directly in `lib.rs`.

## Pipeline

```
SWF file bytes
        │
        ▼  ruffle_core::tag_utils::SwfMovie::from_data
parsed SwfMovie
        │
        ├── ImportAssets URL → navigator → BSA/BA2 resource provider
        │                         → local future executor → continued preload
        │
        ▼  PlayerBuilder::new().with_renderer(WgpuRenderBackend).with_movie(..).build()
ruffle_core::Player (Arc<Mutex<…>>; advances frames, runs ActionScript)
        │
        ▼  Player::tick(FloatDuration) then Player::render()
offscreen wgpu TextureTarget (RGBA8) on Ruffle's own wgpu/Vulkan device
        │
        ▼  downcast renderer → WgpuRenderBackend::capture_frame() → CPU RGBA into SwfPlayer.pixel_buffer
RGBA pixel buffer (cached; only re-emitted when `dirty`)
        │
        ▼  byroredux::main → texture_registry.update_rgba(ui_texture_handle, …)
existing Vulkan VkImage replaced in place (deferred-destroy of the old one)
        │
        ▼  draw_frame: bind pipeline_ui (no depth, alpha blend, bindless sampler)
        ▼  draw the fullscreen UI quad, sampling textures[textureIndex]
Pixels on screen
```

The trick is the **CPU bridge** between Ruffle's wgpu backend and our
Vulkan renderer. Ruffle is built around wgpu and renders to a wgpu
texture on its **own** device (created with `create_wgpu_instance` /
`request_adapter_and_device` over `wgpu::Backends::VULKAN`); we don't
share GPU contexts between that device and the engine's `ash` Vulkan, so
we read pixels back to the CPU via `capture_frame()` and re-upload to an
engine-side Vulkan texture. This costs one round-trip per UI frame but
it's bounded by the SWF resolution (the loose-demo player is sized to the
swapchain extent) and works without coupling the two backends.

The whole UI plane is **one fullscreen quad** in the renderer with one
texture binding. Multiple menus stack inside Ruffle (main menu → submenu
→ messagebox) — that's the SWF runtime's job, not Vulkan's.

## SwfPlayer API

```rust
pub struct SwfPlayer {
    player: Arc<Mutex<ruffle_core::Player>>,
    width: u32,
    height: u32,
    pixel_buffer: Vec<u8>,   // last captured RGBA8, reused frame to frame
    dirty: bool,             // set on tick(), cleared after a successful render()
    host_object_state: ScaleformHostObjectState,
    navigator_runtime: Option<ScaleformNavigatorRuntime>,
}

impl SwfPlayer {
    pub fn new(swf_data: &[u8], width: u32, height: u32) -> anyhow::Result<Self>;
    pub fn from_resource_provider(
        provider: Rc<dyn ScaleformResourceProvider>,
        movie_path: &str,
        width: u32,
        height: u32,
        profile: ScaleformProfile,
    ) -> anyhow::Result<Self>;
    pub fn tick(&mut self, dt: f64);          // seconds; wrapped in FloatDuration internally
    pub fn render(&mut self) -> Option<&[u8]>; // borrows pixel_buffer; None if not dirty
    pub fn dimensions(&self) -> (u32, u32);
    pub fn host_object_state(&self) -> ScaleformHostObjectState;
    pub fn current_frame(&self) -> Option<u16>;
    pub fn resource_loads(&self) -> Vec<ScaleformResourceLoad>;
    pub fn resource_error(&self) -> Option<&str>;
}
```

`new()` parses the SWF (`SwfMovie::from_data`), spins up a headless
wgpu/Vulkan device, builds an offscreen `TextureTarget` of the requested
size, wires it into a `WgpuRenderBackend`, attaches a software video
backend (`ruffle_video_software`), selects blocking preload for the
already-resident SWF bytes, and starts playback (`set_is_playing(true)`).
For a Fallout 4 contract movie, the in-memory ABC adapter is inserted before
this parse/build sequence and the lifecycle class constructor is patched to
bootstrap immediately after it initializes `BGSCodeObj`; the source asset on
disk is never modified.

`from_resource_provider()` loads the root and relative dependencies through
the same source. `Ba2Archive` and `BsaArchive` implement
`ScaleformResourceProvider`; other overlay/mod stacks can implement the
one-method trait without coupling Ruffle to a particular archive format.
The virtual root URL preserves the menu's archive directory, so Fallout 4's
`fonts_en.swf` request from `interface\hudmenu.swf` resolves to
`interface\fonts_en.swf`. The player retains Ruffle's local executor and
alternates unlimited preload passes with `run_until_stalled()` outside the
player mutex until each queued import completes. Missing or failed resources
are surfaced through construction errors or `resource_error()` rather than
leaving the root silently parked before frame 1.

Pinned Ruffle initializes `ImportAssets` children at preload frame zero while
its AVM2 `DoABC`/`SymbolClass` preloader indexes `frame - 1`. Fallout 4's
`fonts_en.swf` exercises that underflow. For paths proven to be
`ImportAssets` targets, the navigator inserts a raw zero-length `ShowFrame`
boundary immediately before the first affected tag. This restores the frame
index Ruffle normally has for a root movie without reserializing the SWF's
other tags. Ordinary dynamic movie loads are not rewritten, and
`ScaleformResourceLoad::import_preload_rewritten` records when the workaround
was applied.

`tick(dt)` advances Ruffle's clock (`Player::tick(FloatDuration::from_secs(dt))`)
and runs any ActionScript that wants to fire (timers, frame scripts,
button handlers), pumps any newly queued archive future after releasing the
player lock, then marks the player **dirty**.

`render()` is a no-op fast path when not dirty. When dirty it calls
`Player::render()`, downcasts the boxed renderer back to the concrete
`WgpuRenderBackend<TextureTarget>`, calls `capture_frame()`, and copies
the resulting `RgbaImage` into the reused `pixel_buffer` (with a size-
mismatch guard that logs and skips). It returns a borrow of that buffer
and clears the dirty flag. The width/height are the renderer-side surface
dimensions, **not** the SWF's native size — Ruffle scales internally.

## UiManager

```rust
pub struct UiManager {
    player: Option<SwfPlayer>,  // None until a menu is loaded
    pub visible: bool,
    input_focused: bool,
    pub menu_name: String,      // e.g. the SWF path / "startmenu"
    pub width: u32,
    pub height: u32,
}

impl UiManager {
    pub fn new(width: u32, height: u32) -> Self;
    pub fn load_swf(&mut self, swf_data: &[u8], name: &str) -> anyhow::Result<()>;
    pub fn load_swf_from_resource_provider(
        &mut self,
        provider: Rc<dyn ScaleformResourceProvider>,
        movie_path: &str,
        name: &str,
        profile: ScaleformProfile,
    ) -> anyhow::Result<()>;
    pub fn tick(&mut self, dt: f64);             // forwards to the active player when visible
    pub fn render(&mut self) -> Option<&[u8]>;   // None when hidden or no player
    pub fn has_input_focus(&self) -> bool;
    pub fn set_input_focus(&mut self, focused: bool) -> bool;
    pub fn handle_input(&mut self, event: UiInputEvent) -> bool;
    pub fn set_mouse_in_stage(&mut self, is_in_stage: bool) -> bool;
    pub fn close(&mut self);                      // drops the player, clears state
}
```

`UiManager` is **deliberately not** an ECS `World` resource — Ruffle's
`Player` owns non-`Send`/`Sync` backends (video, audio), so the manager
is held directly on the main `App` struct (`ui_manager: Option<UiManager>`
in `byroredux/src/main.rs`) alongside the `VulkanContext`, and ticked /
rendered inline in the per-frame loop rather than through the scheduler.

In the current loose-SWF demo (`--swf path.swf`) there is exactly one
optional player sized to the swapchain extent. Future Bethesda menu
integration will need to manage multiple active menus (one per layer);
that compositing happens inside Ruffle, so the engine-side change is
about which SWF(s) `UiManager` drives, not about stacking Vulkan quads.

## Vulkan integration

The UI is drawn at the tail of the main render pass, not in a separate
pass or subpass. The renderer side has a dedicated UI pipeline
(`pipeline::create_ui_pipeline`, stored as `VulkanContext::pipeline_ui`)
with:

- **No depth test / no depth write / no stencil** — UI draws on top of
  the world (`depth_test_enable(false)`, `depth_write_enable(false)`,
  `stencil_test_enable(false)`; world-geometry stencil lives in the
  opaque/blend pipelines, #337).
- **Alpha blend** on the HDR color slot (`SRC_ALPHA`,
  `ONE_MINUS_SRC_ALPHA`; alpha channel `ONE`/`ZERO`).
- **G-buffer masked off.** The main render pass has 6 color attachments
  (HDR + normal + motion + mesh-id + …). The UI pipeline writes RGBA to
  slot 0 (HDR) only; the other five attachments use a no-op blend state
  with `color_write_mask(empty)` so the UI quad never pollutes the
  normal / motion-vector / mesh-id G-buffer.
- **Lightweight vertex format.** The UI quad uses `UiVertex` (position +
  UV only, **20 bytes** — `[f32; 3]` + `[f32; 2]`, 2 attribute
  descriptions) rather than the full 100-byte scene `Vertex`. The split
  landed alongside the M-NORMALS vertex work (#783); the 20-byte size and
  field offsets are pinned by tests in `crates/renderer/src/vertex.rs`.
- **Bindless texture sampling.** `ui.frag` samples
  `textures[nonuniformEXT(fragTexIndex)]` from the shared bindless array
  (`set = 0, binding = 0`) used by `triangle.frag` and the composite
  pass. The texture index is read **per-instance** from the instance SSBO
  in `ui.vert` (`fragTexIndex = instances[gl_InstanceIndex].textureIndex`),
  **not** via the MaterialBuffer — a contract codified after the #776 /
  #785 / #1065 regressions, with `ui.vert` carrying a struct-size-only
  mirror of `GpuInstance` (no `MaterialBuffer`, no `GpuMaterial`) for
  std430 lockstep. The reflection / layout tests in
  `crates/renderer/src/vulkan/scene_buffer/` enforce this.
- **Static-vs-dynamic state invariant.** Viewport and scissor are the UI
  pipeline's only dynamic states (`UI_PIPELINE_DYNAMIC_STATES`, len 2);
  depth/cull/depth-bias are static and applied by the pipeline bind
  itself. `draw.rs` re-sets viewport/scissor after binding `pipeline_ui`
  (defensive, #133) and a `const` assertion fires if anyone grows the
  dynamic-state list without extending the explicit `cmd_set_*` calls
  (#663).

### Texture upload — `register_rgba` / `update_rgba`

There is no bespoke `update_ui_texture` entry point; the UI texture is an
ordinary entry in the renderer's `TextureRegistry`:

- On `--swf` load (`byroredux/src/scene.rs`) a transparent-black RGBA
  buffer is registered with `texture_registry.register_rgba(...)`, yielding
  a `ui_texture_handle` stored on the `App`.
- Each frame, when `UiManager::render()` returns a fresh buffer, the main
  loop calls `texture_registry.update_rgba(handle, w, h, pixels)`.

`update_rgba` **replaces the texture in place** (rebuilding the `VkImage`
from the new RGBA) and uses **deferred destruction** (issue #134): the
replaced image is parked on a per-entry `pending_destroy` ring and only
freed once `MAX_FRAMES_IN_FLIGHT` frames have elapsed (drained via
`tick_deferred_destroy`). That is what makes per-frame UI texture updates
stall-free — without it, every UI frame would need a `device_wait_idle`
to know the previous frame finished sampling the old texture before
freeing it. The bindless descriptor slot reactivates on the descriptor
write that `update_rgba` queues.

The fullscreen quad mesh itself is registered once via
`VulkanContext::register_ui_quad()` (called from `scene.rs`), which uploads
`mesh::fullscreen_quad_ui_vertices()` (NDC corners, RT skipped) and stashes
the result as `ui_quad_handle`.

## SWF demo

```bash
cargo run -- --swf path/to/menu.swf
```

This:

1. Reads the SWF file (`std::fs::read`)
2. Constructs a `UiManager::new(w, h)` sized to the swapchain extent and
   `load_swf`s the bytes (which creates the `SwfPlayer`)
3. Registers a transparent-black UI texture (`register_rgba`) and stores
   its handle on the `App`
4. Per frame, inside the main draw loop:
   a. `ui.tick(dt)` (dt from the `DeltaTime` resource, falling back to
      1/60 s)
   b. `if let Some(pixels) = ui.render() { texture_registry.update_rgba(...) }`
   c. records the UI quad draw with the UI texture handle as the bound
      instance
5. Renderer draws the standard scene (or a black background if no scene
   was loaded), then the UI quad on top within the same render pass

> Tested SWFs: simple Skyrim-SE AS2 menus (fader / loading / messagebox
> class) have rendered correctly in manual runs; this doc no longer
> pins a verified-result table because there is no automated assertion of
> those specific files in-tree. End-to-end verification is gated on the
> `_global.gfx` stub work below.

## What's not yet wired up

The M20 milestone (Phase 1) is the **infrastructure**: load a SWF, render
it offscreen, upload to Vulkan, draw on top. The full Bethesda menu pack
needs additional layers that are not yet implemented:

### Scaleform host methods and remaining `_global.gfx` stubs

Bethesda menus call into a small set of Scaleform-specific globals for
layout, locale, and texture loading. The Ruffle ExternalInterface transport
is installed and the first host inventory is checked in.

Skyrim's AS2 [`GameDelegate`](https://github.com/schlangster/skyui/blob/master/src/CLIK/gfx/io/GameDelegate.as)
passes the logical method as the ExternalInterface method name and prepends
a numeric request ID to its arguments. When the call supplied an
ActionScript callback, the host must re-enter the registered `respond`
callback with that ID before `ExternalInterface.call` returns. Returning a
value directly does not complete that protocol. `ScaleformHostBridge`
now models this exactly: calls retain their request ID, configured response
values or argument-dependent response handlers use the re-entrant callback
path, and diagnostics distinguish known commands, missing responses,
registered extensions, and unknown methods.

`ScaleformHostCatalog::for_profile(SkyrimAvm1)` contains the 74 literal
`GameDelegate.call` method names in the SkyUI source tree pinned at
`835428728e2305865e220fdfc99d791434955eb1`; 12 are marked as callback
requests. Catalog entries are recognition and protocol metadata, not claims
that gameplay behavior exists. The engine must still implement or explicitly
stub each drained call.

Fallout 4 does not use Skyrim's protocol. Its dynamic menu root constructs
`BGSCodeObj`; native code fills that object with function values and invokes
`onCodeObjCreate`, while `onCodeObjDestruction` clears the reference. The
menu-side `BGSExternalInterface.call` helper is only a null-safe lookup and
`Function.apply` wrapper around those installed functions—it is not Flash's
ExternalInterface transport. The reconstructed
[F4CF interface source](https://github.com/F4CF/Interface) describes itself
as intended to track vanilla closely and yields 129 distinct calls through
that object. Installed-ABC inventory adds nine methods used by the shipped
Atomic Command holotape (`closeHolotape`, high-score get/set, action animation,
and registered-sound controls), for 138 total. The catalog preserves case
because FO4 contains forms such as `CloseMenu`, `closeMenu`, `PlaySound`, and
`playSound`.

Ruffle deliberately keeps its AVM2 object model private. Rather than fork the
VM, `avm2_host.rs` locates the class that declares `BGSCodeObj`,
`onCodeObjCreate`, and `onCodeObjDestruction`, rewrites that constructor
in-memory immediately after its `BGSCodeObj` initialization, and inserts an
eager ABC adapter before the class ABC. This avoids Ruffle's intentionally
stubbed `LoaderInfo.getLoaderInfoByDefinition` root lookup and also brings the
host contract up before later GFx-dependent constructor work can fail. The
adapter populates the menu-created object with one forwarding function per
catalog method and calls the lifecycle hook. Each forwarding function uses
Ruffle's supported ExternalInterface boundary with a namespaced transport
method such as `BGSCodeObj.PlaySound`; `ScaleformHostBridge` normalizes that
to logical method `PlaySound` while retaining
`host_object = Some("BGSCodeObj")`.
Immediate response handlers therefore also work for FO4 functions used as
queries. Reserved readiness and destruction callbacks are registered only
after installation; dropping `SwfPlayer` invokes the latter, and a private
acknowledgement increments `code_object_destruction_count()` only after the
menu's `onCodeObjDestruction` hook returns.
F4SE independently demonstrates the same general extension pattern by
installing function objects on `root.f4se` in
[`Hooks_Scaleform.cpp`](https://github.com/ianpatt/f4se/blob/master/f4se/Hooks_Scaleform.cpp).

Generated ABC structure and malformed contracts are covered by default tests.
Ignored installed-corpus tests load HUD, Pip-Boy, and Atomic Command from
`Fallout4 - Interface.ba2`. HUD and Pip-Boy both expose a `true` readiness
callback and acknowledge destruction after drop; Atomic Command is correctly
classified as a child program with no standalone lifecycle. A bytecode
inventory test follows both direct `BGSCodeObj.method(...)` calls and
`BGSExternalInterface.call(BGSCodeObj, "method", ...)`, proving all three
representatives are covered by the 138-method catalog. Future DLC/mod menus
can still surface additions through the same inventory and unknown-method
diagnostics.

| Profile | What exists now | What must be created |
|---|---|---|
| Skyrim AVM1 | `GameDelegate` transport, 74 recognized methods, 12 request contracts, response re-entry | Per-method engine behavior and remaining `_global.gfx` compatibility |
| Fallout 4 AVM2 | `BGSCodeObj` lifecycle, 138 installed-corpus methods, generated forwarding ABC, object-aware dispatch, BA2-backed imports, HUD/Pip-Boy/Atomic Command lifecycle and inventory checks | Per-method engine behavior and remaining GFx compatibility |
| FO3/FNV | XML corpus confirmed; no SWF profile | Separate legacy XML UI runtime or translation path |

### Papyrus ↔ UI bridge

In Bethesda's runtime, scripts (Papyrus / ECS-native) communicate with
menus via a queue of "UI events". A pause menu might receive
`OnButtonPress("Resume")` and the script handles it. We need to wire
ECS events ↔ Ruffle's ActionScript event system. (The engine-side event
plumbing now exists — see the event/condition runtime, M47.0/M47.1 — so
this is a matter of connecting it to Ruffle, not building it from zero.)

### Input routing

Loading a menu now grants `UiManager` input focus and closing or replacing it
sends the corresponding focus transition before dropping the player.
`byroredux/src/ui_input.rs` translates winit's physical and logical keyboard
identity, key location, text controls, Unicode input, IME composition, pointer
coordinates/buttons, and wheel units into `UiInputEvent`. `SwfPlayer` forwards
that platform-neutral contract to `Player::handle_event`; native pointer
enter/leave also updates Ruffle's separate `mouse_in_stage` state.

Dispatch order is egui debug overlay → focused Scaleform menu → world input.
A focused menu captures the input class even if no individual ActionScript
listener reports it handled, so modal menus cannot leak Escape, movement, or
mouse-look into gameplay. Any previously held world keys and cursor grab are
released on transfer. F3 remains an engine-global debug-overlay shortcut.
Pointer coordinates are scaled from the current native window extent into the
movie's persistent viewport, which also keeps input aligned after a swapchain
resize.

The current manager still owns one active menu. A real menu stack must define
which visible layer receives focus and how non-modal HUD movies coexist with a
modal pause/container/Pip-Boy layer. Gamepad translation is also a later slice;
the input contract intentionally starts with the native keyboard/pointer path
used by the existing engine loop.

### Font loading

Bethesda ships custom font SWFs that menus load at startup. The archive
navigator now delivers Fallout 4's `fonts_en.swf` through `ImportAssets`;
per-game font mapping, fallback selection, and visual-fidelity coverage
remain.

### Menu pack

Once the four pieces above land, the Skyrim ~34 menus (and FO4's larger
set, including the Pip-Boy) should load with minimal per-menu work.

See the [Creation Engine UI](../legacy/creation-engine-ui.md) legacy doc
for the menu catalog and the [Papyrus API Reference](../legacy/papyrus-api-reference.md)
notes for the format-string system menus rely on.

## Tests

The UI crate has 16 default tests plus three ignored installed-corpus
smokes; the executable adds three winit-translation tests. The synthetic
non-Bethesda SWFs come from Ruffle's pinned ExternalInterface fixtures:

- The `byroredux-ui` crate compiles as part of the workspace.
- Real headless AVM1 and AVM2 movies receive representative focus, pointer,
  key, and text-control events before verifying ActionScript → host calls,
  callback discovery, and host → ActionScript invocation through Ruffle's
  null renderer.
- Unit coverage pins profile detection, the 74-method sorted catalog,
  profile isolation, Skyrim request-ID normalization and response routing,
  dispatch diagnostics, monotonically sequenced calls, nested value
  conversion, Ruffle event conversion, winit key semantics, text-control
  modifiers, and window-to-movie coordinate scaling.
- Navigator coverage pins relative archive resolution, root confinement and
  percent decoding, the imported-AVM2 frame-boundary workaround, executor
  pumping, and frame-1 advancement. The ignored Fallout 4 corpus test repeats
  that sequence against the installed BA2.
- The renderer-side UI contract is covered by tests, not the Ruffle glue:
  `UiVertex` size/offsets (`crates/renderer/src/vertex.rs`), the bindless
  layout match between `triangle.frag` and `ui.frag`
  (`texture_registry_bindless_tests.rs`), and the `ui.vert` GpuInstance /
  `textureIndex` contract + descriptor reflection
  (`crates/renderer/src/vulkan/scene_buffer/`).
- Manual: `cargo run -- --swf path.swf` for each tested menu.

Real Bethesda movies remain local/ignored corpus tests: proprietary SWFs
must not be committed. The host bridge now provides the observable call
queue needed for those compatibility assertions.

## Draining the host bridge

`ScaleformHostBridge` is drain-based: `record_call` enqueues every
ActionScript→engine call and `drain_calls` is the only thing that removes
one. Until #2714 the engine never drained it — the binary's whole use of
`crates/ui` was `new` / `load_swf` / `tick` / `render` / input — so the queue
grew for the life of a loaded menu. The main loop now drains it once per
frame beside `ui.tick(dt)` (`byroredux/src/main.rs`), which is what keeps the
backlog at its natural depth, and logs each call at `debug` plus a one-shot
`warn` for any method the bridge classified `Unknown` or `MissingResponse`.
That turns `unknown_methods()` / `unanswered_methods()` into live
diagnostics rather than test-only ones. Acting on the calls — routing them
into quest / inventory / player state — remains M48 work.

`MAX_QUEUED_CALLS` (1024, drop-oldest, counted by `dropped_calls()`) backs
that up for the case where the drain does not run. It is a backstop rather
than a capacity estimate: measured against the installed corpora, every
vanilla menu tested — Skyrim SE `hudmenu` / `inventorymenu` / `magicmenu`,
FO4 `hudmenu` / `pipboymenu` / `containermenu` — produced **at most one**
host call across 600 ticked frames with periodic input bursts, because
Bethesda menus wait to be called *into* before they call back out. Anyone
sizing this number should re-measure rather than reason from the "HUD menus
call the host every frame" intuition, which the corpus does not support.

## Related docs

- [Creation Engine UI](../legacy/creation-engine-ui.md) — Bethesda menu
  catalog, Scaleform extensions, the GFx interpreter contract
- [Papyrus API Reference](../legacy/papyrus-api-reference.md) — what UI
  events a menu can receive, what the script side needs to expose
- [Vulkan Renderer](renderer.md) — the `pipeline_ui` setup, bindless
  texture array, and `update_rgba` deferred-destruction path
- [Game Loop](game-loop.md) — where the inline UI tick/render fits in the
  per-frame flow
