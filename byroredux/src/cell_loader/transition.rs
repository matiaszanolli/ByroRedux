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

/// Identity of the interior cell currently loaded, plus the plugin set it
/// came from — everything a save needs to reload the same cell.
///
/// Set by [`super::load::load_cell_with_masters`] on every interior load;
/// the M45 save registry serialises it as a resource column so `load`
/// can re-issue the same `TransitionDestination::Interior` before applying
/// saved component deltas. Absent in loose-NIF / exterior-streaming modes
/// (no single interior cell identity).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CurrentCellContext {
    /// Cell editor-ID (the `--cell` value / transition destination key).
    pub cell_editor_id: String,
    /// Main ESM path (the `--esm` value).
    pub esm_path: String,
    /// Master plugin paths in load order (the repeated `--master` values).
    pub masters: Vec<String>,
}

impl Resource for CurrentCellContext {}

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
/// Mirrors the convention used at REFR placement in `references.rs`:
/// `(x, z, -y)`. #1617 — delegates to the coord SoT so this stays in
/// lockstep with the canonical swap (bit-identical to the old inline form).
pub fn position_zup_to_yup(p: [f32; 3]) -> Vec3 {
    Vec3::from_array(byroredux_core::math::coord::zup_to_yup_pos(p))
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

/// Tear down the currently-loaded interior cell, if any. Reads
/// [`CurrentCellRoot`] — `Some(root)` means
/// `load_cell_with_masters` was the last cell entry point and there's
/// an interior to drop. Always clears `CurrentCellRoot` to `None` so
/// the orchestrator can re-stamp on the next load.
///
/// Used by [`load_interior_cell`] and the App-level Interior→Exterior
/// path (which needs to drop the interior before spinning up the
/// streaming state).
pub fn unload_current_interior(
    world: &mut byroredux_core::ecs::World,
    ctx: &mut byroredux_renderer::VulkanContext,
) {
    let prev_root = world.try_resource::<CurrentCellRoot>().and_then(|r| r.0);
    if let Some(prev) = prev_root {
        log::info!("Transition: unloading prior interior cell (root {prev})");
        super::unload_cell(world, ctx, prev);
    }
    world.insert_resource(CurrentCellRoot(None));
}

/// Reposition the [`ActiveCamera`] at a destination spawn point.
/// Pure World mutation — used by both Interior→Interior and
/// Interior→Exterior / Exterior→Interior paths.
pub fn reposition_camera(world: &mut byroredux_core::ecs::World, dest_pos: Vec3, dest_rot: Quat) {
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
}

/// Load an interior cell as part of a transition. Tears down any prior
/// interior, calls `load_cell_with_masters` for the destination, then
/// repositions the camera. The caller is responsible for draining any
/// exterior `WorldStreamingState` before this fires — that lives on
/// `App`, not `World`, so the orchestrator can't reach it.
///
/// Returns the engine-Y-up camera position on success so the App can
/// log + signal the SVGF/TAA temporal-discontinuity recovery window.
/// Source + destination descriptor for an interior transition: which cell to
/// load (`editor_id` resolved against `masters` / `esm_path`) and where to
/// drop the camera afterwards (Z-up position + rotation, converted to Y-up
/// inside [`load_interior_cell`]). Grouped to keep the argument count down.
pub struct InteriorCellRequest<'a> {
    pub editor_id: &'a str,
    pub masters: &'a [String],
    pub esm_path: &'a str,
    pub dest_pos_zup: [f32; 3],
    pub dest_rot_zup: [f32; 3],
}

pub fn load_interior_cell(
    world: &mut byroredux_core::ecs::World,
    ctx: &mut byroredux_renderer::VulkanContext,
    tex_provider: &crate::asset_provider::TextureProvider,
    mat_provider: Option<&mut crate::asset_provider::MaterialProvider>,
    request: InteriorCellRequest,
) -> Result<Vec3, String> {
    let InteriorCellRequest {
        editor_id,
        masters,
        esm_path,
        dest_pos_zup,
        dest_rot_zup,
    } = request;
    unload_current_interior(world, ctx);
    let result = super::load_cell_with_masters(
        masters,
        esm_path,
        editor_id,
        world,
        ctx,
        tex_provider,
        mat_provider,
    )
    .map_err(|e| format!("{e:#}"))?;

    // #1340 — apply the loaded interior's lighting (the startup `--cell`
    // path does this too, via the same helper). Without it the door-walked
    // interior keeps the previous cell's `CellLightingRes`: stale
    // ambient/fog + the exterior directional sun leaking into a sealed
    // interior (the failure #1282 gated on `is_interior`).
    if let Some(ref lit) = result.lighting {
        super::apply_interior_cell_lighting(world, lit);
    }

    let dest_pos = position_zup_to_yup(dest_pos_zup);
    let dest_rot = rotation_zup_to_yup_quat(dest_rot_zup);
    reposition_camera(world, dest_pos, dest_rot);
    // #1874 — `reposition_camera` only moves the CAMERA. In
    // `PlayerMode::Character` the physics capsule stays behind in the
    // just-unloaded source cell; `camera_follow_system` (Stage::Late,
    // every frame) pins the camera to "body position + eye_height,"
    // so on the very next tick it would snap the camera straight back
    // toward the stale (often now ungrounded / free-falling through
    // unloaded geometry) capsule — undoing this reposition and
    // re-triggering a fresh, unsignaled camera discontinuity every
    // frame until the capsule happened to settle. That recurring
    // fight, not a single bad motion vector, is what let a ghosted/
    // doubled TAA-SVGF history artifact "stick" indefinitely after a
    // door transition. No-ops harmlessly in FlyCam mode (no player
    // body to snap). See `snap_character_body_to_camera`'s doc comment
    // for the full mechanism.
    crate::systems::snap_character_body_to_camera(world);
    Ok(dest_pos)
}

/// Log header used by both interior and exterior orchestrator entries.
/// Pulled out so the App-level dispatcher and the in-module helpers
/// emit one consistent format.
pub fn log_transition_header(transition: &PendingCellTransition) -> String {
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
    dest_label
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
        let prev = world.try_resource::<CurrentCellRoot>().and_then(|r| r.0);
        assert!(
            prev.is_none(),
            "no CurrentCellRoot resource → orchestrator must treat as clean-slate"
        );
    }
}
