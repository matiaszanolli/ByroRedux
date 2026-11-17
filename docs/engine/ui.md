# UI System — Scaleform / SWF via Ruffle

Skyrim and Fallout 4 use Scaleform GFx (Adobe Flash / SWF) for menus
including the HUD, dialogue, and Fallout 4's Pip-Boy. This is not the
menu format used by every supported game: Fallout: New Vegas uses legacy
XML tiles and NIF assets for its loading menu. Skyrim ships ~34 SWF menus;
Fallout 4 ships even more.

ByroRedux integrates [**Ruffle**](https://github.com/ruffle-rs/ruffle)
(the open-source Flash player written in Rust) as a library, so we can
load and run those SWF menus without writing a Flash interpreter from
scratch and without linking to Adobe's GFx runtime.

Source: [`crates/ui/src/`](../../crates/ui/src/)

> Status note (2026-07-26): R4 selected pinned Ruffle plus ByroRedux-owned
> Bethesda profiles. The first M48 slice now includes profile detection,
> a bidirectional ExternalInterface bridge, and a pinned 142-method
> Skyrim/SkyUI host catalog. The second slice adds Fallout 4's
> `BGSCodeObj` lifecycle, a 269-method installed-corpus catalog, and
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
| Status                 | Loose SWF demo (`--swf path.swf`), archive-backed vanilla menu launch (`--menu interface\\hudmenu.swf --menu-archive <BSA-or-BA2>`), and the M48.6 `--hud` Scaleform HUD route (vanilla `hudmenu.swf` over the live frame, `hud.*` console, transparent stage); AVM1/Skyrim and AVM2/Fallout 4 profiles; bidirectional host bridge; Skyrim `GameDelegate` and Fallout 4 `BGSCodeObj` contracts; BSA/BA2-relative `ImportAssets` loading; focused winit input routing. The archive-backed route is verified end to end against real game data by [`docs/smoke-tests/m48-menu-load.sh`](../smoke-tests/m48-menu-load.sh) (#3273) — it needs a Vulkan device, so `cargo test` covers only the CLI-argument parser |
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
├── host/        `host.rs`'s own test module
├── input.rs     Platform-neutral keyboard, pointer, text, IME, and focus events
├── lib.rs       UiManager — top-level handle: owns the active SwfPlayer,
│                visibility/input-focus/viewport state, load/tick/render
│                (no `close` — deleted under #2723)
├── navigator.rs Archive-backed relative URL resolution, resource diagnostics,
│                Ruffle future executor, and ImportAssets preload compatibility
├── player.rs    SwfPlayer — Ruffle wrapper, own wgpu/Vulkan device,
│                offscreen TextureTarget, capture_frame() → cached RGBA buffer
├── prepare.rs   prepare_movie — one decompress + at most one tag walk shared
│                by every load stage instead of each redoing it (#2968)
└── profile.rs   Skyrim AVM1 / Fallout 4 AVM2 host profiles and detection
```

The catalog, player, profile, host bridge, host call/dispatch status, and
typed value are re-exported from `byroredux_ui`; `UiManager` remains
defined directly in `lib.rs`.

## Pipeline

```
SWF file bytes
        │
        ▼  prepare::prepare_movie — one decompress + at most one tag walk (#2968),
        ▼  shared by profile detection, host-object injection, ImportAssets
        ▼  extraction, and the from_data call below (each used to redo this)
PreparedMovie
        │
        ▼  ruffle_core::tag_utils::SwfMovieData::from_data(…, None)
parsed SwfMovieData
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
        ▼  byroredux::app_frame → texture_registry.update_rgba(layer.texture(), …), one per open layer
queued pixels copied into the existing VkImage by the next frame submission
        │
        ▼  presentation pass: fullscreen tone-map triangle writes the swapchain
        ▼  then bind the overlay pipeline (no depth, alpha blend, bindless sampler)
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
    pacer: RenderPacer,      // render-and-readback pacing, unpaced unless opted in (#4717)
    render_passes: u64,      // passes that reached Ruffle's render()
    uploaded_once: bool,     // gates the "unchanged content" early-out below
    host_bridge: ScaleformHostBridge,
    host_object_state: ScaleformHostObjectState,
    navigator_runtime: Option<ScaleformNavigatorRuntime>,
    resource_errors: Vec<String>,
    resource_errors_capped: bool,
    resource_loads: Vec<ScaleformResourceLoad>,  // deduplicated by archive_path (#2967)
    resource_loads_capped: bool,
    preload_stall_frames: u32,
    preload_stalled: bool,
}

impl SwfPlayer {
    pub fn new(swf_data: &[u8], width: u32, height: u32) -> anyhow::Result<Self>;
    pub fn new_with_profile(
        swf_data: &[u8],
        width: u32,
        height: u32,
        profile: ScaleformProfile,           // explicit; a contradicting movie is refused
    ) -> anyhow::Result<Self>;
    pub fn from_resource_provider(
        provider: Arc<dyn ScaleformResourceProvider>,
        movie_path: &str,
        width: u32,
        height: u32,
        profile: Option<ScaleformProfile>,   // None = let prepare_movie detect (#3771)
    ) -> anyhow::Result<Self>;
    pub fn tick(&mut self, dt: f64);          // seconds; wrapped in FloatDuration internally
    pub fn render(&mut self) -> Option<&[u8]>; // borrows pixel_buffer; None if the pixels are unchanged
    pub fn set_render_pacing(&mut self, pacing: RenderPacing);
    pub fn render_passes(&self) -> u64;
    pub fn handle_input(&mut self, event: UiInputEvent) -> bool;
    pub fn set_mouse_in_stage(&mut self, is_in_stage: bool);
    pub fn set_stage_transparent(&mut self);
    pub fn dimensions(&self) -> (u32, u32);
    pub fn resize(&mut self, width: u32, height: u32) -> bool; // in place, Stage.onResize (GUI-REG-008)
    pub fn host_object_state(&self) -> ScaleformHostObjectState;
    pub fn current_frame(&self) -> Option<u16>;
    pub fn resource_loads(&self) -> &[ScaleformResourceLoad];
    pub fn resource_error(&self) -> Option<&str>;
    pub fn resource_errors(&self) -> &[String];
    pub fn preload_stalled(&self) -> bool;
    pub fn invoke_callback(
        &mut self,
        name: &str,
        arguments: impl IntoIterator<Item = ScaleformValue>,
    ) -> Option<ScaleformValue>;              // None = unknown / unregistered; Some(Null) = returned null
    pub fn profile(&self) -> ScaleformProfile;
    pub fn host_bridge(&self) -> ScaleformHostBridge;
}
```

`new()` parses the SWF (`SwfMovieData::from_data`, the Ruffle b91e82160 rename of `SwfMovie`), spins up a headless
wgpu/Vulkan device, builds an offscreen `TextureTarget` of the requested
size, wires it into a `WgpuRenderBackend`, attaches a software video
backend (`ruffle_video_software`), selects blocking preload for the
already-resident SWF bytes, and starts playback (`set_is_playing(true)`).
For a Fallout 4 contract movie, the in-memory ABC adapter is inserted before
this parse/build sequence and the lifecycle class constructor is patched to
bootstrap immediately after it initializes `BGSCodeObj`; the source asset on
disk is never modified.

`from_resource_provider()` loads the root and relative dependencies through
the same source. Its `profile` is `Option<ScaleformProfile>`, and **`None` is
the normal production value** (#3771): `prepare_movie` detects the profile from
the header of the decode it already performed, so passing `Some(..)` only makes
sense for a caller holding an *independent* profile source — it re-arms the
mismatch guard, which is a real check, but a caller that obtains the value by
extracting the archive entry and running `ScaleformProfile::detect` itself is
spending a second archive extraction and a second whole-stream inflate to
answer this module's own detection with itself. `--menu` did exactly that until
#3771 and now passes `None`, reading `UiLayer::profile()` afterward.
`load_swf_with_profile()` is the separate explicit-profile entry and keeps a
plain `ScaleformProfile`. `Ba2Archive` and `BsaArchive` implement
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
player lock, then marks the player **dirty** when Ruffle reports
`needs_render`. That flag is raised after *every* frame that runs, so a live
movie is dirty at its own frame rate even when nothing on screen moves.

`render()` is a no-op fast path when not dirty. When dirty it calls
`Player::render()`, downcasts the boxed renderer back to the concrete
`WgpuRenderBackend<TextureTarget>`, calls `capture_frame()`, and copies
the resulting `RgbaImage` into the reused `pixel_buffer` (with a size-
mismatch guard that logs and skips). It clears the dirty flag, then decides
on **content**, not on the render having happened (#2719): it compares the
freshly captured RGBA against `pixel_buffer` and returns `None` when the
pixels are unchanged and at least one upload has already gone out
(`uploaded_once`). A render that produced identical pixels therefore costs
no GPU upload. The width/height are the renderer-side surface
dimensions, **not** the SWF's native size — Ruffle scales internally.

**Render pacing (#4717).** Gating the upload on content still left the render
and the blocking full-target readback running on every dirty tick — measured
at 5–8 ms per pass at 1080p, 24–30 times a second, for a static always-on HUD.
`SwfPlayer::set_render_pacing(RenderPacing)` (opt-in; modal menus stay
unpaced) thins those passes out. Ruffle offers no finer "the picture changed"
signal than `needs_render` (its `Stage::invalidated` is raised only by
ActionScript's `Stage.invalidate()`), so the pacer learns from the one exact
signal: whether the last readback differed from the one before. While the
picture is changing, or the host has just written to the movie (`handle_input`,
`set_mouse_in_stage`, `invoke_callback`, `set_stage_transparent`), passes run at
most every `active_interval`. After `idle_after` consecutive byte-identical
passes with no host write, they drop to one per `idle_interval` — a probe, not a
freeze: a movie that starts animating on its own is found within one interval
and is active again. Time is the sum of the `dt`s given to `tick`. The Scaleform
HUD route installs 33 ms (the shared HUD cadence) / 250 ms / 8;
`SwfPlayer::render_passes()` counts the passes that ran. A static 30 Hz movie
drops from 150 passes per 150 ticks to 26.

## UiManager

The open movies, bottom to top (M48 Phase 1, #6653). Each layer is one
`SwfPlayer` with its menu name, the path it was loaded from, its visibility and
the engine's texture handle for it; every visible layer ticks and renders to
its own overlay texture, and the renderer draws the layers as consecutive
instances of the UI quad, so each blends over the one below
(`FrameInputs::ui_texture_handles`). Which layer receives input is the menu
registry's decision (GUI-REG-009): the app hands it over with
`set_input_owner` every frame.

```rust
pub struct UiLayer { /* player, menu_name, path, visible, texture, id */ }

impl UiLayer {
    pub fn menu_name(&self) -> &str;             // canonical name (#4757)
    pub fn path(&self) -> &str;                  // archive path / loose file it was loaded as
    pub fn id(&self) -> u64;                     // new per installed player (#3772 latches)
    pub fn visible(&self) -> bool;               // hud.off hides without closing
    pub fn set_visible(&mut self, visible: bool);
    pub fn texture(&self) -> Option<u32>;        // the engine's compositor handle
    pub fn set_texture(&mut self, texture: Option<u32>);
    pub fn profile(&self) -> ScaleformProfile;   // resolved profile (#3771)
    pub fn host_bridge(&self) -> ScaleformHostBridge;
    pub fn drain_host_calls(&self) -> Vec<ScaleformHostCall>;   // once/frame per layer (#2714)
    pub fn dropped_host_calls(&self) -> u64;     // latched per layer id by the app (#2969 / #3772)
    pub fn invoke_callback(
        &mut self,
        name: &str,
        arguments: impl IntoIterator<Item = ScaleformValue>,
    ) -> Option<ScaleformValue>;
    pub fn render(&mut self) -> UiFrame<'_>;     // Fresh / Unchanged / Hidden (#2972)
}

pub struct UiManager {
    layers: Vec<UiLayer>,          // bottom to top
    input_owner: Option<String>,   // the layer that receives UiInputEvent
    pub width: u32,
    pub height: u32,
}

impl UiManager {
    pub fn new(width: u32, height: u32) -> Self;
    pub fn load_swf(&mut self, swf_data: &[u8], name: &str) -> anyhow::Result<()>;
    pub fn load_swf_with_profile(
        &mut self,
        swf_data: &[u8],
        name: &str,
        profile: ScaleformProfile,
    ) -> anyhow::Result<()>;
    pub fn load_swf_from_resource_provider(
        &mut self,
        provider: Arc<dyn ScaleformResourceProvider>,
        movie_path: &str,
        name: &str,
        profile: Option<ScaleformProfile>,   // None = let prepare_movie detect (#3771)
    ) -> anyhow::Result<()>;              // pushes a layer on top; a same-named menu is replaced in place
    pub fn close(&mut self, name: &str) -> Option<UiLayer>;   // GUI-REG-007; caller releases the texture
    pub fn layers(&self) -> &[UiLayer];
    pub fn layer_mut(&mut self, name: &str) -> Option<&mut UiLayer>;
    pub fn tick(&mut self, dt: f64);             // every visible layer
    pub fn resize(&mut self, width: u32, height: u32) -> bool; // GUI-REG-008, every layer
    pub fn device_count() -> u32;                // GUI-REG-005: 1 once any movie exists (#2733)
    pub fn set_input_owner(&mut self, owner: Option<&str>) -> bool; // FocusLost / FocusGained once
    pub fn has_input_focus(&self) -> bool;
    pub fn handle_input(&mut self, event: UiInputEvent) -> bool;    // to the input owner only
    pub fn set_mouse_in_stage(&mut self, is_in_stage: bool) -> bool;
}
```

`UiManager` is **deliberately not** an ECS `World` resource — Ruffle's
`Player` owns non-`Send`/`Sync` backends (video, audio), so the manager
is held directly on the main `App` struct (`App::ui.manager`) alongside the
`VulkanContext`, and ticked / rendered inline in the per-frame loop rather than
through the scheduler. What the stack holds reaches the rest of the engine
through the menu registry (below), which is a `World` resource.

**Resize (GUI-REG-008).** The app reads the swapchain extent once a frame
(`App::sync_ui_extent`, before the overlay ticks) and resizes every layer
(`SwfPlayer::resize` → `Player::set_viewport_dimensions` → `TextureTarget::resize`,
which re-lays the stage and raises `Stage.onResize` under `noScale`) and the
MenuXml HUD (`MenuXmlHud::resize`: new screen traits, the engine-anchored meters
and sneak text re-placed, its texture replaced at the new size). The next upload
of each layer goes through `update_rgba`'s extent-change path (new image, old one
deferred, same handle). This retires #2723's documented stretch: its premise
("no in-place resize path") did not hold at the pinned Ruffle (#6643). Pinned by
`a_resized_player_renders_at_the_new_extent_on_the_shared_device` (GPU) and
`a_viewport_resize_reaches_the_movie_as_stage_on_resize` (`crates/ui/tests/avm1_path_access_probe.rs`).

## Vulkan integration

The UI is drawn at the tail of the **presentation** pass — the
output-resolution pass that writes the swapchain — immediately after its
fullscreen tone-map triangle and inside the same subpass (#3426). It is
therefore composited onto the finished, tone-mapped, upscaled image, and
nothing downstream touches it.

Before #3426 it was the tail of the *main geometry* render pass, blended
into the render-resolution HDR G-buffer. Every menu consequently went
through height fog / volumetric transmittance keyed off the world depth
behind it, the bloom add, TAA accumulation with a zero motion vector and no
FSR reactive or transparency mask, FSR upscaling, and finally
`aces(graded * exposure)` in `presentation.frag` — which maps linear 1.0 to
~0.80, so pure-white menu chrome reached the swapchain at ~80 % grey.

The colour round-trip needs no shader work and no format change: Ruffle's
capture is sRGB-encoded bytes uploaded as `R8G8B8A8_SRGB`, so the sampler
linearises it, Vulkan blends in linear space against the **sRGB** swapchain
attachment, and the hardware re-encodes on write. (Re-uploading the capture
as `_UNORM` would double-encode it against this target.)

The renderer side has a dedicated UI pipeline
(`pipeline::create_ui_pipeline`), built against the presentation render pass
and owned by `PresentationPipeline` — which is what a swapchain recreate
rebuilds — with:

- **No depth test / no depth write / no stencil** — UI draws on top of
  the world (`depth_test_enable(false)`, `depth_write_enable(false)`,
  `stencil_test_enable(false)`; world-geometry stencil lives in the
  opaque/blend pipelines, #337).
- **Alpha blend** on the presentation pass's single swapchain color
  attachment (`SRC_ALPHA`, `ONE_MINUS_SRC_ALPHA`; alpha channel
  `ONE`/`ZERO`). The eight-entry blend table the pipeline used to carry —
  RGBA into HDR slot 0, `color_write_mask(empty)` on normal / motion /
  mesh-id / raw-indirect / albedo / the two FSR masks — went away with the
  move: there is one attachment to blend into and no G-buffer to keep the
  overlay out of.
- **Lightweight vertex format.** The UI quad uses `UiVertex` (position +
  UV only, **20 bytes** — `[f32; 3]` + `[f32; 2]`, 2 attribute
  descriptions) rather than the full 104-byte scene `Vertex`. The split
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
  itself. `PresentationPipeline::record_overlay` re-sets viewport/scissor
  after binding the overlay pipeline (defensive, #133) and a `const`
  assertion fires if anyone grows the dynamic-state list without extending
  the explicit `cmd_set_*` calls (#663). Both descriptor sets are (re)bound
  there too: the tone-map draw immediately before it binds this pass's own
  set 0, which is layout-incompatible with the scene layout.

### Texture upload — `register_rgba` / `update_rgba`

There is no bespoke *update_ui_texture* entry point; the UI texture is an
ordinary entry in the renderer's `TextureRegistry`:

- When a movie layer loads (`--swf` / `--menu` in `byroredux/src/scene.rs`,
  `--hud`, `ui.open`) a transparent-black RGBA buffer is registered with
  `texture_registry.register_rgba(...)` and the handle is stored on the layer
  (`UiLayer::set_texture`); `ui.close` drops it.
- Each frame, when a layer's `render()` returns a fresh buffer, the main
  loop calls `texture_registry.update_rgba(handle, w, h, pixels)`, and the
  frame composites every visible layer's handle in stack order.

For matching single-mip RGBA images, `update_rgba` queues the latest pixels
in a retained CPU buffer. After the frame-slot fence wait, the renderer copies
them into that slot's retained staging arena and records the image transfer
at the start of the frame command buffer. Graphics-queue barriers order prior
sampling, the overwrite, and subsequent sampling. The image, image view and
bindless descriptor stay fixed; steady updates require no one-shot submission
or upload fence wait (#3429). Updates coalesce until recording and are consumed
only after a successful frame submission, so an abandoned frame retries them.

An extent/format change still creates a replacement image. The old image enters
the existing deferred-destroy ring and descriptor writes follow the per-slot
fence protocol. Initial registration and these resize/format replacements
still use the synchronous upload path.

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

### Original-game loading screens during scene transitions

Requested 2026-09-16; **legacy door/save-load artwork/tips implemented;
full original menu integration remains incomplete**. Display the active game's
own loading presentation while changing scenes, using installed game assets
rather than shipping copies or substituting an unrelated generic screen.

Installed FNV evidence: `Fallout - Misc.bsa` contains
`menus\loading_menu.xml` (22,111 bytes). It declares a `LoadingMenu` with
image slides, tip text, `Interface\Loading\loading_text_bar.dds`, and
`Interface\Loading\LoadingAnim01.NIF`. `FalloutNV.esm` record
`GoodspringsLoadScreen01` (`0011317C`) supplies `ICON`
`interface\loading\loading_billboard09.dds`, a `DESC` tip, and an `LNAM`
location restriction. `LSCR` previously used `MinimalEsmRecord`,
discarding these fields; `UiManager` still only plays SWF. Loading the XML through
Ruffle is therefore not a viable integration.

`records/load_screen.rs` now decodes the legacy image/location records and
Skyrim/FO4/Starfield model presentation fields, using the
[xEdit record definitions](https://github.com/TES5Edit/TES5Edit/tree/dev-4.1.6/Core)
and installed FNV/Skyrim data. It preserves record flags, localized tips,
remapped references, conditions, camera paths, and authored transforms.
Malformed known fields are recorded explicitly so a future selector can
reject them instead of treating a broken restriction as unrestricted.
The existing load-order merge/deletion machinery owns the typed LSCR map.
FO76-specific presentation fields remain unimplemented; selection and
presentation consumers must not infer support from a nonempty map alone.

The installed-data probe (`cargo run -p byroredux-plugin --example
probe_load_screens -- <plugin.esm> [editor-id substring]`) decoded all **208
FNV** and **298 Skyrim SE** records with zero malformed fields. It loads
loose companion string tables only and reports unresolved tips explicitly;
the Skyrim probe retained lstring placeholders, not verified translated
text. The actual runtime uses the existing archive-backed string-table
loader. These results establish data decoding, not screen selection,
asset rendering, or transition lifecycle correctness.

The first live presentation backend is `byroredux/src/loading_screen.rs`.
For legacy games it retains one valid, unconditional LSCR image from the
active texture archives and its original tip. A loading frame must actually
present before a queued door transition or live save load can tear down the scene. The
cover remains through a destination frame with resident geometry and drained
texture uploads. Gameplay input is cleared, simulation dt is zero while
the debug drain continues, existing menus are preserved, and capture resumes
only if it was held before loading and no focus-loss event intervened.
Unsupported/missing artwork falls back to the existing transition path.
Save covers have a separate presentation gate: the snapshot stays in the
last-writer-wins queue until presentation, and door drains cannot consume
that gate. Save loads defer until an active door transition finishes. Queue
cancellation releases the cover; a synchronous load failure returns through
the same final-frame dismissal path rather than leaving input captured.

Live FNV captures at `/tmp/byro-loading-screen.N5ngpN/loading-fixed.png` and
`after-loading.png` show original magazine artwork (`LSCR 0002186A`) and its
Barter tip, followed by the exterior scene with the capsule grounded. This
also exposed and fixed the shared UI quad's inverted vertical UV mapping
under the positive-height presentation viewport. Unit suites passed:
2,202 engine, 1,083 renderer, and 13 debug-UI tests (ignored tests excluded).
The full FNV P1 round trip subsequently exited 0 with added ordered assertions
for both loading-screen lifecycles (begin -> presented -> transition applied
-> dismissed), two streaming crossings, exactly two activations, and grounded
return. Ordinary streaming crossings did not produce loading covers. The
smoke-contract checker and shell syntax checks also passed.

The save-cover extension passed all 2,204 engine unit tests (34 ignored),
including separate door/save ownership and once-only presentation tests.
The first live FNV attempt in `/tmp/byro-save-cover.X3PEI6/` could not reach a
successful save reload: existing hierarchy validation refused to write the
saloon snapshot (9 inconsistent parent/child links), then the exterior
snapshot (19). Validation was not bypassed. Door presentation/dismissal still
worked in that run.

The follow-up traced the failing links to embedded NIF animation players:
the loader inserted `Children` ownership for unload cleanup but omitted the
reciprocal `Parent`. `spawn_embedded_animation_player` now installs both
edges and retains authored playback phase/root targeting. Two regression
tests cover rooted/rootless ownership, including the real save validator;
all **2,206 engine tests passed** (34 ignored). With validation unchanged,
two consecutive saloon save/reload cycles succeeded, each applying 531 deltas
across 462 matched FormIDs and restoring the grounded character at
`(45.00, 3524.02, 741.00)`. `engine-fixed.stderr` records both ordered save
cover lifecycles; `save-loading.png` shows the original artwork/tip and
`after-save-load.png` shows the restored door prompt with no cover. These
are release-mode FNV checks, not cross-game or long-soak proof. A subsequent
exterior save/reload also completed: 2,738 deltas across 2,634 matched
FormIDs, nine resident cells, and the same ordered cover lifecycle. The
exterior save no longer reports the 19 hierarchy errors. Failed-drain
recovery remains a live-verification gap.

This backend does **not** yet execute `loading_menu.xml`, reproduce its
NIF animation/fonts, rotate/contextually select screens, cover initial boot,
or render Skyrim+ loading models. Only FNV has live artwork
verification; Oblivion/FO3 share the code path but still need installed-data
visual checks. The existing synchronous preparation can still stall the
event loop while the static cover remains visible. These are remaining
requirements, not reasons to declare all-game loading UI complete.

Implementation requirements:

- Decode each game's loading-screen records and selection constraints;
  preserve localization and load-order overrides. Do not show location-bound
  tips indiscriminately.
- Resolve the original assets through the active game's archive providers.
  Legacy XML/NIF presentation and Creation-era SWF/model presentation need
  their corresponding backends, not a single assumed SWF filename.
- Present a loading frame before scene teardown or synchronous preparation,
  keep it responsive during budgeted loading, and dismiss only when the
  destination's geometry and player collision are ready. Cover initial scene
  loading, door transitions, and save loads; ordinary seamless background
  streaming should remain unobstructed.
- Own input during loading without replacing or losing the previous menu;
  unwind on failure, cancellation, supersession, and resize. Missing assets
  must not prevent entering the game.
- Verify real captures during loading and after dismissal, failure recovery,
  and the existing grounded door/traversal gates. A standalone menu launch
  or parser test does not prove transition integration.

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

`ScaleformHostCatalog::for_profile(SkyrimAvm1)` carries 142 `GameDelegate.call`
method names: the 74 literal entries read off the SkyUI source tree pinned at
`835428728e2305865e220fdfc99d791434955eb1` plus the 68 the #3103 corpus sweep
measured in `Skyrim - Interface.bsa`. 16 are marked as callback requests —
every one by SkyUI's fourth-argument rule over measured evidence: 12 read
off the pinned sources, and 4 (`LoadDLC`, `RequestLoadingText`,
`GetMouseButtonForSetDestination`, `ShouldShowMod`) whose corpus call sites
pass scope + response callback (#4720/#5274 — the name-prefix heuristic
classifies no Skyrim entry any more; the corpus resolves 16 four-argument
requests and 125 two-argument commands with no mixed-arity name). Catalog
entries are recognition
and protocol metadata, not claims that gameplay behavior exists. The engine
must still implement or explicitly stub each drained call.

**Behaviour classes (M48 GUI-HOST-001…004, #6665).** The catalog compiles Phase
0's per-method review in (`docs/data/game-ui/host-classes-{skyrim,fallout4}.tsv`,
`crates/ui/src/behaviour.rs`): `ScaleformHostCatalog::behaviour(name)` gives a
method's *target* class (H1 served, H2 placeholder, H3 unserved), the H2 reason
and the canonical API an H1 method is to act through. Every recorded call also
carries what it actually met (`ScaleformHostCall::class`): H1 when the engine
registered a handler, H2 when the method's placeholder is realized (25 Skyrim
rows: console-only features, hosted services and movie-kept state are ignored;
`RequestIsOnPC` answers `true`, `ShouldShowKinectTunerOption` and
`ShouldShowMod` `false`, read off the callbacks in `quest_journal.swf`), H3 for
any other cataloged method (`Null`, whatever its target), H4 for one the catalog
lacks. H2 rows whose answer is not established or not produced stay H3. The
per-frame drain logs one diagnostic per (menu, method) for H3 and H4, naming the
class and the target; `ui.host [menu]` reports an open movie's calls per class
and its distinct H3 / H4 names (GUI-OPS-002). The corpus sweep (A7) asserts every
method the vanilla archive calls has a class: 141 called, targets H1 76, H2 34,
H3 31. No Skyrim method is H1 at run time yet: the H1 actions (GUI-HOST-002,
-005, -006, GUI-EVT-004) are the rest of Phase 3.

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
and registered-sound controls), for 138 total. A full 311-movie sweep of
`Fallout4 - Interface.ba2` (#2966, 2026-08-19) measured 131 more distinct
`BGSCodeObj` methods actually called by shipped menus but missing from those
138; the catalog was regenerated to the union, 269 entries, with the 131
sweep-added entries' `kind` (`Command`/`Request`) inferred from name prefix
(`Get*`/`Is*`/`Should*`/`Can*`/`get*`) rather than measured against source —
the original 138 had `kind` read directly from F4CF's reconstructed AS3 /
the installed-ABC inventory instead. #3773 — pre-fix, the array carried no
marker distinguishing the two provenances, so a maintainer had to re-derive
which 131 were guessed before trusting any entry's `kind`; every
`ScaleformHostMethod` now carries a `provenance:
ScaleformKindProvenance::{Measured, HeuristicNamePrefix}` field
(`crates/ui/src/catalog.rs`) recording exactly this split, pinned by
`fallout4_catalog_provenance_split_matches_the_2966_sweep`. The heuristic's
own boundary is demonstrably imprecise — every `Request`-classified entry
happens to match the prefix set, while at least 16 `Command`-classified
names carry query-shaped verbs (`Request*`, `Check*`, `Validate*`, …) the
prefix rule doesn't cover — so a `HeuristicNamePrefix` `Command` is a weaker
claim than a `Measured` one, not a wrong one; none of the 16 are asserted
misclassified. 45 of the 269 are unreferenced by that one archive — kept,
since Far Harbor/Nuka-World/Vault-Tec Workshop/Automatron/Creation Club
content lives in separate archives this measurement didn't have on disk to
sweep. The catalog preserves case because FO4 contains forms such as
`CloseMenu`, `closeMenu`, `PlaySound`, and `playSound`.

Ruffle deliberately keeps its AVM2 object model private. Rather than fork the
VM, `avm2_host.rs` locates the class that declares `BGSCodeObj` and
`onCodeObjCreate`, then records separately whether the optional
`onCodeObjDestruction` trait exists. It rewrites that constructor
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
queries. Reserved readiness is registered after installation. The destruction
callback is registered only when the movie declares that trait; in that case,
dropping `SwfPlayer` invokes it and a private acknowledgement increments
`code_object_destruction_count()` only after the hook returns. The public host
object state distinguishes adapters that lack this optional teardown hook.
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
representatives are covered by the 269-method catalog. Future DLC/mod menus
can still surface additions through the same inventory and unknown-method
diagnostics.

| Profile | What exists now | What must be created |
|---|---|---|
| Skyrim AVM1 | `GameDelegate` transport, 142 recognized methods, 16 request contracts, response re-entry | Per-method engine behavior and remaining `_global.gfx` compatibility |
| Fallout 4 AVM2 | `BGSCodeObj` lifecycle, 269 installed-corpus methods, generated forwarding ABC, object-aware dispatch, BA2-backed imports, HUD/Pip-Boy/Atomic Command lifecycle and inventory checks | Per-method engine behavior and remaining GFx compatibility |
| FO3/FNV | XML corpus confirmed; no SWF profile | Separate legacy XML UI runtime or translation path |

### Papyrus ↔ UI bridge

In Bethesda's runtime, scripts (Papyrus / ECS-native) communicate with
menus via a queue of "UI events". A pause menu might receive
`OnButtonPress("Resume")` and the script handles it. We need to wire
ECS events ↔ Ruffle's ActionScript event system. (The engine-side event
plumbing now exists — see the event/condition runtime, M47.0/M47.1 — so
this is a matter of connecting it to Ruffle, not building it from zero.)

### Input routing

Input goes to at most one menu: the menu registry's input owner, the topmost
open menu whose flags claim input (GUI-REG-009). When that menu is a movie the
app hands it Ruffle focus (`UiManager::set_input_owner`, once a frame after the
registry publishes); a native page or the HUD owning (or not claiming) input
leaves every movie unfocused. A newly loaded movie takes focus at load, and the
same frame's registry publish confirms or moves it; closing a movie
(`UiManager::close`, `ui.close`) hands input to the next owner in the same
frame (GUI-REG-007).
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

The native player-facing modal uses the same boundary: Escape opens pause and
Tab opens Inventory only when no focused Scaleform movie owns input. Both
release world input. Inventory presentation snapshots the player's
canonical `Inventory` and `EquipmentSlots`; equip actions are applied after the
egui frame, avoiding simultaneous UI reads and ECS mutation. Fallout 4/76 Mods,
Junk, and Misc are separate categories based on OMOD loose-item links and MISC
component data.

The HUD stays open, and drawn, under a modal (GUI-REG-003; the draw rule is the
open runtime-measured item below). Gamepad translation is a later slice;
the input contract intentionally starts with the native keyboard/pointer path
used by the existing engine loop.

### Menu registry, stack and path access (M48 design, Phase 0)

Design for [`docs/prd/game-ui-integration.md`](../prd/game-ui-integration.md)
(GUI-REG, GUI-PATH, GUI-XML), written under #6643. **Phase 1 (#6653) built the
registry and stack** below (`crates/core/src/ecs/resources/menu_registry/`,
`byroredux/src/menu_stack.rs`, `UiManager`'s layers above); the path access and
MenuXml input parts are Phases 2 and 3. Evidence is in `docs/data/game-ui/`
(tables named below) and `crates/ui/tests/avm1_path_access_probe.rs`.

#### Registry and stack (GUI-REG-001…011)

- **Where it lives.** A `MenuRegistry` resource in `byroredux-core`
  (`crates/core/src/ecs/resources/`). Its readers span the scripting crate
  (`MenuMode`, `Begin MenuMode`, `Utility.IsInMenuMode`), the SDK alias
  `ui.menu-is-open`, debug-ui and the binary; core is the one crate all of them
  reach. The registry is state; surfaces (movie, MenuXml document, native page)
  call one open / close API and own their presentation.
- **Entry.** `OpenMenu { name, code, surface, flags, opened_frame }`, ordered
  bottom to top. `name` is the game-facing name; `code` is the numeric
  `MenuMode` code on Oblivion / FO3 / FNV. `flags` = pauses game, counts toward
  menu mode, claims input, is HUD, allows saving.
- **Where flags come from.** The per-game tables
  `menu-flags-{skyrim,fallout4,fnv-fo3,oblivion}.tsv` and the code tables
  `menu-codes-{fnv-fo3,oblivion}.tsv`, compiled in and pinned by a parse test.
  Skyrim and Fallout 4 flags are the `IMenu` bit set (`kPausesGame` 1<<0 …,
  28 names, same in both); "counts toward menu mode" equals `kPausesGame` there
  (SKSE64 `UI::numPausesGame`, F4SE `numPauseGame`, CK `IsInMenuMode`), and the
  HUD carries neither, which is what keeps it out of menu mode (GUI-EVT-006).
  Oblivion, FO3 and FNV document **no** pause or menu-mode flag anywhere (GECK,
  CS, xNVSE, xOBSE, xEdit); the only per-menu datum is the XML
  `<stackingtype>`. Every such row is `engine_default_applied = yes`, and the
  default is an engine decision, not a sourced value: `&does_not_stack;`
  (HUDMainMenu, BreathMenu, QuickKeysMenu, HUDSafeZone, …) is an overlay (no
  pause, no menu mode, no input claim), everything else is modal. Skyrim and
  Fallout 4 menus no source documents (5 and 11) default to `kNone`
  (SKSE64 initialises `IMenu` flags to 0) and the table names them, as
  GUI-REG-004 requires. The VATS menu never pauses on its own account
  (`VATS-UI-007`), whatever bit a source gives it.
- **Derived answers.** Input owner = topmost entry that claims input
  (GUI-REG-009). Pause = any open entry pauses (GUI-REG-010; replaces the
  per-page `simulation_paused`). Menu mode = any entry counts (ObScript
  `MenuMode [code]` through the code table, `Begin MenuMode` blocks, Papyrus
  `Utility.IsInMenuMode`). `UI.IsMenuOpen(name)` = the name is in the stack.
- **HUD under a modal (GUI-REG-003).** *In the registry and reported open:
  settled.* Skyrim `UI::IsMenuOpen` is `menu->OnStack()` (`kOnStack`, 1<<6);
  the HUD's flags (`0x18902`) carry neither `kModal` nor `kPausesGame`.
  Fallout 4 `UI::GetMenuOpen` is `OnStack()` too. *Drawn: not settled by any
  public source.* Fallout 4's mechanism is `passesTopMenuTest &&
  menuCanBeVisible` with `kRendersUnderPauseMenu`; Skyrim's lever is not
  named; FO3 / FNV's HUD runs a state machine (`HUDMainMenu::HUDStates`) fed by
  the other menus. Default until measured: the registry keeps the HUD open and
  nothing hides it; the draw rule is a runtime-measured item (filed).
- **Surfaces and Q1.** The target surface per menu is derived, not hand-set:
  `scripts/game-ui-data/join_surfaces.py` appends `target_surface`,
  `fallback_surface` and the basis to each `menu-inventory-*.tsv`. A movie whose
  every host call is H1 or H2 takes S1; one H3 call sends the whole menu to its
  native page (S3) if one exists, else S4 `recorded`, so a menu is never
  half-alive. Oblivion / FO3 / FNV documents take S2.
- **Movies (Q3).** `UiManager` becomes a set of players keyed by registry entry.
  All visible layers render as separate overlay textures in stack order; every
  player ticks, only the input owner receives `UiInputEvent`. Closing or failing
  to load returns input to the next owner in the same frame (GUI-REG-007).
- **One device (GUI-REG-005): already true.** `shared_descriptors()`
  (`crates/ui/src/player.rs`) creates the wgpu device once per process (#2733);
  PRD §3.1 row 2 is stale. The registry only has to report the count.

#### Phase 1 as built (#6653)

- **Tables.** The flags, codes and inventory TSVs are `include_str!`'d into
  `menu_registry/tables.rs` and parsed once per game; `phase0_tables_parse_with_the_flags_the_design_states`
  and `every_vanilla_menu_code_names_a_menu` pin the parse. Fallout 3 and New
  Vegas share one table (one `GameKind`, one code table; the New Vegas inventory
  is the superset). Fallout 76 and Starfield read the Fallout 4 table until
  Phase 6 inventories them. "Claims input" on Skyrim / Fallout 4 is any of
  `kUsesCursor`, `kUsesMenuContext`, `kModal`, `kUpdateUsesCursor`.
- **Engine defaults beyond the rule above, each named in the code.** The
  Oblivion / Fallout `DialogMenu` keeps the world running (menu mode, input, no
  pause): no source gives it a pause bit, the documented Skyrim / Fallout 4
  dialogue menus carry none, and the engine's dialogue page plays its timed
  responses through the scheduler (766e1746e). The operator overlay registers
  as the console (`MenuMode 3`, `UI.IsMenuOpen("Console")`) with input only — it
  is an engine tool and neither pauses the world it inspects (#5141) nor puts
  the game in menu mode. A crafting page on a game with no crafting menu
  (Oblivion, Fallout 4) registers as the engine name `Crafting Menu` with the
  game's default flags.
- **Publishing.** `App::step_menus` runs once a frame after the modal pages:
  sync the UI extent, apply `ui.open` / `ui.close`, then `MenuRegistry::publish`
  from the native layers (`native_menus`: the HUD first, the loading cover, the
  page under its game-facing name, a barter session, a message box waiting with
  no other page up, the overlay), the movie layers (named by the inventory's
  movie join, `interface\hudmenu.swf` → `HUD Menu`) and the surface-less menus
  `ui.open` opened. A menu still up keeps its place; a HUD goes to the bottom.
- **Pause.** `menu_stack::simulation_paused`: any open menu with `PAUSES_GAME`,
  except while the loading / movie cover holds the game, which already runs the
  scheduler at a held clock. A paused frame runs `run_menu_mode_frame` (quest
  and object `MenuMode` blocks, stage results) and the debug drain.
- **Scripts.** ObScript `MenuMode` reads `menu_mode_query`; `Begin GameMode`
  blocks (quest and object) skip menu-mode frames and `Begin MenuMode` blocks
  run on them (`obscript_object_menu_mode_system` is new); Papyrus
  `Utility.IsInMenuMode` is `papyrus_in_menu_mode`; the SDK `ui.menu-is-open`
  snapshot is every registry name.
- **Script-opened menus** (GUI-EVT-003, #6665). A script's menu request carries
  the menu's code (Oblivion, Fallout 3, New Vegas) or its script function, and
  the registry's tables resolve it to the game-facing name the Phase 0 inventory
  gives (`MenuDef::script_functions`, `MenuTable::by_script_function`, read off
  the inventory's `script_entry_points`). The character-creation screens open
  their native page, which publishes under that name; every other request is
  reported as S4 with its fallback (`script.requests`: `menu RouletteMenu code
  1082 (S4, recorded)`, `menu RaceSex Menu (S4, accepts the current appearance)
  from ShowRaceMenu`). `Game.ShowRaceMenu`'s auto-accept placeholder is that
  fallback. Papyrus menu functions the VM does not run yet (`ShowGiftMenu`,
  `ShowTrainingMenu`, `OpenInventory`, `ShowSPECIALMenu`) still decline their
  script.
- **Menu events** (GUI-EVT-001/002, #6668). Every opening and closing the
  registry makes (`open`, `close`, and a publish that drops a menu) leaves a
  `MenuTransition` under the table's spelling, at most 256 between drains.
  `menu_open_close_event_system` drains them each frame and queues Fallout 4's
  `OnMenuOpenCloseEvent(asMenuName, abOpening)` on every script registered for
  that name through `RegisterForMenuOpenCloseEvent` (Papyrus VM;
  `MenuEventRegistrations`, saved; details in
  [scripting.md](scripting.md#menu-open--close-events-m48)). Scenario A8's
  vanilla fixture waits on GUI-EVT-003 / the race menu: its handler is reached
  through `Game.ShowSPECIALMenu` / `ShowRaceMenu`.
- **Console.** `ui.menus`, `ui.open <menu>`, `ui.close <menu> [accept|cancel]`
  ([debug-cli.md](debug-cli.md)). `ui.open` loads a movie target from the
  game's interface archive, opens a bare-openable native page (Pause,
  Inventory) or records an absent menu; a MenuXml target falls back to its
  native page until Phase 3. Gate: `docs/smoke-tests/m48-menu-stack.sh` (A3).

#### Resize (GUI-REG-008)

Built in Phase 1; see [UiManager](#uimanager). The design as written:

Ruffle at the pinned revision **does** resize in place, so #2723's premise
("no in-place resize path") no longer holds: `Player::set_viewport_dimensions`
calls `WgpuRenderBackend::set_viewport_dimensions`, which calls
`TextureTarget::resize` (replaces the target) and rebuilds the surface, then
`Stage::build_matrices` re-lays the stage. The rebuild is therefore:

1. `SwfPlayer::resize(w, h)`: `set_viewport_dimensions`, reallocate
   `pixel_buffer`, update `width` / `height`, mark dirty.
2. The engine, after it recreates the swapchain, calls it for every open movie,
   then re-registers each UI texture at the new extent through the
   dynamic-RGBA extent-change path (new image, old one deferred, descriptor
   kept).
3. MenuXml: `MenuRenderer::set_screen(ScreenTraits::for_width(w))` and a fresh
   `frame_size()`.

Phase 1's tests resize a headless player (GPU-gated) and assert `dimensions()`
and the next frame's length, and a Ruffle player without a renderer asserts the
movie hears `Stage.onResize` with the new `Stage.width`. The live window-resize
path itself is not driven by a gate yet.

#### Path access (GUI-PATH-001…005, decision Q2) — built in Phase 2 (#6661)

**Q2's first question: can an AVM1 adapter reach `_root` members through
ExternalInterface? Yes** (`crates/ui/tests/avm1_path_access_probe.rs`, Phase 0).
Phase 2 built the path service on both VMs:

- **The contract** (`crates/ui/src/path.rs`). Three ExternalInterface callbacks,
  `__byro_path_get(p)`, `__byro_path_set(p, v)` and `__byro_path_call(p, args)`,
  plus the AVM2 adapter's `__byro_path_attach(p, class)` (#7036), all in
  `RESERVED_ENGINE_CALLBACKS` by exact name (GUI-PATH-004). Both adapters
  answer `{found, type, value}`, which `decode_reply` turns into a `PathOutcome`:
  `Found(value)`, `Absent` (GUI-PATH-002, reported once per path), `NotReady`
  (the adapter's callbacks are not registered yet) or `Queued`. `value` crosses
  back only for a number, string or boolean: a display object or class instance
  would be converted member by member across the boundary.
- **AVM1** (`crates/ui/src/avm1_path.rs`). A `DoAction` spliced into frame 1 of
  every AVM1 movie, after a leading `FileAttributes`, on the stock decode's tag
  stream (no tag parse, #5479). Get is `eval(p)` (`GetVariable` on the dotted
  string). Set and call split at the last dot and act on `eval(parent)`, so a call
  binds its parent as `this`; a bare name resolves on `_root`. A present member
  whose value is `undefined` reads as absent — AS2 cannot tell the two apart.
- **AVM2** (`crates/ui/src/avm2_host/path_helpers.rs`). Three more methods in the
  `BGSCodeObj` adapter plus a walker. AS3 has no `eval`, so the walker splits the
  path, checks `k in o` per segment (falling back to `getChildByName` for a
  display child that is not a declared variable) and indexes from the root the
  installer stores. The installer registers the callbacks after
  `onCodeObjCreate`. **Only movies carrying the `BGSCodeObj` lifecycle class get
  the AVM2 adapter**, so an AVM2 movie without it answers `NotReady`.
- **Attach** (`PathOp::Attach`, #7036, AVM2 only). Stands in for native code that
  instantiates a movie's exported widget classes: when the path's parent (a
  display container) has no child of the last segment's name, the adapter
  constructs the class from `ApplicationDomain.currentDomain` (the adapter's
  domain is the movie's), checks it is a `DisplayObject`, names it and
  `addChild`s it; a child already there is found as-is, so it is idempotent. An
  AVM1 movie answers `NotReady` (its `attachMovie` is reachable through a call).
- **Queue or refuse** (GUI-PATH-003), chosen per call: `UiLayer::path_op(op,
  WhenAbsent::Queue)` holds an operation whose target (or adapter) is not there
  yet and retries it after every tick, a newer operation on the same path
  replacing it (at most 64 per movie); `WhenAbsent::Refuse` answers now.
- **The only route** (GUI-PATH-005). `UiLayer::path_op` is the only engine code
  that reads, writes or invokes a movie member by path. The HUD driver's older
  SkyUI-class pushes call *registered callbacks* by name, not members by path.
  No script surface reaches it: the extender `UI.*` functions stay unserved until
  the Legacy Mod Compatibility PRD maps them, and then they go through it.
- **Tests (A6).** `avm1_adapter_sets_and_reads_a_root_path` (the real
  `prepare_movie` injection on a synthetic AVM1 movie) and
  `avm2_adapter_reports_an_absent_path` (the real adapter ABC, installed on a
  stand-in root by a driver script, as the patched lifecycle constructor does),
  plus `avm2_adapter_attaches_a_named_display_instance_once` (#7036), all on
  renderer-less Ruffle players, no GPU.

**What the runtime measurements settled** (Phase 0 read these from bytecode):

- **Skyrim's `call` callback does not reach the HUD handlers.**
  `invoke_callback("call", ["SetHealthMeterPercent", …])` leaves
  `lastHealthMeterPercent` untouched; the path service calls
  `_root.HUDMovieBaseInstance.SetHealthMeterPercent` directly instead.
- **Skyrim's meters need `abForce = false`.** The meters sit faded out (the
  `Health` clip on frame 1 of its 200-frame animation); only the non-forced branch
  runs `RunMeterAnim`, which fades them in, then tweens to the target.
- **Fallout 4's HP bar sits under its rads overlay.** `RadsBar_mc.Percent` starts
  at 1 and is drawn over the fill, so the HP meter shows nothing until the rads
  overlay is fed. The engine models no Fallout 4 radiation, so the driver feeds 0.
- **Fallout 4's compass widget has no ActionScript** (#7036). `Fallout4.exe`'s
  `HUDCompass` builds it: one `DirectionMarkerWidget` (the movie's exported
  letter-and-tick class) per cardinal in `OtherMarkerHolder_mc`, at 0 / 90 / 180 /
  270°, labelled from the translation table (`$Compass_North` → `N`); each update
  sets a marker's `x = −halfW · Δ / 50°` (`Δ = wrap(heading − angle)`,
  `halfW = (CompassBar_mc.width − 2) / 2` = 151) and hides it from `|Δ| ≥ 50°` on.
  Nothing scrolls: `CompassBar_mc` is a static strip. The driver
  (`Fallout4Compass`) does the same through attach and set. The labels set through
  `htmlText` with the authored `<b>`: Ruffle's `text` setter drops to the field's
  non-bold default format. Since #6665 the label is the key itself, as native sets
  it, translated by the patched Ruffle text pass (GUI-FONT-004), and draws in the
  Noto Sans fallback until Fallout 4's `$MAIN_Font_Bold` class is mapped (Phase 6).
  Quest and location markers are not modelled.

#### Dialogue menu on the game's movie (GUI-PAGE-004, Skyrim) — Phase 4 (#6666)

Q1's first menu. When the registry's game is Skyrim and its `Dialogue Menu` targets
a movie, a conversation opens `interface\dialoguemenu.swf` through the registry's
`open_movie_menu` instead of the native page (which stays the fallback when the
movie does not load). `byroredux/src/scaleform_dialogue.rs` stands in for native
code on both sides, from the movie's own `InitExtensions` and `GameDelegate.call`
sites:

- **Engine → movie** (path service on `_root.DialogueMenu_mc`, when
  `dialogue_snapshot` changes): `SetSpeakerName`, `ShowDialogueText` /
  `HideDialogueText`, `PopulateDialogueLists` (`text, topicIsNew, topicIndex` per
  topic, `topicIsNew` = not yet said, then `-1`), `DoShowDialogueList(true,
  false)`, and `OnVoiceReady` once per line (it arms the movie's skip / pick
  timer).
- **Movie → engine** (H1, registered with the bridge): `TopicClicked
  [topicIndex]` → `select_topic_by_form_id`, the native page's click path;
  `SkipText` → the line's voice stops and its speaking cue ends; `CloseMenu` →
  `end_open_conversation`; `FadeDone` → the layer closes (also after
  `MAX_FADE_FRAMES` if the movie never reports it). A conversation that ends
  elsewhere (Goodbye, `dialogue.end`, Escape, Inventory, barter, crafting) closes
  the movie through `leave_open_dialogue` / `close_dialogue_menu`.

Gate: `dt2-skyrim-forcegreet.sh` Phase P — `ui.menus` shows `Dialogue Menu …
surface=movie interface\dialoguemenu.swf`, and the page snapshot names Irileth,
lists topics and carries the speaking cue (sampled on a topic's answer: the 0.6 s
greeting is over by the first poll). The movie draws the speaker, topic list and
subtitle; the button icon glyph and the dialogue font class are not resolved yet.

#### Dialogue menu on the game's document (GUI-PAGE-004, New Vegas) — Phase 4 (#6666)

Q1 on the MenuXml track. When the registry's game is Fallout 3 / New Vegas and its
`DialogMenu` (code 1009) targets a MenuXml document, a conversation opens
`menus\dialog\dialog_menu.xml` (archives from `session_menu_assets`, no `--hud`
needed) instead of the native page, which stays the fallback.
`byroredux/src/menuxml_dialogue.rs` feeds what native code sets, read off the
document's own traits:

- the root's io traits `_DialogVisible` and `_ShowingText` (on while the line's
  speaking cue is; off shows the topic list in its place);
- `DM_SpeakerNameLabel` / `DM_SpeakerText` strings;
- a pool of 16 `DM_TopicTemplate` rows under `DM_TopicList`, each with
  `listindex`, `_y`, `height` (34), `string` and `_line_alpha` (128 for a topic
  already said, the native page's said-marker).

Input routes through `menu_xml_input` while the document owns it: a click on
`DM_ClickRect` or the line skips it, a click on `DM_Topic<i>` selects that topic
through `select_topic_by_form_id`; both reuse `scaleform_dialogue::apply`. The
document composites over the HUD unless a video or loading screen covers the
frame. Exits share `leave_open_dialogue` / `close_dialogue_menu` with S1.

Bringing it up fixed four MenuXml dialect gaps (all in `crates/menuxml`, pinned
by unit and corpus tests): `visible` propagates to descendants; the `.fnt`
record's last fields are kerning-left / kerning-right / ascent (text had been
spaced by ascent and top-aligned); `globals()` reads `menus\globals.xml`; and
text tiles report their displayed size.

Gate: `dt1-dialogue-layers.sh` Phase P — `ui.menus` shows `DialogMenu code=1009
surface=menuxml menus\dialog\dialog_menu.xml` and the page snapshot names Doc
Mitchell with the speaking cue. Live, the document draws the speaker and the
wrapped line in `Glow_Monofonto_VL_dialogs` on its panel. The list's bracket art
(`fade_to_top.dds` / `fade_to_bottom.dds`, `solid.dds`) does not resolve yet.

#### Native page labels (GUI-PAGE-005) — Phase 4 (#6666)

Where a native page stays the surface, its words come from the game's data.
`byroredux_core::ui_labels::UiLabels` is one table, resolved per load order beside the stealth
rules (`populate_scene_runtime`): each `LabelDef` names a word by its engine text (the key) and
the string `GMST`s that carry the game's word, tried in order (`Use` → `sInventoryUse`, `sUse`;
`gold` → `sInventoryCaps`, `sGold`). The load order's winning value is drawn when one is
authored, else the key. A value may be a printf template, which `UiLabels::format` fills
(`sNotEnoughVendorGold` takes the price and the purse; `sOpenWithKey` the key's name). The table
is a handle (`Arc`), so the panel snapshot carries it per frame for a refcount, and views that
outlive the frame (`InteractionPrompt::verb`, `VitalBarView::label`) hold `Arc<str>` clones.

- **Coverage.** Skyrim resolves 39 of the 57 labels from `Skyrim.esm` (through its string
  tables; the boot log prints `Native page labels: 39 of 57`). The rest are FO3 / FNV words
  (`HP`, `AP`, `RAD`, `Aid`, `Ammo`) or exist on Skyrim only as Scaleform `$` translations,
  which native pages do not read. **New Vegas resolves 0:** `FalloutNV.esm` authors none of
  these settings (its 648 `GMST`s are gameplay values); the defaults are in the packed
  executable (GUI-FONT-005's FNV source is open), so FNV draws the keys unless a plugin authors
  the setting.
- **Where it is used.** Every page in `crates/debug-ui/src/panels/game_menu.rs` and
  `chargen.rs` (buttons, headings, tabs, detail rows, the settings registry's labels), the
  interaction prompt verbs, the vitals labels, the barter refusals, the XP and key notices, and
  the quicksave / quickload lines.
- **#6825's residues.** Barter's currency word is the `gold` label (it was hard-coded); vitals
  colour by actor value (`VitalBarView::actor_value`), not by the label's spelling; the
  inventory's `Form ID` row and form-id hover text are gone; save / load diagnostics stay in
  the log and console, and the HUD gets the game's line (`sQuickSaving`, `sQuickLoading`,
  `sCantSaveNow`) or the engine's "Load failed.".
- **The inventory and its test.** `docs/data/game-ui/native-page-strings.tsv` lists every word
  the native pages draw: `game-string` (resolved through `UiLabels`; the row's string is the
  fallback) or `engine-text` with its reason (candidates whose setting means something else,
  such as `sLevelUp` being a menu caption, were moved here with that reason; none is left a
  candidate). `page_strings_tests.rs` draws each page (pause, settings, inventory, message box,
  barter, crafting, dialogue, character creation, the HUD prompt and vitals) twice: with every
  label resolved to a marker, where anything left beyond the snapshot's data must be listed
  engine text, and with the engine defaults, where the fallbacks must be listed too. Producer
  text that reaches the HUD message layer (notices) is listed but not drawn by a page test.

#### MenuXml input (GUI-XML-001…003) — built in Phase 3 (#6665)

`crates/menuxml` stays engine-neutral (no dependency on the UI or Ruffle crates).
The layout walk that builds the draw list also collects the **hit targets**
(`layout::build_layout`, `HitTarget`): visible tiles whose `target` trait is true,
whatever their alpha (vanilla's hit rects are alpha 0), with the clip window they
are seen through when they `clips`. `MenuRenderer::hit_test(x, y) -> Option<TileHit>`
returns the topmost (`TileHit` carries the tile, its name and its `<id>`);
`pointer_down` / `pointer_up` activate a target pressed and released in place,
once; `key(MenuKey::{Next, Previous, Activate})` moves a focus through the targets
in draw order from the `xdefault` one; `take_activations` drains. The binary-side
adapter is `byroredux/src/menu_xml_input.rs`: `route` maps
`UiInputEvent::{MouseMove, MouseDown, MouseUp, KeyDown}` (Down / Tab, Up, Enter /
Space) onto those, and `owns_input` is the registry's input-owner rule
(GUI-REG-009). Values a document reads go in through `set_override`,
`instantiate_template` and `graft_fragment` (GUI-XML-003).

Two dialect fixes came with it: **`<hotrect>`** (Fallout 3 / New Vegas list items
and the lists that hold them) is a rect tile; it used to parse as a trait, which
swallowed the tile and its subtree (the corpus pins grew: FNV 1,060 → 1,545
un-spliced tiles, FO3 736 → 1,054). **`io()`** reads the menu root's traits, the
ones engine code and scripts set (`SetUIFloat "HudMainMenu\_trait"`, GECK
*Tutorial: Understanding XML*); it used to read 0.

The Phase 3 exit's corpus-gated tests, `#[ignore = "needs FNV/FO3 game data on
disk"]`: `menu_xml_input::tests::fnv_menu_document_takes_pointer_input` and
`fo3_menu_document_takes_pointer_input`. Each loads `menus\message_menu.xml`
through the game's `MenuProfile`, stands in for what native code sets (the
menu's io traits, the list's size: `list_box.xml` "REQUIRED OVERRIDE" width,
"recommended override" height; per button `listindex`, `_y`, `string` and the
height native computes from the text), instantiates two buttons
(`MM_ButtonTemplate` under `MM_ButtonList`), and asserts that `hit_test` at a
button's centre returns it, a `UiInputEvent` down / up pair on it yields exactly
one activation for it, and a pair outside every tile yields none. The first live S2
surface for a non-HUD menu is the FNV dialogue document (Phase 4, above), which
routes the frame's input through `route` / `owns_input`.

#### Strings and fonts the design depends on

- **`-s` strings (GUI-FONT-005).** They are XML entities (`&-sContinue;`), not
  record references; 618 occurrences across the three corpora and
  `expand_entity` expands them to nothing today. Each names a built-in game
  setting whose default text lives in the executable, with the ESM overriding it:
  only 1 of 102 (FO3) and 1 of 145 (FNV) names has a `GMST` record at all
  (`sMorphine`), so "resolve through the plugin's game settings" alone would
  resolve one string per game. The design needs a per-game default-setting table
  plus ESM override and a case-insensitive lookup (`&-sOK;` is `sOk`). FO3's
  table is readable from `Fallout3.exe` (1,066 `s*` defaults); **FNV's source is
  open** because the on-disk executable is packed (filed).
- **Fallback font (Q5).** Noto Sans (SIL OFL 1.1), Regular and Bold, bundled.
  It is already installed here as static instances, covers Latin, Latin Extended
  (Polish), Cyrillic and Greek, and is redistributable. CJK stays a per-language
  fallback (Noto Sans CJK, OFL) subset at bundling time, not part of the base
  bundle. The OFL text ships beside the files.

### Font loading

Bethesda ships custom font SWFs that menus load at startup. The archive
navigator delivers Fallout 4's `fonts_en.swf` through `ImportAssets`.

**M48 Phase 3 (#6665, `crates/ui/src/fonts.rs`, `translate.rs`):**

- **Font classes (GUI-FONT-001).** A Skyrim movie imports its fonts by class
  name from `gfxfontlib.swf`; the shipped one is a placeholder (it lacks
  `$EverywhereMediumFont`, which `hudmenu.swf` imports), and GFx fills the import
  from `fontconfig.txt` instead. The navigator serves a library built from it:
  each `map "$Class" = "Face" Style` exported under its class, pointing at the
  face of that style copied out of the `fontlib` SWFs. All 20 vanilla Skyrim
  classes resolve (`installed_skyrim_font_configuration_builds_the_classes_the_hud_imports`).
  Fallout 4's AVM2 movies name classes on their text fields (`$MAIN_Font_Bold`)
  rather than importing them; mapping those is Phase 6.
- **Language (GUI-FONT-002).** `fontconfig_<language>.txt` when the archive ships
  one for the game-text language (`BYRO_STRINGS_LANG`, the `.STRINGS` loader's
  token; `set_text_languages`), else `fontconfig.txt`.
- **Fallback (GUI-FONT-003).** Noto Sans Regular and Bold (SIL OFL 1.1, Q5,
  `assets/fonts/noto-sans/` with `OFL.txt`) are every player's device font and
  default sans / serif / typewriter. Ruffle lays a span whose embedded face it
  cannot find out in the default sans, so an unmapped class, a missing face or a
  missing glyph falls back to it; each unresolved class is reported once. Before
  this, Fallout 4 HUD text did not draw at all.
- **`$` keys (GUI-FONT-004).** GFx shows a translation-table entry for text that
  is a `$` key; Ruffle has no such pass. The workspace patches `ruffle_core` (vendored
  at the pinned revision, `third_party/ruffle/PATCHES.md`): `EditText` creation,
  `text` and `htmlText` pass through `byro_translation::translate` (whole text,
  or each run between HTML tags), and `install_translations` gives it the
  language's `interface\translate_<language>.txt` on the first movie load.
- **Unfed HUD widgets.** With fonts resolving, widgets native code fills and
  hides showed their authored sample text (Skyrim's "Dialogue Line 1 / 2",
  Fallout 4's quest-update, XP, region, ammo, crit, stealth and enemy-health
  widgets). `ScaleformGame::unfed_widgets` stands in once: Skyrim's own
  `HideSubtitle()`, Fallout 4 `visible = false` on each until its feed lands.

### Menu pack

Once the four pieces above land, the Skyrim ~34 menus (and FO4's larger
set, including the Pip-Boy) should load with minimal per-menu work.

See the [Creation Engine UI](../legacy/creation-engine-ui.md) legacy doc
for the menu catalog and the [Papyrus API Reference](../legacy/papyrus-api-reference.md)
notes for the format-string system menus rely on.

## Tests

The UI crate has **118 default tests plus 3 ignored** installed-corpus smokes
(measured 2026-10-31 with `cargo test -p byroredux-ui`; the two extra over the
2026-10-17 count of 116 are the #6225 and #5480 pins. The 13 extra over the
2026-10-12 count of 103 were the #5719 ceiling pins, the #5732 callback /
empty-`ImportAssets` pins, and the #5966/#5967 pins: the declared-length
ceiling on every decode site, and the measured `SwfDecodeCounts` over real
CWS movies). #5274's
provenance rework — the lib target's own summary. `-- --list` includes the
ignored tests in its count, which is how this line's predecessor
double-counted them as "101 default plus 3 ignored". Re-measure rather than
trusting this line — it has drifted repeatedly: #4721 re-measured it after
the count had drifted six times, most recently through the #3103 corpus-sweep
additions and #4720's four-argument scanner tests).
The executable adds
winit-translation tests in `byroredux/src/ui_input.rs`. The synthetic
non-Bethesda SWFs come from Ruffle's pinned ExternalInterface fixtures:

- The `byroredux-ui` crate compiles as part of the workspace.
- Real headless AVM1 and AVM2 movies receive representative focus, pointer,
  key, and text-control events before verifying ActionScript → host calls,
  callback discovery, and host → ActionScript invocation through Ruffle's
  null renderer.
- Unit coverage pins profile detection, the 142-method sorted catalog,
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
frame beside `ui.tick(dt)` (`byroredux/src/app_frame.rs`), which is what keeps the
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

## MenuXml legacy UI (M48.4 Oblivion / M48.5 FO3)

The pre-Skyrim games (Oblivion, FO3/FNV) do not use Scaleform — their
menus are Bethesda's XML UI ("MenuXml"), evaluated per frame by a FOLD
trait language. `byroredux-menuxml` carries that track: the M48.4
Oblivion HUD first, then M48.5 generalized the per-game facts behind
`MenuProfile` and added FO3 as the second consumer (FNV rides the same
profile with a ninth font slot).

**Crate layout** (`crates/menuxml/`):

- `parse.rs` — tolerant scanner. Vanilla XML is not well-formed:
  `</text-->` comment tails overlap tags, `<onlynotif>` is a shipped
  typo, prefabs arrive via `<include>`. `Document` keeps a name index
  for override targeting.
- `eval.rs` — the trait FOLD: `copy/add/sub/mul/div/mod/min/max/and/or/
  eq/neq/lt/lte/gt/gte/onlyif/not/ceil/floor/abs` with working-value +
  argument chains, `&true;` = 2, comparisons yield 2/0, switch-case
  `copy` of a `_name_` string appends the numeric working value
  (`_animation_` + 23 → `_animation_23`). Selectors: `me()`, `parent()`,
  `sibling()`, `child()`, `screen()`, `strings()`, `globals()` (FO3/FNV
  `menus\globals.xml`, e.g. `_background_fill_alpha`; #6666), bare tile names.
  Only `visible` defaults true; `alpha/red/green/blue` default 255. A text
  tile's `width` / `height` are the size of its displayed text (the native
  code sets them; documents read them to centre and frame text): the
  renderer evaluates once, measures each visible text with the raster's
  wrap, and evaluates again with those sizes (#6666).
- `layout.rs` — locus-chain positioning, depth sort (document order
  tiebreak), `clipwindow` scissor accumulation. `visible` propagates: a
  hidden tile hides its whole subtree, hit targets included (#6666; CS
  wiki: "unlike visible, clips does not propagate"). Rect tiles never paint
  (vanilla's screen-wide grab zones are invisible hit targets). The
  depth sort uses `f32::total_cmp` (#4716): `<depth>NaN</depth>` parses,
  and the old `partial_cmp().unwrap_or(Equal)` comparator is not a total
  order, which `sort_by` panics on. `NaN` sorts last (draws on top); `-0`
  is folded to `0` so it still ties by document order.
- `raster.rs` — CPU source-over rasterizer. The image blit implements
  the CS-Wiki zoom contract: **default (unauthored) zoom draws texels
  1:1 clipped to the tile rect** — the ribbon art is power-of-2 padded
  (256 px wide, ~164 px of ink) and the authored bar width is a fill
  *fraction* of the ink width, so stretching-as-default rendered a
  constant ~63%-width health bar at every value. `zoom -1` (`&scale;`)
  stretches to the tile; `cropx/cropy` apply post-zoom in display
  pixels. M48.5 adds the FO3-era `<tile>` repeat mode: 1:1 texel
  tiling across the tile rect with `cropx` as a *wrapping* scroll
  offset — the tick-mark meters and the seamlessly scrolling
  compass strip both ride it. **Menu XML is untrusted input** (#4715):
  the pixel loops of `blit` and `blit_sub` (the glyph path) are clamped
  to the framebuffer, the outermost clip, so a `16000 x 16000` tile costs
  at most one blend per framebuffer pixel instead of 256M iterations per
  HUD render; non-finite geometry (`inf`, `NaN`, or `1e39`, which
  `str::parse::<f32>` turns into `inf`) draws nothing instead of
  saturating the loop bound to `i64::MAX`; and the tiled texel-index math
  wraps rather than overflowing on a saturated `as i64`.
- `tex.rs` / `font.rs` — own DDS decoder (BC1/BC2/BC3 + masked
  uncompressed; the `image` crate rejects some vanilla headers) and the
  `.fnt` + `.tex` bitmap-font pair (296-byte header, 256×56-byte glyph
  records, white-on-transparent atlas tinted at draw time). A record ends
  `width, height, kerningLeft, kerningRight, ascent`: the pen advances
  `kerningLeft + width + kerningRight` and a glyph's top sits
  `point size − ascent` below the line's top (#6666, matching OpenMW's
  reader; the corpus tests pin that letters share a baseline).
- `menu.rs` — `MenuRenderer`: loads a menu + the game's strings
  document + font table (per `MenuProfile`), resolves menu art
  across the three shipped resolution sets (`textures\Menus`,
  `Menus80`, `Menus50` by UI width — an Oblivion-only convention;
  FO3's `Interface\…` paths resolve through the `textures\`
  passthrough arm), applies `(tile, trait)` overrides, renders
  frames. M48.5 adds the runtime menu API the source engine had
  from C++: `instantiate_template` (clone a `<template>`
  prototype's content under a named tile) and `graft_fragment`
  (wrap a rootless prefab like the ops-driven
  `menus\prefabs\meter.xml` as a named child rect, so
  `parent()`/`sibling()` ops resolve against the wrapper).
- `prepare.rs` (Scaleform profiles) — `normalize_scaleform_dialect`
  (#4470, #5456, #5479): the choke point every root and `ImportAssets`
  movie passes through before Ruffle sees it (the resource navigator shims
  every SWF body it serves, including a movie a running menu loads through
  `Loader.load` / `loadClip`, #5480; a well-formed CWS dependency is served
  inflated as FWS, so `ScaleformResourceLoad::byte_len` is the post-shim
  length), owning the vanilla dialect patches
  (the #5456/#5479 PlaceObject3 family) on the uncompressed/freshly
  inflated stream; conforming FWS/ZWS containers return `None` and take
  the stock path. Whatever this shim accepts is the dialect contract —
  its pins are the dialect-shim ui tests. The stock path honours the
  container header's own `u32` length, so the shim's private inflate cap
  (#5719) is not a bound on its own: `ensure_declared_len_within_ceiling`
  (#5966, `MAX_SWF_BODY_BYTES` = 128 MiB over FWS/CWS/ZWS claims) gates
  `prepare_movie`, the navigator's dependency decodes,
  `ScaleformProfile::detect` and both host-call scanners before any decode,
  and the navigator's fetch holds every SWF body it serves to it (#6225) —
  the second choke point, since Ruffle decodes a runtime-loaded movie itself,
  and a source-shape test fails on a new `decompress_swf(` call without it.
  `SwfDecodeCounts::decompresses` (#5967) is measured: a CWS menu open
  reads 1, an uncompressed movie 0, a shim that inflates and then declines
  reads 2.
- `menu.rs` (MenuXml) — `MAX_DECODED_TEXTURE_BYTES` (#5658): the
  1 GiB budget `MenuRenderer` holds decoded menu textures under, sized
  from the vanilla census (979.9 MiB total for Oblivion's 6,111 files
  across all three resolution sets — a renderer only decodes the
  handful its document names, so the budget cannot refuse vanilla art),
  one decode per resolved archive path shared by the
  `Menus\`/`Menus80\`/`textures\menus\` spellings.
- `profile.rs` — `MenuProfile`: the per-game corpus facts in one
  place. Oblivion: 5 `[Fonts]` slots in the Misc BSA, atlas
  `fonts\<name>.tex`, strings from `menus\strings.xml`. FO3/FNV:
  8/9 slots in the *texture* BSA under `textures\fonts\`, atlas
  `.tex` or `.dds` beside each `.fnt` (binary layout identical —
  14632 B, atlas name at offset 12), no strings document
  (`falloutdict.txt` is the hacking dictionary; FO3's `-sPrefixed`
  strings are GMST refs, future work).

**Engine integration** (`byroredux/src/hud.rs`): `--hud` discovers
the game from the corpus itself — whichever vanilla Misc BSA sits
beside `--esm` (`Fallout - Misc.bsa` vs `Oblivion - Misc.bsa`;
FO3 vs FNV decided by the load order, #4718: a base ESM named in
`--master`/`--esm`, else the one beside the corpus, else FO3) selects an
`HudGameProfile`:
archive names, `MenuProfile`, bar count/labels/AVIF keys, and the
assembly style. **Oblivion** is *authored* — the XML ships the
bar/compass art and the driver only pushes `hudmain_*`
overrides. **FO3/FNV** are *assembled* — `hud_main_menu.xml`
ships empty container rects and the launch path mirrors the
source engine's C++ assembly: `meter.xml` grafted under
`HitPoints`/`ActionPoints` (anchored bottom-left/-right),
`template_compass_window` instantiated at the root, then driven
per frame via `_Value` (0..1 fill) and `cropx` (heading × strip
texels/degree). FO3 bar values resolve the canonical AVIF editor
ids (`Health` / `ActionPoints`) through `PlayerVitals` each load —
#4675: the original literals here (`0x2C9` / `0x2D0`) were
`fallout.rs`'s unit-test fixture ids, not real AVIFs, so no
real-content entity carried them and the bars always drew full. The driver
registers **three**
overlay textures, and rotates uploads across them
(`write_rgba_inplace` queues mip-0 copies through the same frame submission
path as `update_rgba`, with no image/view/descriptor churn). A change
signature (bar bits + heading×10 + visibility) plus a 33 ms cadence cap
keeps a static HUD at zero raster cost. Bar values come from pinned
console values or the PLAYER's `ActorValues` (resolved through
`PlayerVitals`, #4675); the playable slice stamps the player from the
base `NPC_` with the PlayerOnly formulas evaluated (#4674). Console surface: `hud.on`, `hud.off`,
`hud.values <f0> <f1> [<f2>]` — one fraction per the game's bars, 2 for FO3 (or `auto`), `hud.heading <deg>`, `hud.status`;
`tex.dump <bsa-path> <texture-path> [out.png]` (in the `tex.*` family)
extracts and decodes any archive texture — including menu sets and font
atlases — to PNG for offline inspection.

**Debugging**: `BYRO_HUD_DUMP=1` writes each rendered frame's raw RGBA to
`/tmp/hud_engine_frame.rgba`; `RUST_LOG=byroredux::hud=debug` logs every
render's values and target buffer. The `render_hud` example
(`cargo run -p byroredux-menuxml --example render_hud`) renders reference
frames + trait probes against installed data, independent of the engine.

The smoke gate (`docs/smoke-tests/m48-4-oblivion-hud.sh`) pins
`hud.values 1.0` then `0.35`, captures both frames, and asserts the
health fill's contiguous strong-red run shrinks 158 → 53 px with a full
five-filter PNG decode. The classifier is deliberately strict — the
trough's cream ornamentation (254,221,161) passes an `r>g+30` filter and
merges with the fill into a constant run that hides regressions.

The M48.5 gate (`docs/smoke-tests/m48-5-fo3-hud.sh`) runs the FO3
fixture cell (Moriarty's Saloon — an interior, so the scene behind
the HUD is dark) and counts near-white tick columns
(`min(r,g,b) > 180`) in the HP band: 226 → 68 columns after a 30%
pin, plus a compass-band ink check (327 columns). Corpus facts and
the graft/instantiate render are pinned env-gated in
`crates/menuxml/tests/fo3_corpus.rs` (`BYROREDUX_FO3_DATA`). The
FO3 HUD art ships untinted (white ticks/strip) — the engine tints
via `systemcolor &hudmain;`, whose default constant is not yet
pinned from game settings, so M48.5 renders art-native colors.

## Scaleform HUD (M48.6 Skyrim / M48.7 Fallout 4)

`--hud` is the vanilla-HUD front door on every legacy-track game: the
launcher probes the Scaleform route first (`Skyrim - Interface.bsa` or
`Fallout4 - Interface.ba2` beside `--esm` → `byroredux/src/scaleform_hud.rs`,
dispatched over a `ScaleformGame` enum), then the MenuXml profiles; the
two are mutually exclusive per run, and `--menu` suppresses the Scaleform
HUD (a second `UiManager` construction would orphan its host-call
draining).

The Skyrim HUD is `interface\hudmenu.swf` through the Ruffle player, with
three HUD-grade fixes over the raw `--menu` route:

- **Transparent stage** — `SwfPlayer::set_stage_transparent` flips
  Ruffle's `WindowMode::Transparent`, whose clear is alpha-0 instead of
  the movie's background color. Without it the full-screen overlay
  texture carries an opaque stage clear and the rendered world is
  invisible behind the HUD. `--menu` keeps the opaque stage (modal menus
  own the screen by design).
- **World input stays live** — `set_input_owner(None)` at launch, and the
  HUD's registry flags claim no input (GUI-REG-009); a HUD is not a modal menu.
- **`hud.off` hides** — the driver mirrors `HudControl.visible` into the
  player each frame; `render()` answers `UiFrame::Hidden` and the UI
  quad stops.

The driver shares `HudControl` with the MenuXml track (Skyrim labels:
health/magicka/stamina; `hud.status` gained a trailing
`backend=menuxml|scaleform`), computes bar fractions from the same
pinned-or-`ActorValues` semantics, and runs the same change-signature +
33 ms cadence discipline. Bridge diagnostics reach the console through
the `ScaleformHudDiag` resource mirror (`hud.debug`): the `Rc`-based
bridge cannot live in the (Send-bounded) ECS, so the driver copies the
registered-callback / unknown-method / unanswered-method sets out at 1 Hz
alongside the last computed frame.

**Protocol reality** (pinned by `crates/ui/tests/hudmenu_protocol.rs` and
the `hud.debug` smoke gate): vanilla hudmenu is passive chrome. Its
bytecode calls exactly four host methods — `GetButtonFromUserEvent`,
`PlaySound`, `RegisterHUDComponents`, `myLog` — and it registers only the
GameDelegate pair `call`/`respond` on ExternalInterface. The vanilla
engine feeds the meters by invoking movie members by path, which Ruffle's
ExternalInterface does not expose by itself. **Since M48 Phase 2 (#6661) the
meters are live:** the injected path adapters reach those members, and the driver
feeds them every changed frame through the path service
(`ScaleformGame::meter_feeds`; see
[Path access](#path-access-gui-path-001005-decision-q2--built-in-phase-2-6661)):
Skyrim's `Set{Health,Magicka,Stamina}MeterPercent` (0..100, `abForce = false`) and
`SetCompassAngle`, Fallout 4's HP and AP `MeterBarWidget.Percent` (0..1) with the
rads overlay cleared, and Fallout 4's compass direction markers (#7036, attached and
placed by `HUDCompass`'s rule; see the path-access notes above). The movie's own `call` callback does not reach those
handlers (measured in Phase 2; Phase 0 had read it from bytecode). The SkyUI-class
poll handlers (`updateStats`, `RequestPlayerInfo`) and the push table stay for
menus that speak that protocol. `hud.debug` prints each feed path's last outcome
and the queued count. While an original HUD is up, the native layer draws no
second set of vitals bars (GUI-HUD-004). Gates: `m48-skyrim-hud-meters.sh` (A4),
`m48-fo4-hud-meters.sh` (A5), `m48-fo4-hud-compass.sh` (#7036).

The smoke gate (`docs/smoke-tests/m48-6-skyrim-hud.sh`) gates the boot
line, `backend=scaleform` + 3-bar pins, the `hud.debug` callback pair +
driver liveness, and a chrome on/off pixel **diff**: `hud.off` between
two captures makes the world (static across captures) cancel out, so the
compass chrome is measured without a brightness assumption — the
transparent stage lets the world legitimately contribute brightness.

**M48.7 — Fallout 4 (AVM2):** the same driver serves
`interface\hudmenu.swf` out of `Fallout4 - Interface.ba2` through the
injected BGSCodeObj forwarding adapter — the `hud: loaded` line now
carries `profile=` and `state=` (FO4 greps
`state=Some(AdapterInjected)`). FO4 drives 2 bars (health / ap —
canonical editor ids resolved per load through `PlayerVitals`; the
original literals `0x2C9` / `0x2D0` were fallout.rs fixture ids, not
FO4's real `Health 0x2D4` / `ActionPoints 0x2D5`, #4675); the SkyUI
poll handlers are Skyrim-gated (FO4's
BGSCodeObj catalog has no meter-shaped queries and none are fabricated),
and the push table skips the adapter's `__byro*` lifecycle hooks — which
`hud.debug` mirrors as the runtime AdapterInjected observable. Protocol
pinned by `crates/ui/tests/fallout4_hudmenu_protocol.rs`: vanilla FO4
hudmenu is `AdapterInjected`, registers the lifecycle hooks
(`__byroBGSCodeObjReady` answers `Bool(true)`), acknowledges destruction
exactly once on drop (it declares `onCodeObjDestruction`), and boots
with `unknown_methods()` empty — zero gameplay host calls while idle,
so meters stay engine-empty exactly as on Skyrim. Smoke
`docs/smoke-tests/m48-7-fo4-hud.sh` runs the MedTekResearch01 fixture
and diffs chrome on/off over the health-bar and compass bands.

## Related docs

- [Creation Engine UI](../legacy/creation-engine-ui.md) — Bethesda menu
  catalog, Scaleform extensions, the GFx interpreter contract
- [Papyrus API Reference](../legacy/papyrus-api-reference.md) — what UI
  events a menu can receive, what the script side needs to expose
- [Vulkan Renderer](renderer.md) — the overlay pipeline setup, bindless
  texture array, and `update_rgba` deferred-destruction path
- [Game Loop](game-loop.md) — where the inline UI tick/render fits in the
  per-frame flow
