//! M45 Save/Load — binary-side wiring for the [`byroredux_save`] crate.
//!
//! The save crate is engine-agnostic: it knows how to snapshot a
//! [`World`] given a [`SaveRegistry`], but only the binary sees every
//! component type, so the binary populates the registry
//! ([`build_save_registry`]) and installs it as a resource.
//!
//! ## What's wired
//!
//! - `save [slot]` — runs the pre-save validation pass, snapshots the
//!   live world, and writes a CRC-protected file atomically. Read-only
//!   against the World, so it's a plain [`ConsoleCommand`].
//! - `save.info <slot>` — decodes + verifies a slot (magic / version /
//!   CRC / schema fingerprint) and prints what it contains, without
//!   touching the live world.
//!
//! ## Live load (M45.1)
//!
//! `load <slot>` reloads the saved cell through the existing loader (full
//! GPU/physics/camera setup) and overlays the saved game-state deltas
//! keyed by stable form id (see [`execute_pending_save_loads`]). The
//! player/camera pose is restored on top of that: [`capture_player_pose`]
//! refreshes a [`PlayerPose`] resource each frame so a save records where
//! the player was standing and looking, and [`apply_player_pose`] re-places
//! the persisted player body (Character mode) or camera (FlyCam) after the
//! reload — without it, `load` always dropped the player at the cell's
//! default door spawn rather than the saved spot.
//!
//! ## File layout (#5585)
//!
//! Split from a single 2192-line `save_io.rs` by responsibility; every
//! pre-split path (`crate::save_io::X`) re-exports from here unchanged:
//!
//! - [`registry`] — the curated registry rows, `MUTABLE_DELTA_COLUMNS`,
//!   save-dir discovery, `SaveState` / `PendingSaveLoadSlot`
//! - [`player`] — `PlayerPose` capture/apply, the deferred
//!   player save-action queue, snapshot context accessors
//! - [`commands`] — `save` / `save.info` / `load` console commands and
//!   the binary-side validation gates
//! - [`live_load`] — [`execute_pending_save_loads`] and the reload
//!   helpers it drains through

use std::path::PathBuf;

use byroredux_core::console::{CommandOutput, ConsoleCommand};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::World;
use byroredux_core::math::Vec3;
use byroredux_save::validate::{
    log_validation_warnings, validate_entity_reference, validate_world, ValidationError,
    ValidationKind,
};
use byroredux_save::{disk, encode, save_world, SaveRegistry, Snapshot};

mod commands;
mod live_load;
mod player;
mod registry;

pub use commands::{LoadCommand, SaveCommand, SaveInfoCommand, queue_startup_load};
pub use live_load::execute_pending_save_loads;
pub use player::{
    PendingPlayerSaveActions, PlayerPose, PlayerSaveAction, apply_player_pose,
    capture_player_pose, queue_player_save_action, snapshot_cell_context,
    snapshot_exterior_context, snapshot_player_pose,
};
pub use registry::{
    PendingSaveLoadSlot, SaveState, build_save_registry, discover_save_dir,
};
pub(crate) use commands::{command_output_is_failure, execute_pending_player_save_actions};
// Same-width re-exports for the items that were module-private before the
// split and are consumed across the sibling boundary in production: the
// load drain (live_load) runs the same validation gates the save command
// (commands) runs, and overlays `MUTABLE_DELTA_COLUMNS` (registry).
use commands::{validate_cinematic_entity_refs, validate_form_ids};
use registry::MUTABLE_DELTA_COLUMNS;
// Test-only re-exports, at the width these symbols had before the split
// (same pattern as `asset_provider/material`'s #3857 split): the sibling
// test modules reach them through `use super::*`, which the
// unused-import lint cannot see through — without the `cfg(test)` gate
// the production build warns, and without the re-export the test build
// does not compile. Both halves are load-bearing.
#[cfg(test)]
pub(crate) use commands::{quickload_latest, quicksave};
#[cfg(test)]
use live_load::{
    LiveReloadLedgers, PRE_RELOAD_RESOURCES, clear_player_gear_handoff_scratch,
    discard_outgoing_session_requests, discard_pending_story_events,
    exterior_reload_bootstrap_mode, reset_story_location_cursor,
};

// Tests live in sibling files by topic (#2407 / TD1-004) — the repo's
// existing convention (`cell_loader/*_tests.rs`,
// `scene_buffer/*_tests.rs`). Contents moved verbatim.
#[cfg(test)]
mod command_queue_tests;
#[cfg(test)]
mod consumable_tests;
#[cfg(test)]
mod live_reload_tests;
#[cfg(test)]
mod registry_completeness_tests;
#[cfg(test)]
mod round_trip_tests;
#[cfg(test)]
mod serde_default_guard_tests;
#[cfg(test)]
mod validation_gate_tests;
