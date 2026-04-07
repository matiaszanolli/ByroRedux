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

## At a glance

| | |
|---|---|
| Crate                  | `byroredux-ui` |
| External engine        | Ruffle (Flash / ActionScript 1, 2, 3) |
| Render path            | Ruffle → wgpu offscreen → CPU pixel buffer → Vulkan texture upload → fullscreen quad |
| Status                 | Loose SWF demo working (`--swf path.swf`); cargo run shows the menu as a fullscreen overlay |
| Tested SWFs            | Skyrim SE `fadermenu.swf`, `loadingmenu.swf`, `messagebox.swf` (all AS2 / Flash v15) |
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
fader / loading / messagebox menus all render with **zero GFx stubs
implemented yet**.

## Module map

```
crates/ui/src/
├── lib.rs       UiManager — top-level resource type, SWF loader, frame tick wrapper
└── player.rs    SwfPlayer — Ruffle wrapper, offscreen wgpu rendering, RGBA pixel readback
```

## Pipeline

```
SWF file bytes
        │
        ▼  ruffle_core::Player::load_swf
ruffle_core::Player (advances frames, runs ActionScript)
        │
        ▼  ruffle_render_wgpu::backend::WgpuRenderBackend
offscreen wgpu render-target texture (RGBA8)
        │
        ▼  Player::render() → readback into a CPU Vec<u8>
RGBA pixel buffer (one per UI frame)
        │
        ▼  byroredux::main → ctx.update_ui_texture(rgba)
Vulkan VkImage uploaded as a sampled texture, descriptor set built
        │
        ▼  draw_frame: bind UI pipeline (no depth, alpha blend, passthrough shaders)
        ▼  Draw fullscreen quad with the UI texture sampled
Pixels on screen
```

The trick is the **CPU bridge** between Ruffle's wgpu backend and our
Vulkan renderer. Ruffle is built around wgpu and renders to a wgpu
texture; we don't share GPU contexts between Ruffle's wgpu and our ash
Vulkan, so we read pixels back to the CPU and re-upload to a Vulkan
texture. This costs one round-trip per UI frame but it's bounded by the
SWF resolution (typically 1920×1080) and works without coupling the two
backends.

The whole UI plane is **one fullscreen quad** in the renderer with one
texture binding. Multiple menus stack inside Ruffle (main menu → submenu
→ messagebox) — that's the SWF runtime's job, not Vulkan's.

## SwfPlayer API

```rust
pub struct SwfPlayer {
    player: Arc<Mutex<ruffle_core::Player>>,
    renderer_backend: WgpuRenderBackend,
    width: u32,
    height: u32,
}

impl SwfPlayer {
    pub fn from_bytes(swf_data: &[u8], width: u32, height: u32) -> Result<Self>;
    pub fn tick(&mut self, dt: Duration);
    pub fn render(&mut self) -> Vec<u8>;       // RGBA8 pixel buffer
    pub fn movie_size(&self) -> (u32, u32);
}
```

`tick(dt)` advances Ruffle's clock and runs any ActionScript that wants
to fire (timers, frame scripts, button handlers). `render()` rasterises
the current frame to the wgpu render target and reads it back to CPU.
The width/height are the renderer-side surface dimensions, **not** the
SWF's native size — Ruffle scales internally.

## UiManager (resource)

```rust
pub struct UiManager {
    pub players: Vec<SwfPlayer>,
    pub framebuffer: Option<Vec<u8>>,
}
```

Held as a `World` resource so the UI tick system can advance every
loaded SWF on the per-frame schedule, then mark the framebuffer dirty so
the renderer picks it up on the next draw.

In the current loose-SWF demo (`--swf path.swf`), there's exactly one
player with the full window size; future Bethesda menu integration will
register multiple players (one per active menu) and composite them
within Ruffle.

## Vulkan integration

The renderer side has a dedicated **UI pipeline** with:

- No depth test / no depth write (UI draws on top of the world)
- Alpha blend enabled (`SRC_ALPHA`, `ONE_MINUS_SRC_ALPHA`)
- Passthrough vertex shader (just a quad)
- Single sampler binding for the UI texture
- Drawn after the main scene render pass, in the same render pass as a
  separate subpass — no extra binding cost

The UI texture is part of the standard `texture_registry` but with a
**deferred destruction** path: when Ruffle finishes a frame and we upload
the new RGBA buffer, the old `VkImage` isn't destroyed immediately —
it's queued behind two frames worth of fences before actually being
freed. This is the "deferred texture destruction" line item in the
renderer feature list and it's **what makes dynamic UI texture updates
stall-free**. Without it, every UI frame would need a `device_wait_idle`
to know the previous frame had finished sampling the old texture before
freeing it.

## SWF demo

```bash
cargo run -- --swf path/to/menu.swf
```

This:

1. Reads the SWF file
2. Creates a `SwfPlayer` sized to the window
3. Inserts it into the `UiManager` resource
4. Per frame:
   a. `swf_player.tick(dt)`
   b. `let pixels = swf_player.render()`
   c. `ctx.update_ui_texture(&pixels)`
5. Renderer draws the standard scene (or a black background if no scene
   was loaded), then the UI quad on top

Tested SWFs from Skyrim SE:

| File | Type | Result |
|---|---|---|
| `fadermenu.swf` | Black-fader transition | Renders correctly |
| `loadingmenu.swf` | Loading screen with art | Renders correctly |
| `messagebox.swf` | Modal text box | Renders correctly |

All three are AS2 / Flash v15. They use a few Scaleform-specific globals
under `_global.gfx.*` that Ruffle treats as unknown — those calls are
no-ops in Ruffle's engine, so any **rendering-only** Scaleform extensions
just don't happen, but the rest of the menu draws fine.

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
ECS events ↔ Ruffle's ActionScript event system.

### Input routing

When a menu is open, mouse/keyboard go to the menu, not the world. The
game loop already has an `Input` resource and a fly-camera system; we
need a "UI focus" resource that toggles input routing depending on
whether any modal menu is active.

### Font loading

Bethesda ships custom fonts (`fontlib_loader.swf`) that the menus load at
startup. Ruffle has a font subsystem; we need to feed the FNV / Skyrim
font assets through it.

### Menu pack

Once the four pieces above land, the Skyrim ~34 menus (and FO4's larger
set, including the Pip-Boy) should load with minimal per-menu work.

See the [Creation Engine UI](../legacy/creation-engine-ui.md) legacy doc
for the menu catalog and the [Text Replacement & Markup](../legacy/papyrus-api-reference.md)
notes for the format-string system menus rely on.

## Tests

UI tests are minimal at this stage — Ruffle has its own extensive test
suite, and our integration is mostly thin glue. The tests we have:

- Compilation smoke test that the `byroredux-ui` crate builds
- Manual: `cargo run -- --swf path.swf` for each tested menu

End-to-end UI testing is gated on the `_global.gfx` stub work; once a
menu touches Bethesda-specific globals, we need a way to assert that
those globals returned the expected values.

## Related docs

- [Creation Engine UI](../legacy/creation-engine-ui.md) — Bethesda menu
  catalog, Scaleform extensions, the GFx interpreter contract
- [Papyrus API Reference](../legacy/papyrus-api-reference.md) — what UI
  events a menu can receive, what the script side needs to expose
- [Vulkan Renderer — Texture upload](renderer.md#texture-upload) — the
  `update_ui_texture` path and deferred destruction
- [Game Loop](game-loop.md) — where the UI tick fits in the per-frame
  schedule
