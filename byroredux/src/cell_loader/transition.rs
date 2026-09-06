//! Cell-transition orchestrator (M40 Phase 2 Stage 3).
//!
//! Console commands and the gameplay E-key activate system can't trigger
//! a cell swap directly — they take `&World` (read-only) while
//! load/unload requires `&mut World + &mut VulkanContext + Provider`s.
//! The deferred-execution shape solves this:
//!
//!   1. The trigger site (`door.teleport` console command, E-key
//!      activate system) writes a [`PendingCellTransition`] resource
//!      with the destination cell + camera position/rotation.
//!   2. The next frame's main loop checks for the resource via
//!      [`take_pending_transition`] and, if set, dispatches to
//!      [`execute_pending`] with full mutable access.
//!
//! This module owns the resource types + the orchestrator. The actual
//! load/unload primitives live in their existing siblings
//! (`load::load_cell_with_masters`, `load::InteriorCellApplyJob`,
//! `unload::unload_cell`); this layer threads them together with the correct
//! state machine for each of the four transition pairs.
//!
//! Pairs handled:
//! - **Interior → Interior**: unload current interior (via
//!   [`CurrentCellRoot`]), load destination, reposition camera.
//! - **Exterior → Interior**: caller drains the `WorldStreamingState`
//!   beforehand (App-owned, not World-visible), then dispatches here.
//! - **Interior → Exterior** and **Exterior → Exterior** (including
//!   cross-worldspace): both handled by `App::step_cell_transition`'s
//!   `TransitionDestination::Exterior` arm — tear down any active
//!   interior first (no-op on the Exterior→Exterior path), drain any
//!   existing `WorldStreamingState` (always, even intra-worldspace, so
//!   the failure mode stays uniform), rebuild a fresh
//!   `ExteriorWorldContext` + `WorldStreamingState` for the destination
//!   worldspace/grid, and restream the initial radius. This is a full
//!   drain-and-reparse, not a state-preserving crossing: live changes to
//!   persistent refs made in the worldspace being left are not carried
//!   forward across the crossing (EX-15/#2369 tracks closing that gap).

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::Resource;
use byroredux_core::math::{Quat, Vec3};

use crate::asset_provider::{MaterialProvider, TextureProvider};
use crate::components::DoorTeleport;

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

/// Identity of the exterior worldspace/grid currently streaming — the
/// `CurrentCellContext` equivalent for exterior sessions (EX-09/17 item 4).
///
/// `WorldStreamingState` (the thing that actually knows worldspace/grid/
/// radius) lives on `App`, not as a `World` resource — it can't, since the
/// streaming driver needs to borrow it alongside `VulkanContext` and the
/// asset providers every frame, and `SaveCommand::execute` only ever sees
/// `&World`. This resource is the deliberately-thin mirror: just the
/// identity fields a save/load round-trip needs, kept in sync at every
/// point that starts, moves, or tears down exterior streaming (see
/// [`crate::scene::begin_exterior_streaming`], which sets this after every
/// fresh exterior session, `App::step_streaming`'s grid-crossing update,
/// and [`crate::streaming_helpers::drain_streaming_state`], which clears
/// it — the exterior mirror of [`clear_current_interior_identity`] below).
///
/// Absent in loose-NIF / interior modes, same posture as `CurrentCellContext`
/// being absent in exterior mode — the two are mutually exclusive, matching
/// `WorldStreamingState`/`CurrentCellRoot`'s existing either-or contract.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CurrentExteriorContext {
    /// Lowercase worldspace EDID (`ExteriorWorldContext::worldspace_key`).
    pub worldspace_key: String,
    /// Main ESM path (the `--esm` value).
    pub esm_path: String,
    /// Master plugin paths in load order (the repeated `--master` values).
    pub masters: Vec<String>,
    /// Player's current grid cell — `WorldStreamingState::last_player_grid`.
    pub grid: (i32, i32),
    /// Load radius the session was streaming at.
    pub radius_load: i32,
    /// Unload radius the session was streaming at.
    pub radius_unload: i32,
}

impl Resource for CurrentExteriorContext {}

/// Clear the exterior identity mirror. Called from
/// [`crate::streaming_helpers::drain_streaming_state`] — the same choke
/// point every streaming teardown (Exterior→Interior, Exterior→Exterior,
/// save-load reload) already funnels through, mirroring how
/// [`clear_current_interior_identity`] hangs off [`unload_current_interior`]
/// below. A fresh exterior session re-stamps this immediately after
/// (`begin_exterior_streaming`), so the only observable "absent" window is
/// between a teardown and its following rebuild — same as `CurrentCellRoot`
/// briefly reading `None` mid-transition today.
pub(crate) fn clear_current_exterior_identity(world: &mut byroredux_core::ecs::World) {
    world.remove_resource::<CurrentExteriorContext>();
}

/// A queued cell transition. Set by trigger sites (`door.teleport`
/// console command and E-key gameplay interaction), consumed
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
    /// Exterior cell at the given worldspace + grid. Implemented —
    /// `App::step_cell_transition` drains any existing streaming state,
    /// rebuilds a fresh `ExteriorWorldContext`/`WorldStreamingState` for
    /// this worldspace/grid, and restreams the initial radius. Covers
    /// both Interior→Exterior and Exterior→Exterior (including
    /// cross-worldspace), per the module doc above.
    Exterior {
        worldspace: String,
        grid: (i32, i32),
        masters: Vec<String>,
        esm_path: String,
    },
}

/// Successful result from resolving and queueing a door's XTEL payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueuedDoorTransition {
    pub destination_label: String,
    pub destination_form_id: u32,
}

/// Recoverable reasons a door activation cannot become a cell transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueueDoorTransitionError {
    MissingDoor(EntityId),
    MissingCellIndex,
    DestinationNotFound(u32),
    MissingPluginSet,
    MissingTransitionSlot,
}

impl std::fmt::Display for QueueDoorTransitionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingDoor(entity) => {
                write!(formatter, "entity {entity} has no DoorTeleport component")
            }
            Self::MissingCellIndex => write!(
                formatter,
                "no LoadedCellIndex resource; an ESM-driven cell load is required"
            ),
            Self::DestinationNotFound(form_id) => write!(
                formatter,
                "destination FormID {form_id:08X} is absent from the loaded plugin set"
            ),
            Self::MissingPluginSet => write!(
                formatter,
                "no LoadedPluginSet resource; the engine was not booted with --esm"
            ),
            Self::MissingTransitionSlot => write!(
                formatter,
                "no PendingCellTransitionSlot resource; scene setup is incomplete"
            ),
        }
    }
}

impl std::error::Error for QueueDoorTransitionError {}

/// Resolve a door's destination REFR to its parent cell and enqueue the
/// existing deferred transition orchestrator.
///
/// This is the single producer used by both normal player interaction and the
/// `door.teleport` diagnostic command, preventing the debug and gameplay paths
/// from drifting apart.
pub fn queue_door_transition(
    world: &byroredux_core::ecs::World,
    entity: EntityId,
) -> Result<QueuedDoorTransition, QueueDoorTransitionError> {
    let door = world
        .query::<DoorTeleport>()
        .and_then(|query| query.get(entity).copied())
        .ok_or(QueueDoorTransitionError::MissingDoor(entity))?;

    let index = world
        .try_resource::<super::index::LoadedCellIndex>()
        .ok_or(QueueDoorTransitionError::MissingCellIndex)?;
    let owned_cell = index
        .0
        .cells
        .cell_for_refr_form_id(door.destination_form_id)
        .map(|cell| cell.to_owned())
        .ok_or(QueueDoorTransitionError::DestinationNotFound(
            door.destination_form_id,
        ))?;
    drop(index);

    let plugin_set = world
        .try_resource::<LoadedPluginSet>()
        .ok_or(QueueDoorTransitionError::MissingPluginSet)?;
    let masters = plugin_set.masters.clone();
    let esm_path = plugin_set.esm_path.clone();
    drop(plugin_set);

    use byroredux_plugin::esm::cell::OwnedCellRef;
    let (destination, destination_label) = match owned_cell {
        OwnedCellRef::Interior { editor_id } => {
            let destination_label = format!("interior '{editor_id}'");
            (
                TransitionDestination::Interior {
                    editor_id,
                    masters,
                    esm_path,
                },
                destination_label,
            )
        }
        OwnedCellRef::Exterior { worldspace, grid } => {
            let destination_label = format!("exterior '{worldspace}' ({},{})", grid.0, grid.1);
            (
                TransitionDestination::Exterior {
                    worldspace,
                    grid,
                    masters,
                    esm_path,
                },
                destination_label,
            )
        }
    };

    let mut slot = world
        .try_resource_mut::<PendingCellTransitionSlot>()
        .ok_or(QueueDoorTransitionError::MissingTransitionSlot)?;
    slot.0 = Some(PendingCellTransition {
        destination,
        source_refr_form_id: 0,
        destination_position_zup: door.position_zup,
        destination_rotation_zup: door.rotation_zup,
    });

    Ok(QueuedDoorTransition {
        destination_label,
        destination_form_id: door.destination_form_id,
    })
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
///
/// #2435 / COORD-2 — this used to call the plain (non-dispatcher)
/// `euler_zup_to_quat_yup`, contradicting this doc comment. Zero
/// effect at the shipping default (`--rotation-mode 1` ≡ the plain
/// helper's formula), but under `--rotation-mode 0/2/3` the player
/// would land at a door with an orientation from a DIFFERENT
/// convention than the surrounding geometry — exactly the scenario
/// the diagnostic flag exists to triage, and XTEL was the one REFR-
/// family site that couldn't be triaged with it.
pub fn rotation_zup_to_yup_quat(rot: [f32; 3]) -> Quat {
    super::euler_zup_to_quat_yup_refr(rot[0], rot[1], rot[2])
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
/// Used by [`InteriorCellApply`] (the resumable interior-load job
/// `app_step.rs` drives), the App-level Interior→Exterior path (which
/// needs to drop the interior before spinning up the streaming state),
/// the `cell.load` debug command, and the M45.1 live-load apply in
/// `save_io.rs` — six call sites in all. #3885: this previously named
/// the synchronous `load_interior_cell`, which had already been
/// superseded and is now deleted.
pub fn unload_current_interior(
    world: &mut byroredux_core::ecs::World,
    ctx: &mut byroredux_renderer::VulkanContext,
) {
    let prev_root = world.try_resource::<CurrentCellRoot>().and_then(|r| r.0);
    if let Some(prev) = prev_root {
        log::info!("Transition: unloading prior interior cell (root {prev})");
        super::unload_cell(world, ctx, prev);
    }
    clear_current_interior_identity(world);
}

/// Clear both pieces of interior identity as one state transition. Exterior
/// streaming has no single-cell context, so retaining either resource would
/// make a later save claim it belongs to the departed interior (#3021).
fn clear_current_interior_identity(world: &mut byroredux_core::ecs::World) {
    world.insert_resource(CurrentCellRoot(None));
    world.remove_resource::<CurrentCellContext>();
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
/// inside [`finish_interior_cell_load`], which [`InteriorCellApply`]
/// calls on the last step). Grouped to keep the argument count down.
pub struct InteriorCellRequest<'a> {
    pub editor_id: &'a str,
    pub masters: &'a [String],
    pub esm_path: &'a str,
    pub dest_pos_zup: [f32; 3],
    pub dest_rot_zup: [f32; 3],
}

/// App-owned state for a resumable interior door transition.
///
/// The providers must live for the whole apply, not just the frame that
/// parses the destination. Keeping them beside the job also prevents a
/// partially-loaded cell from borrowing a provider that the next frame has
/// already dropped.
pub(crate) struct InteriorCellApply {
    pub(crate) job: super::load::InteriorCellApplyJob,
    pub(crate) tex_provider: TextureProvider,
    pub(crate) mat_provider: MaterialProvider,
    pub(crate) dest_pos_zup: [f32; 3],
    pub(crate) dest_rot_zup: [f32; 3],
    pub(crate) dest_label: String,
}

pub(crate) enum InteriorCellApplyProgress {
    Pending(InteriorCellApply),
    Complete { dest_label: String, cam_pos: Vec3 },
}

impl InteriorCellApply {
    pub(crate) fn begin(
        world: &mut byroredux_core::ecs::World,
        ctx: &mut byroredux_renderer::VulkanContext,
        tex_provider: TextureProvider,
        mut mat_provider: MaterialProvider,
        request: InteriorCellRequest<'_>,
        dest_label: String,
    ) -> anyhow::Result<Self> {
        let InteriorCellRequest {
            editor_id,
            masters,
            esm_path,
            dest_pos_zup,
            dest_rot_zup,
        } = request;
        let job = super::load::InteriorCellApplyJob::begin(
            masters,
            esm_path,
            editor_id,
            world,
            ctx,
            &tex_provider,
            Some(&mut mat_provider),
        )?;
        Ok(Self {
            job,
            tex_provider,
            mat_provider,
            dest_pos_zup,
            dest_rot_zup,
            dest_label,
        })
    }

    pub(crate) fn advance(
        self,
        world: &mut byroredux_core::ecs::World,
        ctx: &mut byroredux_renderer::VulkanContext,
        budget: &mut super::FrameTimeBudget,
    ) -> InteriorCellApplyProgress {
        let Self {
            job,
            tex_provider,
            mut mat_provider,
            dest_pos_zup,
            dest_rot_zup,
            dest_label,
        } = self;
        match job.advance(world, ctx, &tex_provider, Some(&mut mat_provider), budget) {
            super::load::InteriorCellApplyProgress::Pending(job) => {
                InteriorCellApplyProgress::Pending(Self {
                    job,
                    tex_provider,
                    mat_provider,
                    dest_pos_zup,
                    dest_rot_zup,
                    dest_label,
                })
            }
            super::load::InteriorCellApplyProgress::Complete(result) => {
                let cam_pos = finish_interior_cell_load(
                    world,
                    result,
                    position_zup_to_yup(dest_pos_zup),
                    rotation_zup_to_yup_quat(dest_rot_zup),
                );
                InteriorCellApplyProgress::Complete {
                    dest_label,
                    cam_pos,
                }
            }
        }
    }

    pub(crate) fn cancel(
        self,
        world: &mut byroredux_core::ecs::World,
        ctx: &mut byroredux_renderer::VulkanContext,
    ) {
        self.job.cancel(world, ctx);
    }
}

/// Apply the non-ECS-camera side effects shared by synchronous and resumable
/// interior loads after the reference phase has completed.
pub(crate) fn finish_interior_cell_load(
    world: &mut byroredux_core::ecs::World,
    result: super::load::CellLoadResult,
    dest_pos: Vec3,
    dest_rot: Quat,
) -> Vec3 {
    world.insert_resource(result.phases);
    super::apply_interior_cell_lighting(world, result.lighting.as_ref());
    world.insert_resource(result.region_ambient);
    reposition_camera(world, dest_pos, dest_rot);
    crate::systems::ground_character_body_at(world, dest_pos);
    dest_pos
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

    #[test]
    fn clearing_interior_identity_removes_saved_cell_context() {
        let mut world = World::new();
        world.insert_resource(CurrentCellRoot(Some(7)));
        world.insert_resource(CurrentCellContext {
            cell_editor_id: "Vault21".into(),
            esm_path: "FalloutNV.esm".into(),
            masters: Vec::new(),
        });

        clear_current_interior_identity(&mut world);

        assert_eq!(world.resource::<CurrentCellRoot>().0, None);
        assert!(world.try_resource::<CurrentCellContext>().is_none());
    }

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

    /// #2435 / COORD-2 regression — `rotation_zup_to_yup_quat`'s doc
    /// comment promises it wraps `euler_zup_to_quat_yup_refr` (the
    /// `--rotation-mode` dispatcher every other REFR-family site uses),
    /// but it used to call the plain, non-dispatcher `euler_zup_to_quat_yup`
    /// instead. A value-based test can't discriminate the two at the
    /// shipping default (`--rotation-mode 1`'s formula is bit-identical
    /// to the plain helper's), and mutating the dispatcher's shared
    /// `AtomicU8` mode from a test would race every other test in this
    /// crate that exercises REFR placement concurrently — so this is a
    /// static source check, the same shape this codebase already uses
    /// for logic a value-based unit test can't safely pin.
    #[test]
    fn rotation_zup_to_yup_quat_calls_the_refr_dispatcher_not_the_plain_helper() {
        let src = include_str!("transition.rs");
        let fn_start = src
            .find("pub fn rotation_zup_to_yup_quat")
            .expect("rotation_zup_to_yup_quat must still exist");
        let fn_body = &src[fn_start..fn_start + 400.min(src.len() - fn_start)];
        assert!(
            fn_body.contains("euler_zup_to_quat_yup_refr("),
            "rotation_zup_to_yup_quat must call the `--rotation-mode` \
             dispatcher (euler_zup_to_quat_yup_refr), matching its own doc \
             comment and every other REFR-family call site — calling the \
             plain euler_zup_to_quat_yup instead silently opts XTEL \
             transitions out of `--rotation-mode` triage (#2435)"
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

    #[test]
    fn queue_door_transition_reports_missing_prerequisites() {
        let mut world = World::new();
        let entity = world.spawn();
        assert_eq!(
            queue_door_transition(&world, entity),
            Err(QueueDoorTransitionError::MissingDoor(entity))
        );

        world.insert(
            entity,
            DoorTeleport {
                destination_form_id: 0x1234,
                position_zup: [0.0; 3],
                rotation_zup: [0.0; 3],
            },
        );
        assert_eq!(
            queue_door_transition(&world, entity),
            Err(QueueDoorTransitionError::MissingCellIndex)
        );

        // #3415 — the positive counterpart. Once the index is present the
        // door gets PAST the prerequisite gate and fails on the destination
        // lookup instead, which is what an exterior door did not manage to
        // do before the exterior boot arm learned to install the resource.
        world.insert_resource(super::super::LoadedCellIndex(std::sync::Arc::new(
            byroredux_plugin::esm::records::EsmIndex::default(),
        )));
        assert_eq!(
            queue_door_transition(&world, entity),
            Err(QueueDoorTransitionError::DestinationNotFound(0x1234)),
            "with the cell index installed the prerequisite gate must be \
             satisfied — the empty index then legitimately has no such REFR"
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
