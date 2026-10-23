//! M47.2 — Starfield's scanned references: `ObjectReference.SetScanned(abValue)`,
//! `IsScanned()` and `AddToMapScanned(abAllowFastTravel)`.
//!
//! There is no CK page for Starfield, but the game's own `ObjectReference.pex`
//! shows what the composite does — `AddToMapScanned(b)` is
//! `Self.SetScanned(true)` then `Self.AddToMap(b)` — and xEdit's condition
//! table describes the state: `IsScanned` is "check if a object has been
//! scanned". The state is a per-reference flag a script sets; scanning with the
//! scanner (which would set it too) has no player-side runtime yet. Saved.

use std::collections::BTreeSet;

use byroredux_core::ecs::resource::Resource;

/// The references scripts marked scanned (and not later cleared).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ScannedReferences {
    scanned: BTreeSet<u32>,
}

impl Resource for ScannedReferences {}

impl ScannedReferences {
    pub fn is_scanned(&self, reference: u32) -> bool {
        self.scanned.contains(&reference)
    }

    pub fn set(&mut self, reference: u32, scanned: bool) {
        if scanned {
            self.scanned.insert(reference);
        } else {
            self.scanned.remove(&reference);
        }
    }
}

/// #5802 — this subsystem's world setup, owned here instead of the crate's
/// `register` hub: Starfield `SetScanned` / `AddToMapScanned` flags (saved).
pub fn install(world: &mut byroredux_core::ecs::world::World) {
    world.init_resource::<ScannedReferences>();
}

/// #5802 — this subsystem's saved column. Called by the binary's
/// `build_save_registry` at its place in the row order (the schema
/// fingerprint is order-sensitive).
#[cfg(feature = "save")]
pub fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    registry.register_resource::<ScannedReferences>("ScannedReferences");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scanned_is_set_and_cleared_per_reference() {
        let mut scanned = ScannedReferences::default();
        assert!(!scanned.is_scanned(9));
        scanned.set(9, true);
        assert!(scanned.is_scanned(9));
        assert!(!scanned.is_scanned(10));
        scanned.set(9, false);
        assert!(!scanned.is_scanned(9));
    }
}
