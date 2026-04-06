//! Scene graph flags component — visibility and update control.
//!
//! PackedStorage: checked per-frame by culling and update systems.
//! Equivalent to Gamebryo's NiAVObject::m_uFlags.

use crate::ecs::packed::PackedStorage;
use crate::ecs::storage::Component;

/// Scene graph visibility and update control flags.
///
/// Mirrors Gamebryo's NiAVObject flags field. The most important flag
/// is `APP_CULLED` which hides an entity from rendering without removing it.
///
/// Flag bits match the Gamebryo v3.2 enum for compatibility with NIF import.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneFlags(pub u32);

impl SceneFlags {
    // ── Flag constants (from Gamebryo NiAVObject.h) ─────────────────

    /// Application-level culling. When set, the entity is hidden from rendering.
    /// This is the primary visibility control flag.
    pub const APP_CULLED: u32 = 0x0001;

    /// Selective update enabled (optimization — skip Update when nothing changed).
    pub const SELECTIVE_UPDATE: u32 = 0x0002;

    /// Selective transform updates enabled.
    pub const SELECTIVE_XFORMS: u32 = 0x0004;

    /// Selective property controller updates enabled.
    pub const SELECTIVE_PROP_CONTROLLER: u32 = 0x0008;

    /// Selective rigid body updates enabled.
    pub const SELECTIVE_RIGID: u32 = 0x0010;

    /// Display object flag (occlusion culling system).
    pub const DISPLAY_OBJECT: u32 = 0x0020;

    /// Disable sorting for this object (always render in submission order).
    pub const DISABLE_SORTING: u32 = 0x0040;

    /// Override selective transform updates.
    pub const SELECTIVE_XFORMS_OVERRIDE: u32 = 0x0080;

    /// Object is a node (NiNode) rather than a leaf (NiGeometry).
    pub const IS_NODE: u32 = 0x0100;

    // ── Convenience methods ─────────────────────────────────────────

    /// Create with default flags (visible, not culled).
    pub const fn visible() -> Self {
        Self(0)
    }

    /// Create from raw NIF flags value.
    pub const fn from_nif(flags: u32) -> Self {
        Self(flags)
    }

    /// Check if the entity is application-culled (hidden).
    pub fn is_culled(self) -> bool {
        self.0 & Self::APP_CULLED != 0
    }

    /// Set or clear the APP_CULLED flag.
    pub fn set_culled(&mut self, culled: bool) {
        if culled {
            self.0 |= Self::APP_CULLED;
        } else {
            self.0 &= !Self::APP_CULLED;
        }
    }

    /// Check if this is a node (vs leaf geometry).
    pub fn is_node(self) -> bool {
        self.0 & Self::IS_NODE != 0
    }

    /// Check if sorting is disabled.
    pub fn sorting_disabled(self) -> bool {
        self.0 & Self::DISABLE_SORTING != 0
    }
}

impl Default for SceneFlags {
    fn default() -> Self {
        Self::visible()
    }
}

impl Component for SceneFlags {
    type Storage = PackedStorage<Self>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_visible() {
        let f = SceneFlags::default();
        assert!(!f.is_culled());
        assert_eq!(f.0, 0);
    }

    #[test]
    fn from_nif_flags() {
        let f = SceneFlags::from_nif(0x0101); // IS_NODE | APP_CULLED
        assert!(f.is_culled());
        assert!(f.is_node());
    }

    #[test]
    fn set_culled() {
        let mut f = SceneFlags::visible();
        assert!(!f.is_culled());
        f.set_culled(true);
        assert!(f.is_culled());
        f.set_culled(false);
        assert!(!f.is_culled());
    }

    #[test]
    fn disable_sorting() {
        let f = SceneFlags::from_nif(SceneFlags::DISABLE_SORTING);
        assert!(f.sorting_disabled());
        assert!(!f.is_culled());
    }
}
