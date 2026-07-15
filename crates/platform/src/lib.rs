//! Windowing and platform abstraction for ByroRedux.
//!
//! Thin wrapper around `winit` — window creation and raw display/window
//! handles for the Vulkan surface (`renderer` crate). Deliberately small:
//! platform-specific behavior belongs here so `renderer` and `byroredux`
//! stay windowing-toolkit-agnostic.

pub mod window;
