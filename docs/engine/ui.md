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

> Status note (as of Session 42 / 2026-05-28): the UI subsystem is still
> **Phase-1 infrastructure**. The Ruffle integration glue in `crates/ui/`
> has not changed structurally since it landed (the crate is two files —
> `lib.rs` + `player.rs`). Everything that *has* moved since the doc was
> first written is on the **renderer side**: the UI overlay was migrated
> onto the bindless texture array and a dedicated lightweight vertex
> format, and the draw-time pipeline-state invariant was codified. Those
> changes are folded into the relevant sections below.

## At a glance

| | |
|---|---|
| Crate                  | `byroredux-ui` |
| External engine        | Ruffle (Flash / ActionScript 1, 2, 3), git-pinned (see `crates/ui/Cargo.toml`) |
| Ruffle render backend  | `ruffle_render_wgpu` on its **own** wgpu/Vulkan device (separate from the engine's `ash` Vulkan) |
| Render path            | Ruffle → wgpu offscreen `TextureTarget` → `capture_frame()` CPU RGBA → Vulkan texture upload → fullscreen quad |
| Lifetime               | `UiManager` is **not** an ECS resource — Ruffle's `Player` is not `Send + Sync`; it lives in the main loop alongside `VulkanContext` |
| Status                 | Loose SWF demo working (`--swf path.swf`); cargo run shows the menu as a fullscreen overlay |
| Pending                | Scaleform GFx stubs (`_global.gfx`), Papyrus↔UI bridge, input routing, font loading, full menu pack |

## Why Ruffle?

The legacy Bethesda menus run on Scaleform GFx 4.x / 5.x, which is
proprietary middleware Adobe acquired and Autodesk later sunset. There is
no open-source Scaleform runtime. We have three options:

1. **Reverse-engineer Scaleform GFx** — months of work and a permanent
   drag-along of legacy ActionScript dialect quirks
2. **Reimplement every menu in a modern UI library** (egui, imgui, ...)
   — fastest start, but throws away every modder's existing SWF mods
3. **Use Ruffle** — open-source Flash player written in Rust, already
   handles AS1/2/3, only needs Bethesda's GFx-specific globals stubbed
   to render most menus

Option 3 is what M20 picked. The bet is that "Bethesda menu" ≈ "Flash file
that uses a small set of Scaleform extensions" and that those extensions
can be stubbed in a few hundred lines of glue. So far that bet is holding:
simple AS2 menus render with **zero GFx stubs implemented yet** — calls
into Bethesda's `_global.gfx.*` namespace that Ruffle doesn't recognise
are simply not present, so any rendering-only Scaleform extensions are
absent while the rest of the menu draws.

## Module map

```
crates/ui/src/
├── lib.rs       UiManager — top-level handle: owns the active SwfPlayer,
│                visibility/menu-name/viewport state, load/tick/render/close
└── player.rs    SwfPlayer — Ruffle wrapper, own wgpu/Vulkan device,
                 offscreen TextureTarget, capture_frame() → cached RGBA buffer
```

Both types are re-exported from `byroredux_ui` (`pub use player::SwfPlayer;`,
`UiManager` defined directly in `lib.rs`).

## Pipeline

```
SWF file bytes
        │
        ▼  ruffle_core::tag_utils::SwfMovie::from_data
parsed SwfMovie
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
}

impl SwfPlayer {
    pub fn new(swf_data: &[u8], width: u32, height: u32) -> anyhow::Result<Self>;
    pub fn tick(&mut self, dt: f64);          // seconds; wrapped in FloatDuration internally
    pub fn render(&mut self) -> Option<&[u8]>; // borrows pixel_buffer; None if not dirty
    pub fn dimensions(&self) -> (u32, u32);
}
```

`new()` parses the SWF (`SwfMovie::from_data`), spins up a headless
wgpu/Vulkan device, builds an offscreen `TextureTarget` of the requested
size, wires it into a `WgpuRenderBackend`, attaches a software video
backend (`ruffle_video_software`), and starts playback
(`set_is_playing(true)`).

`tick(dt)` advances Ruffle's clock (`Player::tick(FloatDuration::from_secs(dt))`)
and runs any ActionScript that wants to fire (timers, frame scripts,
button handlers), then marks the player **dirty**.

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
    pub menu_name: String,      // e.g. the SWF path / "startmenu"
    pub width: u32,
    pub height: u32,
}

impl UiManager {
    pub fn new(width: u32, height: u32) -> Self;
    pub fn load_swf(&mut self, swf_data: &[u8], name: &str) -> anyhow::Result<()>;
    pub fn tick(&mut self, dt: f64);             // forwards to the active player when visible
    pub fn render(&mut self) -> Option<&[u8]>;   // None when hidden or no player
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

### `_global.gfx` Scaleform stubs

Bethesda menus call into a small set of Scaleform-specific globals for
layout, locale, and texture loading. We need to install AS-side stubs
inside Ruffle's player so calls like `gfx.io.GameDelegate.call(...)`
return sensible defaults instead of being silently dropped.

### Papyrus ↔ UI bridge

In Bethesda's runtime, scripts (Papyrus / ECS-native) communicate with
menus via a queue of "UI events". A pause menu might receive
`OnButtonPress("Resume")` and the script handles it. We need to wire
ECS events ↔ Ruffle's ActionScript event system. (The engine-side event
plumbing now exists — see the event/condition runtime, M47.0/M47.1 — so
this is a matter of connecting it to Ruffle, not building it from zero.)

### Input routing

When a menu is open, mouse/keyboard go to the menu, not the world. The
game loop already has an input path and a fly-camera system; we need a
"UI focus" concept that toggles input routing depending on whether any
modal menu is active. Today `UiManager::visible` gates tick/render but
does not yet capture input away from the world.

### Font loading

Bethesda ships custom fonts (`fontlib_loader.swf`) that the menus load at
startup. Ruffle has a font subsystem; we need to feed the FNV / Skyrim
font assets through it.

### Menu pack

Once the four pieces above land, the Skyrim ~34 menus (and FO4's larger
set, including the Pip-Boy) should load with minimal per-menu work.

See the [Creation Engine UI](../legacy/creation-engine-ui.md) legacy doc
for the menu catalog and the [Papyrus API Reference](../legacy/papyrus-api-reference.md)
notes for the format-string system menus rely on.

## Tests

UI tests are minimal at this stage — Ruffle has its own extensive test
suite, and our `crates/ui/` integration is thin glue with no dedicated
unit tests. What guards the integration today:

- The `byroredux-ui` crate compiles as part of the workspace.
- The renderer-side UI contract is covered by tests, not the Ruffle glue:
  `UiVertex` size/offsets (`crates/renderer/src/vertex.rs`), the bindless
  layout match between `triangle.frag` and `ui.frag`
  (`texture_registry_bindless_tests.rs`), and the `ui.vert` GpuInstance /
  `textureIndex` contract + descriptor reflection
  (`crates/renderer/src/vulkan/scene_buffer/`).
- Manual: `cargo run -- --swf path.swf` for each tested menu.

End-to-end UI testing is gated on the `_global.gfx` stub work; once a
menu touches Bethesda-specific globals, we need a way to assert that
those globals returned the expected values.

## Related docs

- [Creation Engine UI](../legacy/creation-engine-ui.md) — Bethesda menu
  catalog, Scaleform extensions, the GFx interpreter contract
- [Papyrus API Reference](../legacy/papyrus-api-reference.md) — what UI
  events a menu can receive, what the script side needs to expose
- [Vulkan Renderer](renderer.md) — the `pipeline_ui` setup, bindless
  texture array, and `update_rgba` deferred-destruction path
- [Game Loop](game-loop.md) — where the inline UI tick/render fits in the
  per-frame flow
