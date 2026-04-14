//! Billboard component — entities that orient themselves to face the camera.
//!
//! Mirrors Gamebryo's NiBillboardNode. Attach to an entity that also has a
//! [`Transform`](super::Transform) + [`GlobalTransform`](super::GlobalTransform).
//! A dedicated system updates the entity's world rotation each frame so the
//! configured facing axis points at the active camera.
//!
//! Billboard mode comes straight from the NiBillboardNode block
//! (`docs/legacy/nif.xml` — `BillboardMode` enum). Pre-10.1.0.0 NIFs packed
//! the mode into NiAVObject flags bits 5–6; the NIF importer normalizes both
//! layouts into [`BillboardMode`] before attaching this component.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;

/// How a billboard rotates to face the camera.
///
/// Values mirror `BillboardMode` in `nif.xml` — do not renumber without
/// updating the NIF importer's mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum BillboardMode {
    /// Align billboard and camera forward vector with minimized rotation.
    AlwaysFaceCamera = 0,
    /// Align with camera forward, but allow free rotation around the up axis.
    RotateAboutUp = 1,
    /// Align with camera forward, non-minimized rotation.
    RigidFaceCamera = 2,
    /// Forward vector always points at camera center, minimized rotation.
    AlwaysFaceCenter = 3,
    /// Forward vector always points at camera center, non-minimized rotation.
    RigidFaceCenter = 4,
    /// Rotate only around the billboard's local Z axis (stays in local X-Y plane).
    BsRotateAboutUp = 5,
    /// Rotate only around the world up axis (BGSM variant).
    RotateAboutUp2 = 9,
}

impl BillboardMode {
    /// Convert a raw NIF `BillboardMode` value to the enum. Unknown values
    /// fall back to [`BillboardMode::AlwaysFaceCamera`] (the Gamebryo default).
    pub const fn from_nif(raw: u16) -> Self {
        // Vanilla Oblivion NIFs use values > 7 — nif.xml documents only
        // 0-5 and 9. Use `.0x7` masking logic for unknowns per nif.xml hint.
        match raw {
            0 => Self::AlwaysFaceCamera,
            1 => Self::RotateAboutUp,
            2 => Self::RigidFaceCamera,
            3 => Self::AlwaysFaceCenter,
            4 => Self::RigidFaceCenter,
            5 => Self::BsRotateAboutUp,
            9 => Self::RotateAboutUp2,
            _ => Self::AlwaysFaceCamera,
        }
    }

    /// True if the billboard is constrained to rotate around an up axis only
    /// (pitch is locked to the local frame).
    pub const fn locks_pitch(self) -> bool {
        matches!(
            self,
            Self::RotateAboutUp | Self::BsRotateAboutUp | Self::RotateAboutUp2
        )
    }
}

/// Marks an entity as a camera-facing billboard.
///
/// The billboard system (`billboard_system`) overwrites the entity's
/// `GlobalTransform` rotation each frame based on the active camera's
/// world position, applying the rule described by [`BillboardMode`].
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct Billboard {
    pub mode: BillboardMode,
}

impl Billboard {
    pub const fn new(mode: BillboardMode) -> Self {
        Self { mode }
    }
}

impl Component for Billboard {
    type Storage = SparseSetStorage<Self>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_nif_maps_known_modes() {
        assert_eq!(BillboardMode::from_nif(0), BillboardMode::AlwaysFaceCamera);
        assert_eq!(BillboardMode::from_nif(1), BillboardMode::RotateAboutUp);
        assert_eq!(BillboardMode::from_nif(5), BillboardMode::BsRotateAboutUp);
        assert_eq!(BillboardMode::from_nif(9), BillboardMode::RotateAboutUp2);
    }

    #[test]
    fn from_nif_unknown_falls_back() {
        assert_eq!(BillboardMode::from_nif(42), BillboardMode::AlwaysFaceCamera);
        assert_eq!(BillboardMode::from_nif(255), BillboardMode::AlwaysFaceCamera);
    }

    #[test]
    fn pitch_lock_matches_nif_xml() {
        assert!(BillboardMode::RotateAboutUp.locks_pitch());
        assert!(BillboardMode::BsRotateAboutUp.locks_pitch());
        assert!(BillboardMode::RotateAboutUp2.locks_pitch());
        assert!(!BillboardMode::AlwaysFaceCamera.locks_pitch());
        assert!(!BillboardMode::RigidFaceCamera.locks_pitch());
    }
}
