//! Process-lifetime wrapper around `EsmCellIndex` so the parsed plugin
//! cell table is reachable from console commands + future systems via
//! the ECS resource API.
//!
//! Inserted by the cell-load entry points (`load_cell_with_masters` and
//! `build_exterior_world_context`) after the ESM parse completes; the
//! resource shadows the function-local `index` so resources of `&World`
//! readers (like the `door.teleport` console command — M40 Phase 2
//! Stage 1) can look up where a destination FormID's parent cell lives
//! without re-parsing the plugin.
//!
//! Why a wrapper instead of `impl Resource for EsmCellIndex` directly:
//! the `byroredux-plugin` crate has no dependency on
//! `byroredux-core` (and we want to keep it that way — plugin parsing
//! is the foundation layer the ECS crate is built on). A thin newtype
//! here threads the orphan rule without forcing plugin to take a
//! reverse dependency.

use std::sync::Arc;

use byroredux_core::ecs::Resource;
use byroredux_plugin::esm::cell::EsmCellIndex;

/// World-resource wrapper around the parsed `EsmCellIndex` for the
/// currently-loaded scene. Set by the cell-load entry points after the
/// ESM parse completes.
///
/// Held behind `Arc` so the exterior streaming path can share the same
/// allocation that `ExteriorWorldContext.record_index` already holds —
/// no per-resource clone on scene setup. The interior load path
/// constructs a fresh `Arc` from its function-local index.
///
/// Read-only after insertion — the parsed index is treated as
/// immutable scene metadata; subsequent cell loads (e.g. through an
/// XTEL portal) replace the resource wholesale rather than mutating in
/// place, so the borrow patterns stay simple.
pub struct LoadedCellIndex(pub Arc<EsmCellIndex>);

impl Resource for LoadedCellIndex {}
