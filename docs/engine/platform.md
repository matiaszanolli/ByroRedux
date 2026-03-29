# Platform Abstraction

The platform crate provides OS-level abstractions. Currently focused on
windowing via winit. Linux-first, with the abstraction layer in place for
future multiplatform support.

Source: `crates/platform/src/`

## Window Management

### WindowConfig

```rust
pub struct WindowConfig {
    pub title: String,     // default: "ByroRedux"
    pub width: u32,        // default: 1280
    pub height: u32,       // default: 720
}
```

### Functions

```rust
// Create a winit window from an active event loop
pub fn create_window(
    event_loop: &ActiveEventLoop,
    config: &WindowConfig,
) -> Result<Window>

// Extract raw handles for Vulkan surface creation
pub fn raw_handles(
    window: &Window,
) -> Result<(RawDisplayHandle, RawWindowHandle)>
```

## Dependencies

| Crate | Purpose |
|-------|---------|
| winit 0.30 | Cross-platform windowing |
| raw-window-handle 0.6 | Platform-agnostic handle traits |

## Future Expansion

- Input handling (keyboard, mouse, gamepad)
- Fullscreen/monitor management
- Cursor control
- Platform-specific filesystem paths
- OS integration (clipboard, drag-and-drop)
