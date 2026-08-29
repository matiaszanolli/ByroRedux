//! Process-lifetime wrapper around the parsed `EsmIndex` so the plugin
//! cell table is reachable from console commands + future systems via
//! the ECS resource API.
//!
//! Inserted by BOTH cell-load entry points after the ESM parse completes —
//! `load_cell_with_masters` for interiors and `assemble_exterior_streaming`
//! for every exterior path (boot `--grid`, the transition orchestrator's
//! Exterior arm, `dbgload`, and the save-reload path all funnel through
//! it). The resource shadows the function-local index so `&World` readers
//! (like the `door.teleport` console command — M40 Phase 2 Stage 1, and
//! `queue_door_transition` behind player activation) can look up where a
//! destination FormID's parent cell lives without re-parsing the plugin.
//!
//! #3415 — the exterior half of that was documented here but never
//! implemented: the insert existed only inside `load_cell_with_masters`,
//! so on a `--grid` boot the resource was absent for the whole session and
//! every exterior door failed with `MissingCellIndex`.
//!
//! Why a wrapper instead of `impl Resource for EsmCellIndex` directly:
//! the `byroredux-plugin` crate has no dependency on
//! `byroredux-core` (and we want to keep it that way — plugin parsing
//! is the foundation layer the ECS crate is built on). A thin newtype
//! here threads the orphan rule without forcing plugin to take a
//! reverse dependency.

use std::sync::Arc;

use byroredux_core::ecs::Resource;
use byroredux_plugin::esm::records::EsmIndex;

/// World-resource wrapper around the parsed [`EsmIndex`] for the
/// currently-loaded scene. Set by both cell-load entry points after the
/// ESM parse completes. The cell table itself is `.0.cells`.
///
/// **Why the whole index and not just `EsmCellIndex`** (#3415): the
/// exterior context already owns an `Arc<EsmIndex>`
/// (`ExteriorWorldContext::record_index`), and an `Arc` cannot be
/// projected onto one of its fields — so a resource holding
/// `Arc<EsmCellIndex>` could only be built there by deep-cloning every
/// cell / static / TXST map in the load order. Widening the payload lets
/// the exterior arm hand over a refcount bump, and costs the interior arm
/// nothing: it owns its `EsmIndex` outright and moves it into the `Arc`.
/// The exterior path has kept the full index resident for the whole
/// session since it was written, so this is the profile that was already
/// proven on the same games, applied to both arms.
///
/// Read-only after insertion — the parsed index is treated as
/// immutable scene metadata; subsequent cell loads (e.g. through an
/// XTEL portal) replace the resource wholesale rather than mutating in
/// place, so the borrow patterns stay simple.
pub struct LoadedCellIndex(pub Arc<EsmIndex>);

impl Resource for LoadedCellIndex {}

#[cfg(test)]
mod boot_arm_coverage_tests {
    //! #3415 — the resource must be installed on BOTH boot arms.
    //!
    //! Source-level rather than runtime: both producers need a live
    //! `VulkanContext` (and, for the exterior arm, real game data), so
    //! neither is reachable from `cargo test`. Scraping the call sites is
    //! what actually keys the guard on the *boot path*, which is where the
    //! defect lived — a runtime test of `queue_door_transition` passes
    //! happily while an entry point silently never inserts the resource.
    //! Same `include_str!` pattern the material-translate boundary guards
    //! and `nif_loader_light_tests` already use.

    /// The interior entry point (`load_cell_with_masters`).
    const LOAD_RS: &str = include_str!("load.rs");
    /// The single funnel every exterior entry point goes through — boot
    /// `--grid`, `begin_exterior_streaming` (the transition orchestrator's
    /// Exterior arm, `dbgload`, and the save-reload path).
    const WORLD_SETUP_RS: &str = include_str!("../scene/world_setup.rs");

    fn inserts_cell_index(src: &str) -> bool {
        src.contains("LoadedCellIndex(")
    }

    #[test]
    fn both_boot_arms_install_the_cell_index() {
        assert!(
            inserts_cell_index(LOAD_RS),
            "the interior loader must insert LoadedCellIndex — without it \
             `queue_door_transition` fails with MissingCellIndex and every \
             door is inert"
        );
        assert!(
            inserts_cell_index(WORLD_SETUP_RS),
            "the exterior streaming funnel must insert LoadedCellIndex too \
             (#3415). Pre-fix only the interior arm did, so on a `--grid` \
             boot the resource was absent for the whole session and every \
             exterior door on FNV's reference route failed silently"
        );
    }

    /// The insert has to happen in `assemble_exterior_streaming`
    /// specifically — that is the funnel all four exterior entry points
    /// share. Putting it in only one caller (e.g. the boot arm) would leave
    /// the transition / dbgload / save-reload paths uncovered again.
    #[test]
    fn the_exterior_insert_sits_in_the_shared_funnel() {
        let start = WORLD_SETUP_RS
            .find("pub(crate) fn assemble_exterior_streaming")
            .expect("assemble_exterior_streaming must exist");
        let rest = &WORLD_SETUP_RS[start..];
        let end = rest
            .find("\npub(crate) fn ")
            .map(|i| i + 1)
            .unwrap_or(rest.len());
        assert!(
            inserts_cell_index(&rest[..end]),
            "LoadedCellIndex must be inserted inside assemble_exterior_streaming \
             — the funnel shared by boot `--grid`, the transition \
             orchestrator's Exterior arm, `dbgload`, and save-reload"
        );
    }
}
