//! M47.2 — location ref types scripts assigned: `ObjectReference.SetLocRefType(
//! akLoc, akRefType)` ("Sets an in-game created object to be the given loc ref
//! type for the given location"; a `None` ref type "clear[s] an already set ref
//! type on a created ref").
//!
//! A placement's authored ref types are its `XLRT` tags
//! ([`crate::scene::SceneAliasCandidate::location_ref_types`]); a script's
//! assignment adds to them, and the clear drops every type a script assigned
//! to that reference. The CK scopes the type to a location; the lookups here
//! (`HasLocRefType`, the alias fill by ref type) ask about the reference's own
//! types, so the location is not part of the key. Keyed by the reference's
//! FormID so it outlives the cell that despawns the entity. Saved.

use std::collections::BTreeMap;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;

/// Every ref type a script assigned, by reference FormID.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct LocRefTypeOverrides {
    types: BTreeMap<u32, Vec<u32>>,
}

impl Resource for LocRefTypeOverrides {}

/// #5802 — this subsystem's world setup, owned here instead of the crate's
/// `register` hub: the scripted SetLocRefType assignments (saved).
pub fn install(world: &mut World) {
    world.init_resource::<LocRefTypeOverrides>();
}

/// #5802 — this subsystem's saved column. Called by the binary's
/// `build_save_registry` at its place in the row order (the schema
/// fingerprint is order-sensitive).
#[cfg(feature = "save")]
pub fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    registry.register_resource::<LocRefTypeOverrides>("LocRefTypeOverrides");
}

impl LocRefTypeOverrides {
    /// Give `reference` the ref type `ref_type`; `0` (a `None` argument)
    /// clears every type scripts gave it.
    pub fn set(&mut self, reference: u32, ref_type: u32) {
        if ref_type == 0 {
            self.types.remove(&reference);
            return;
        }
        let types = self.types.entry(reference).or_default();
        if !types.contains(&ref_type) {
            types.push(ref_type);
        }
    }

    /// The types scripts gave `reference`.
    pub fn of(&self, reference: u32) -> &[u32] {
        self.types.get(&reference).map_or(&[], Vec::as_slice)
    }

    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }
}

/// Whether the reference with FormID `reference` has `ref_type`: an authored
/// `XLRT` tag on `entity`'s placement, or a type a script gave it.
pub fn has_loc_ref_type(world: &World, entity: EntityId, reference: u32, ref_type: u32) -> bool {
    world
        .try_resource::<LocRefTypeOverrides>()
        .is_some_and(|overrides| overrides.of(reference).contains(&ref_type))
        || world
            .get::<crate::scene::SceneAliasCandidate>(entity)
            .is_some_and(|candidate| candidate.location_ref_types.contains(&ref_type))
}

/// Add the types scripts assigned to the alias candidates a refresh collected.
pub(crate) fn apply_to_candidates(
    world: &World,
    candidates: &mut [(EntityId, crate::scene::SceneAliasCandidate)],
) {
    let Some(overrides) = world.try_resource::<LocRefTypeOverrides>() else {
        return;
    };
    if overrides.is_empty() {
        return;
    }
    for (_, candidate) in candidates.iter_mut() {
        for &ref_type in overrides.of(candidate.reference_form_id) {
            if !candidate.location_ref_types.contains(&ref_type) {
                candidate.location_ref_types.push(ref_type);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_none_ref_type_clears_what_scripts_assigned() {
        let mut types = LocRefTypeOverrides::default();
        types.set(7, 100);
        types.set(7, 100);
        types.set(7, 101);
        assert_eq!(types.of(7), &[100, 101]);
        types.set(7, 0);
        assert!(types.of(7).is_empty() && types.is_empty());
        assert!(types.of(8).is_empty());
    }
}
