//! Authored spawn pose for a direct interior load — the engine's `coc`.
//!
//! A door walk already lands on authored data: the transition path places the
//! player on the destination door's `XTEL` pose (`transition.rs`). A direct
//! load (`--cell`, the `coc` equivalent) has no door context, and used to fall
//! back to the first door's *own* placement, which sits in the door frame —
//! half the time on the far side of the wall — with the fly camera then parked
//! a fixed `(0, 100, 200)` BU off it, the loose-NIF framing offset.
//!
//! Vanilla `coc` does not guess. Per the Creation Kit wiki's `Markers` page:
//! *"COCMarkerHeading: The location you will be moved to when using the COC
//! Console Command. If the specified cell does not have a COCMarkerHeading,
//! you will be placed in the 'center' of the cell."* The marker is an ordinary
//! REFR of a base STAT whose EDID is `COCMarkerHeading`, placed at floor level
//! with its heading in the REFR rotation — the same shape as an `XTEL` pose.
//!
//! Ladder, first hit wins:
//! 1. the cell's `COCMarkerHeading` REFR (vanilla `coc`);
//! 2. the arrival pose of the first door whose partner links back to it — the
//!    partner's own `XTEL`, i.e. exactly where a player walking in through that
//!    door is placed;
//! 3. `None` — the caller keeps the REFR loader's door-placement / bbox
//!    centroid fallback (`references::complete`).
//!
//! Rung 2 replaces vanilla's "center of the cell", which has no walkability
//! guarantee; it is a door-entry pose the game itself authored for this cell.

use byroredux_core::math::{Quat, Vec3};
use byroredux_plugin::esm::cell::{EsmCellIndex, PlacedRef};

use super::transition::{position_zup_to_yup, rotation_zup_to_yup_quat};

/// EDID of the base STAT vanilla `coc` places the player on. Matched
/// case-insensitively, as every EDID lookup in the loader is.
const COC_MARKER_EDITOR_ID: &str = "COCMarkerHeading";

/// Which rung of the ladder produced a [`SpawnPose`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnPoseSource {
    /// The cell's `COCMarkerHeading` REFR.
    CocMarker,
    /// A door partner's `XTEL` arrival pose into this cell.
    DoorArrival,
}

/// A floor-level pose authored by the game for placing the player.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpawnPose {
    /// Feet position, engine Y-up. Floor level by construction, like an
    /// `XTEL` destination.
    pub position: Vec3,
    /// Orientation, engine Y-up, converted with the same REFR dispatcher the
    /// `XTEL` arrival uses so both paths face the same way.
    pub rotation: Quat,
    pub source: SpawnPoseSource,
}

impl SpawnPose {
    fn from_zup(position: [f32; 3], rotation: [f32; 3], source: SpawnPoseSource) -> Self {
        Self {
            position: position_zup_to_yup(position),
            rotation: rotation_zup_to_yup_quat(rotation),
            source,
        }
    }

    /// Horizontal view direction. The player takes only the heading from a
    /// marker; any authored tilt is dropped rather than starting the camera
    /// pitched. Camera-forward is local `-Z`, the Y-up image of Bethesda's
    /// local `+Y` facing axis.
    pub fn forward(&self) -> Vec3 {
        let forward = self.rotation * -Vec3::Z;
        let flat = Vec3::new(forward.x, 0.0, forward.z);
        if flat.length_squared() > 1e-8 {
            flat.normalize()
        } else {
            -Vec3::Z
        }
    }

    pub fn label(&self) -> &'static str {
        match self.source {
            SpawnPoseSource::CocMarker => "COCMarkerHeading",
            SpawnPoseSource::DoorArrival => "door arrival (partner XTEL)",
        }
    }
}

/// Resolve the spawn point for a direct interior load: the authored pose when
/// the ladder finds one, else `fallback` (the REFR loader's door-placement /
/// bbox-centroid point). Shared by both interior load paths so they cannot
/// disagree about where `coc` lands.
pub(crate) fn resolve_interior_spawn(
    refs: &[PlacedRef],
    index: &EsmCellIndex,
    fallback: Vec3,
    cell_label: &str,
) -> (Vec3, Option<SpawnPose>) {
    let pose = authored_spawn_pose(refs, index);
    match pose {
        Some(pose) => log::info!(
            "Interior spawn for '{}': {} at ({:.1}, {:.1}, {:.1})",
            cell_label,
            pose.label(),
            pose.position.x,
            pose.position.y,
            pose.position.z,
        ),
        None => log::info!(
            "Interior spawn for '{}': no COCMarkerHeading or linked door — \
             REFR-loader fallback at ({:.1}, {:.1}, {:.1})",
            cell_label,
            fallback.x,
            fallback.y,
            fallback.z,
        ),
    }
    (pose.map_or(fallback, |pose| pose.position), pose)
}

/// Resolve the authored spawn pose for the interior whose placements are
/// `refs`. See the module doc for the ladder.
pub(crate) fn authored_spawn_pose(refs: &[PlacedRef], index: &EsmCellIndex) -> Option<SpawnPose> {
    coc_marker_pose(refs, index).or_else(|| door_arrival_pose(refs, index))
}

fn coc_marker_pose(refs: &[PlacedRef], index: &EsmCellIndex) -> Option<SpawnPose> {
    refs.iter()
        .find(|placed| {
            index
                .statics
                .get(&placed.base_form_id)
                .is_some_and(|base| base.editor_id.eq_ignore_ascii_case(COC_MARKER_EDITOR_ID))
        })
        .map(|marker| {
            SpawnPose::from_zup(marker.position, marker.rotation, SpawnPoseSource::CocMarker)
        })
}

fn door_arrival_pose(refs: &[PlacedRef], index: &EsmCellIndex) -> Option<SpawnPose> {
    refs.iter().find_map(|door| {
        let outbound = door.teleport.as_ref()?;
        let (_, partner) = index.locate_refr(outbound.destination)?;
        let inbound = partner.teleport.as_ref()?;
        // A one-way or mis-linked partner's XTEL may lead somewhere other
        // than this cell; only a reciprocal link is known to arrive here.
        (inbound.destination == door.form_id).then(|| {
            SpawnPose::from_zup(inbound.position, inbound.rotation, SpawnPoseSource::DoorArrival)
        })
    })
}
