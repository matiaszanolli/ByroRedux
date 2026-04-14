//! BSXFlags and BSBound components — NIF-embedded extra data.
//!
//! BSXFlags: physics/animation hints from the NIF's root extra data.
//! BSBound: object-level bounding box from the NIF's root extra data.
//!
//! Both are SparseSetStorage: only a fraction of entities have them.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;

/// BSXFlags extra data — physics and animation hints per NIF.
///
/// Bit flags from the NIF's BSXFlags block (always on the root NiNode):
/// - Bit 0: Animated
/// - Bit 1: Havok (has physics collision)
/// - Bit 2: Ragdoll
/// - Bit 3: Complex (multi-shape collision)
/// - Bit 4: Addon node
/// - Bit 5: Editor marker
/// - Bit 6: Dynamic (not static — can move at runtime)
/// - Bit 7: Articulated (multi-body Havok)
/// - Bit 8: Needs transform updates
/// - Bit 9: External emit
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct BSXFlags(pub u32);

impl BSXFlags {
    pub const ANIMATED: u32 = 1 << 0;
    pub const HAVOK: u32 = 1 << 1;
    pub const RAGDOLL: u32 = 1 << 2;
    pub const COMPLEX: u32 = 1 << 3;
    pub const ADDON: u32 = 1 << 4;
    pub const EDITOR_MARKER: u32 = 1 << 5;
    pub const DYNAMIC: u32 = 1 << 6;
    pub const ARTICULATED: u32 = 1 << 7;
    pub const NEEDS_TRANSFORM_UPDATES: u32 = 1 << 8;
    pub const EXTERNAL_EMIT: u32 = 1 << 9;

    pub fn has_havok(self) -> bool {
        self.0 & Self::HAVOK != 0
    }

    pub fn is_dynamic(self) -> bool {
        self.0 & Self::DYNAMIC != 0
    }

    pub fn is_animated(self) -> bool {
        self.0 & Self::ANIMATED != 0
    }

    pub fn is_editor_marker(self) -> bool {
        self.0 & Self::EDITOR_MARKER != 0
    }
}

impl Component for BSXFlags {
    type Storage = SparseSetStorage<Self>;
}

/// BSBound extra data — object-level bounding box.
///
/// Center and half-extents in the NIF's local space (Z-up in the NIF,
/// converted to Y-up during import for consistency with the renderer).
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct BSBound {
    /// Center of the bounding box (Y-up, world-space after transform).
    pub center: [f32; 3],
    /// Half-extents along each axis.
    pub half_extents: [f32; 3],
}

impl Component for BSBound {
    type Storage = SparseSetStorage<Self>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bsx_flags_bits() {
        let flags = BSXFlags(0b0000_0110); // HAVOK | RAGDOLL
        assert!(flags.has_havok());
        assert!(!flags.is_dynamic());
        assert!(!flags.is_animated());
    }

    #[test]
    fn bsx_flags_dynamic() {
        let flags = BSXFlags(BSXFlags::DYNAMIC | BSXFlags::ANIMATED);
        assert!(flags.is_dynamic());
        assert!(flags.is_animated());
        assert!(!flags.has_havok());
    }
}
