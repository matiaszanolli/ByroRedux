//! Cell-transition orchestrator (M40 Phase 2 Stage 3).
//!
//! Console commands and the future F-key activate system can't trigger
//! a cell swap directly — they take `&World` (read-only) while
//! load/unload requires `&mut World + &mut VulkanContext + Provider`s.
//! The deferred-execution shape solves this:
//!
//!   1. The trigger site (`door.teleport` console command, F-key
//!      activate system) writes a [`PendingCellTransition`] resource
//!      with the destination cell + camera position/rotation.
//!   2. The next frame's main loop checks for the resource via
//!      [`take_pending_transition`] and, if set, dispatches to
//!      [`execute_pending`] with full mutable access.
//!
//! This module owns the resource types + the orchestrator. The actual
//! load/unload primitives live in their existing siblings
//! (`load::load_cell_with_masters`, `unload::unload_cell`); this layer
//! threads them together with the correct state machine for each of
//! the four transition pairs.
//!
//! Pairs handled:
//! - **Interior → Interior**: unload current interior (via
//!   [`CurrentCellRoot`]), load destination, reposition camera.
//! - **Exterior → Interior**: caller drains the `WorldStreamingState`
//!   beforehand (App-owned, not World-visible), then dispatches here.
//! - **Interior → Exterior**: out of scope this stage — errors cleanly.
//!   Requires spinning up a fresh `WorldStreamingState`; deferred to
//!   M40 Phase 2 Stage 3b.
//! - **Exterior → Exterior** (cross-worldspace): out of scope, errors.

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::Resource;
use byroredux_core::math::{Quat, Vec3};

/// Plugin-load configuration captured at engine boot. The transition
/// orchestrator re-uses this to call `load_cell_with_masters` for the
/// destination cell without re-parsing CLI args. Set by the boot path
/// in `scene::setup_scene` whenever a `--esm`-driven cell load runs;
/// absent otherwise (loose-NIF / sweet-roll modes).
///
/// Fields mirror the CLI arg shape so the trigger site reading this
/// resource can hand it straight to [`PendingCellTransition`].
#[derive(Clone, Debug)]
pub struct LoadedPluginSet {
    /// Repeatable `--master <path>` args in CLI order.
    pub masters: Vec<String>,
    /// The `--esm <path>` value.
    pub esm_path: String,
}

impl Resource for LoadedPluginSet {}

/// Tracks the placement-root entity of the currently-loaded interior
/// cell. `Some(root)` after [`super::load::load_cell_with_masters`]
/// returns; cleared by [`execute_pending`] before loading the next
/// cell.
///
/// `None` in two cases:
///   - No cell loaded yet (engine just booted in `--mesh` or `--tree` mode).
///   - Exterior streaming is active (the `WorldStreamingState.loaded`
///     map tracks loaded cells; no single root).
pub struct CurrentCellRoot(pub Option<EntityId>);

impl Resource for CurrentCellRoot {}

/// A queued cell transition. Set by trigger sites (`door.teleport`
/// console command today; F-key activate system in Stage 4), consumed
/// by the main loop the next frame.
///
/// The destination cell is identified by editor-ID + master list. The
/// camera target carries Bethesda Z-up position + Euler rotation
/// straight from the XTEL sub-record; the orchestrator does the
/// Z-up → Y-up flip at consumption.
#[derive(Clone, Debug)]
pub struct PendingCellTransition {
    /// Destination cell editor-ID (interior cells) OR worldspace + grid
    /// (exterior cells, Stage 3b). The orchestrator decides interior vs
    /// exterior dispatch on the enum variant.
    pub destination: TransitionDestination,
    /// Source REFR's placement form-id (for diagnostic logging only).
    pub source_refr_form_id: u32,
    /// Destination position from XTEL (Bethesda Z-up world units). The
    /// orchestrator flips to engine Y-up at execution.
    pub destination_position_zup: [f32; 3],
    /// Destination rotation from XTEL (Bethesda Z-up Euler radians).
    /// Conversion to engine Y-up Quat uses the same `euler_zup_to_quat_yup`
    /// helper REFR placements use.
    pub destination_rotation_zup: [f32; 3],
}

/// Resource slot for the queued transition — always present (inserted
/// at engine boot) so write sites with only `&World` access (console
/// commands) can mutate the `Option` via `resource_mut` without
/// needing to structurally insert. Mirrors the
/// `SelectedRef(Option<EntityId>)` shape used by the `prid` console
/// command.
#[derive(Debug, Default)]
pub struct PendingCellTransitionSlot(pub Option<PendingCellTransition>);

impl Resource for PendingCellTransitionSlot {}

/// Destination classification — produced by the trigger site after it
/// queries the cell index via `cell_for_refr_form_id`. The orchestrator
/// reads this to pick the right load entry point.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransitionDestination {
    /// Interior cell, identified by editor-ID. Loaded via
    /// `load_cell_with_masters`.
    Interior {
        editor_id: String,
        /// Master plugin paths (in load order) — needed because the
        /// destination cell may live in a different plugin than the
        /// source. Today this is the original CLI master list re-played
        /// at transition time; future scope (Stage 3b) will track this
        /// in a session resource keyed off the source plugin.
        masters: Vec<String>,
        /// Main ESM path. Same caveat as `masters`.
        esm_path: String,
    },
    /// Exterior cell at the given worldspace + grid. **Not yet
    /// implemented** — Stage 3b work. Trigger sites can populate this
    /// today; the orchestrator errors cleanly so the queueing path
    /// still gets exercised.
    Exterior {
        worldspace: String,
        grid: (i32, i32),
        masters: Vec<String>,
        esm_path: String,
    },
}

/// Convert a Bethesda Z-up world-space position to engine Y-up.
/// Mirrors the convention used at REFR placement in
/// `references.rs:198-202`: `(x, z, -y)`.
pub fn position_zup_to_yup(p: [f32; 3]) -> Vec3 {
    Vec3::new(p[0], p[2], -p[1])
}

/// Convert a Bethesda Z-up Euler rotation triple to an engine Y-up
/// quaternion. Wrapper over [`super::euler_zup_to_quat_yup_refr`] —
/// same convention REFR placements use, so the camera's orientation
/// matches what the destination REFR would render at.
pub fn rotation_zup_to_yup_quat(rot: [f32; 3]) -> Quat {
    super::euler_zup_to_quat_yup(rot[0], rot[1], rot[2])
}

/// Drain the pending transition from the slot resource. The main
/// loop calls this once per frame; a `Some` return means the next
/// step in the loop must dispatch to [`execute_pending`]. Always
/// leaves `PendingCellTransitionSlot(None)` behind so the resource
/// stays present for the next trigger.
pub fn take_pending_transition(
    world: &byroredux_core::ecs::World,
) -> Option<PendingCellTransition> {
    let mut slot = world.try_resource_mut::<PendingCellTransitionSlot>()?;
    slot.0.take()
}

/// Outcome of the orchestrator — surfaced so the main loop can log /
/// react to failures without inspecting the resource itself.
#[derive(Debug)]
pub enum TransitionOutcome {
    /// Transition succeeded; the world is now at the destination cell.
    Applied {
        /// Engine-space (Y-up) world position where the camera was set.
        camera_position: Vec3,
        /// Diagnostic — destination editor-id (interior) or
        /// `"<worldspace> ({gx},{gy})"` (exterior).
        destination_label: String,
    },
    /// Transition was dropped because the destination variant isn't
    /// implemented yet (interior↔exterior swap = Stage 3b work).
    /// The orchestrator logs the request; the queue entry is consumed
    /// either way so the engine doesn't retry on every frame.
    NotImplemented {
        destination_label: String,
        reason: &'static str,
    },
    /// Transition failed at the loader layer — destination cell not
    /// found in the plugin set, ESM parse error, etc. Logged at error
    /// level; the engine continues with whatever cell was previously
    /// loaded (no rollback).
    Failed {
        destination_label: String,
        error: String,
    },
}

/// Execute a pending interior cell transition. Caller must already
/// have:
///   1. Drained any active exterior streaming state (if transitioning
///      OUT of exterior — App-owned, not visible from World).
///   2. Passed `take_pending_transition` output through, so the queue
///      is empty before this returns.
///
/// On entry, [`CurrentCellRoot`] tells the orchestrator whether a
/// previous interior cell is loaded and needs to be unloaded first;
/// `None` means a clean slate (engine boot or post-exterior-shutdown).
///
/// Side effects on success:
///   - Old cell (if any) torn down via `unload_cell`.
///   - Destination cell loaded via `load_cell_with_masters`, which
///     re-stamps [`CurrentCellRoot`] + replaces [`super::LoadedCellIndex`].
///   - Active camera repositioned to destination.position (Y-up flip)
///     and rotated to destination.rotation (Y-up Quat).
pub fn execute_pending(
    world: &mut byroredux_core::ecs::World,
    ctx: &mut byroredux_renderer::VulkanContext,
    tex_provider: &crate::asset_provider::TextureProvider,
    mat_provider: Option<&mut crate::asset_provider::MaterialProvider>,
    transition: PendingCellTransition,
) -> TransitionOutcome {
    let dest_label = match &transition.destination {
        TransitionDestination::Interior { editor_id, .. } => format!("interior '{editor_id}'"),
        TransitionDestination::Exterior {
            worldspace, grid, ..
        } => format!("exterior '{worldspace}' ({},{})", grid.0, grid.1),
    };
    log::info!(
        "Transition: source REFR {:08X} → {} at pos Z-up ({:.1}, {:.1}, {:.1})",
        transition.source_refr_form_id,
        dest_label,
        transition.destination_position_zup[0],
        transition.destination_position_zup[1],
        transition.destination_position_zup[2],
    );

    let (editor_id, masters, esm_path) = match transition.destination {
        TransitionDestination::Interior {
            editor_id,
            masters,
            esm_path,
        } => (editor_id, masters, esm_path),
        TransitionDestination::Exterior { .. } => {
            // Stage 3b — interior↔exterior swap requires WorldStreamingState
            // lifecycle hooks the orchestrator doesn't have access to from
            // here (state lives in App, not World). Drop the request.
            return TransitionOutcome::NotImplemented {
                destination_label: dest_label,
                reason: "interior↔exterior transition is M40 Phase 2 Stage 3b",
            };
        }
    };

    // 1. Tear down the current interior cell (if any). The
    // CurrentCellRoot resource is `Some` iff `load_cell_with_masters`
    // was the last cell-load entry point; `None` for fresh-boot
    // (sweet-roll / loose-NIF mode) or for exterior-streaming mode
    // (where the resource is left unset by the streaming entry points).
    let prev_root = world
        .try_resource::<CurrentCellRoot>()
        .and_then(|r| r.0);
    if let Some(prev) = prev_root {
        log::info!("Transition: unloading prior interior cell (root {prev})");
        super::unload_cell(world, ctx, prev);
    }
    // Clear the tracker before reloading; `load_cell_with_masters`
    // re-stamps it at the end of its successful return.
    world.insert_resource(CurrentCellRoot(None));

    // 2. Load the destination interior cell. Re-uses the same entry
    // point that the boot path calls, so the resource side-effects
    // (LoadedCellIndex, CurrentCellRoot, CellLightingRes, …) match
    // boot-path semantics 1:1.
    let load_result = super::load_cell_with_masters(
        &masters,
        &esm_path,
        &editor_id,
        world,
        ctx,
        tex_provider,
        mat_provider,
    );
    match load_result {
        Ok(_) => {}
        Err(e) => {
            return TransitionOutcome::Failed {
                destination_label: dest_label,
                error: format!("{e:#}"),
            };
        }
    }

    // 3. Reposition the active camera at the destination spawn point.
    let dest_pos = position_zup_to_yup(transition.destination_position_zup);
    let dest_rot = rotation_zup_to_yup_quat(transition.destination_rotation_zup);
    if let Some(active) = world.try_resource::<byroredux_core::ecs::ActiveCamera>() {
        let cam_entity = active.0;
        drop(active);
        if let Some(mut tq) = world.query_mut::<byroredux_core::ecs::Transform>() {
            if let Some(transform) = tq.get_mut(cam_entity) {
                transform.translation = dest_pos;
                transform.rotation = dest_rot;
            }
        }
    }

    TransitionOutcome::Applied {
        camera_position: dest_pos,
        destination_label: dest_label,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::World;

    /// Mirror of the Z-up → Y-up convention REFR placements use
    /// (`references.rs:198-202`): `(x, y, z)_zup → (x, z, -y)_yup`.
    /// Pin the helper against three canonical orientations.
    #[test]
    fn position_zup_to_yup_matches_refr_convention() {
        // Forward (Bethesda +Y axis) → engine -Z.
        assert_eq!(
            position_zup_to_yup([0.0, 100.0, 0.0]),
            Vec3::new(0.0, 0.0, -100.0)
        );
        // Up (Bethesda +Z axis) → engine +Y.
        assert_eq!(
            position_zup_to_yup([0.0, 0.0, 50.0]),
            Vec3::new(0.0, 50.0, 0.0)
        );
        // Right (Bethesda +X axis) stays +X.
        assert_eq!(
            position_zup_to_yup([25.0, 0.0, 0.0]),
            Vec3::new(25.0, 0.0, 0.0)
        );
        // Mixed.
        assert_eq!(
            position_zup_to_yup([10.0, 20.0, 30.0]),
            Vec3::new(10.0, 30.0, -20.0)
        );
    }

    /// The transition slot pre-installed at engine boot is consumed
    /// atomically by `take_pending_transition` — a single call returns
    /// the queued entry; the second call returns `None`.
    #[test]
    fn take_pending_transition_drains_the_slot_once() {
        let mut world = World::new();
        world.insert_resource(PendingCellTransitionSlot::default());

        // No pending yet.
        assert!(take_pending_transition(&world).is_none());

        // Trigger site posts one.
        {
            let mut slot = world
                .try_resource_mut::<PendingCellTransitionSlot>()
                .unwrap();
            slot.0 = Some(PendingCellTransition {
                destination: TransitionDestination::Interior {
                    editor_id: "GSDocMitchellHouse".into(),
                    masters: Vec::new(),
                    esm_path: "FalloutNV.esm".into(),
                },
                source_refr_form_id: 0xDEAD,
                destination_position_zup: [1.0, 2.0, 3.0],
                destination_rotation_zup: [0.0, 0.0, 0.0],
            });
        }

        // First take returns the entry.
        let taken = take_pending_transition(&world);
        assert!(taken.is_some(), "first take must drain the slot");
        let taken = taken.unwrap();
        assert_eq!(taken.source_refr_form_id, 0xDEAD);
        assert!(matches!(
            taken.destination,
            TransitionDestination::Interior { ref editor_id, .. }
                if editor_id == "GSDocMitchellHouse"
        ));

        // Second take returns None — the slot stays in place but
        // empty, so the main loop's per-frame check is a cheap no-op
        // on subsequent frames until the next trigger fires.
        assert!(
            take_pending_transition(&world).is_none(),
            "slot must be empty after drain"
        );
        assert!(
            world.try_resource::<PendingCellTransitionSlot>().is_some(),
            "slot resource must stay present (not removed)"
        );
    }

    /// `CurrentCellRoot` defaults to "no interior loaded" when absent.
    /// The orchestrator queries it via `try_resource` so the absence
    /// case has to read as a clean-slate, not a panic.
    #[test]
    fn current_cell_root_absence_is_treated_as_no_interior() {
        let world = World::new();
        let prev = world
            .try_resource::<CurrentCellRoot>()
            .and_then(|r| r.0);
        assert!(
            prev.is_none(),
            "no CurrentCellRoot resource → orchestrator must treat as clean-slate"
        );
    }
}
