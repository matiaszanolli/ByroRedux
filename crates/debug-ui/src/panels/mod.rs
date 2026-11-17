//! Concrete debug-UI panels (Phase 4b of the debug-UI plan).
//!
//! Drawn inside the egui closure passed to `DebugUiState::run` by the
//! binary's main loop. Each panel reads from a pre-built
//! [`PanelSnapshot`] (a frozen view of the World resources the
//! panels need) and writes any actions back through
//! [`PanelOutputs`]; the binary applies those to the World after
//! `run` returns, sidestepping the borrow-checker conflict between
//! `&mut DebugUiState` and the world references the panels would
//! otherwise need.
//!
//! Split by job (#6578): `snapshots` holds the overlay's snapshot (with the player's state nested
//! as `game`) and `studio` is the developer overlay. The HUD and the native pages are
//! `byroredux_native_ui`'s (GUI-PAGE-006); this crate hosts the egui frame they draw into.

mod snapshots;
mod studio;

pub use snapshots::{PanelOutputs, PanelSnapshot, QueuedLoad};
pub use studio::{draw, PanelTab};
