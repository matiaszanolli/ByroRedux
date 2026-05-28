# Platform Abstraction

The `byroredux-platform` crate provides OS-level abstractions. It is currently
a thin layer over winit windowing plus raw-handle extraction for Vulkan surface
creation. Linux-first (X11/Wayland via winit), with the abstraction boundary in
place for future multiplatform work.

Source: [`crates/platform/src/`](../../crates/platform/src/) — two files only
(`lib.rs` re-exports the `window` module; `window.rs` holds everything below).
As of Session 42 (2026-05-28) the crate is unchanged since it was first written:
no commit has touched `crates/platform/` since the 2026-03-29 engine rename, so
the surface area here is genuinely small and stable.

## Window Management

All public API lives in [`crates/platform/src/window.rs`](../../crates/platform/src/window.rs).

### WindowConfig

```rust
pub struct WindowConfig {
    pub title: String,     // default: "ByroRedux"
    pub width: u32,        // default: 1280
    pub height: u32,       // default: 720
}
```

`WindowConfig` implements `Default` (the values shown above). The window is
created with a `LogicalSize` inner size, so the requested width/height are in
logical (DPI-independent) units.

### Functions

```rust
// Create a winit window from an active event loop.
// Logs "Window created: {w}x{h} \"{title}\"" on success.
pub fn create_window(
    event_loop: &ActiveEventLoop,
    config: &WindowConfig,
) -> anyhow::Result<Window>

// Extract raw handles for Vulkan surface creation.
pub fn raw_handles(
    window: &Window,
) -> anyhow::Result<(RawDisplayHandle, RawWindowHandle)>
```

`create_window` builds `WindowAttributes` (title + logical inner size) and calls
`ActiveEventLoop::create_window`. `raw_handles` pulls the display and window
handles via the `raw-window-handle` 0.6 `HasDisplayHandle` / `HasWindowHandle`
traits and returns them as the raw enum types.

## Integration

The platform crate does **not** own the event loop, the `ApplicationHandler`, or
input handling — those live in the binary. The flow:

1. [`byroredux/src/main.rs`](../../byroredux/src/main.rs) implements
   winit's `ApplicationHandler` on its `App` struct and drives the event loop.
2. In `App::resumed`, it builds a `WindowConfig::default()`, calls
   `window::create_window(event_loop, &config)`, then `window::raw_handles(&win)`.
3. The resulting `(RawDisplayHandle, RawWindowHandle)` pair plus the window's
   `inner_size()` are handed to `VulkanContext::new(display, window_handle, [w, h])`
   in the renderer to create the Vulkan surface and swapchain.

Input and window events are handled directly in the app's winit event loop, not
through the platform crate:

- `WindowEvent::KeyboardInput` / `CloseRequested` / `Resized` / `RedrawRequested`
  are matched in `App::window_event`.
- `DeviceEvent::MouseMotion` drives the fly camera in `App::device_event`.

So keyboard/mouse handling is implemented today; it simply isn't abstracted into
`byroredux-platform` yet (see Future Expansion below).

## Dependencies

Declared in [`crates/platform/Cargo.toml`](../../crates/platform/Cargo.toml):

| Crate | Version | Purpose |
|-------|---------|---------|
| winit | 0.30 | Cross-platform windowing + event loop types |
| raw-window-handle | 0.6 | Platform-agnostic display/window handle traits |
| log | workspace | `log::info!` on window creation |
| anyhow | workspace | `Result` error type on the public fns |
| byroredux-core | workspace | Declared, but currently unused by `window.rs` (no `core::` references); kept for future platform-side types |

Consumers in the workspace: the `byroredux` binary (window creation + raw
handles) and the `byroredux-renderer` crate (declares the dependency in its
`Cargo.toml`, though it does not `use` it directly today — the renderer receives
the raw handle types passed in from the binary).

## Future Expansion

These remain unimplemented in this crate as of Session 42 (2026-05-28). Keyboard
and mouse input already work, but live in the binary's event loop rather than
behind a platform abstraction:

- Input abstraction (lift keyboard / mouse / gamepad handling out of `main.rs`)
- Fullscreen / monitor management
- Cursor control
- Platform-specific filesystem paths
- OS integration (clipboard, drag-and-drop)
- Non-Linux backends (the winit dependency already enables them; nothing
  Linux-specific lives in this crate)
