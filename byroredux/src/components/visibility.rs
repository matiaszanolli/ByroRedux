//! Render-skip markers (#6714): the three components the render collector reads to keep a
//! mesh entity out of a frame. Moved out of `inventory`, `player_body` and
//! `npc_spawn::loot_appearance`; each old path re-exports its marker.

use byroredux_core::ecs::{Component, SparseSetStorage};

/// Marker on a placement whose item the player already picked up. Keeps the
/// entity resident-but-hidden (render + interaction skip it) and re-parks a
/// `picked_up` tombstone on cell eviction so a respawned copy stays gone.
/// Never serialized itself — the tombstone row is the durable half.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PickedUp;
impl Component for PickedUp {
    type Storage = SparseSetStorage<Self>;
}

/// Marker on each mesh entity under the player body root while the camera is
/// first-person. Consumed by the render collection passes beside
/// `NpcAppearanceHidden`/`PickedUp` (skinned palettes + static draw loop).
/// Kept as a component rather than a per-frame resource consult so the two
/// render sites pay exactly the per-mesh query cost they already do, and so
/// equip-event hides (`NpcAppearanceHidden`) never fight the view toggle —
/// the two markers compose.
#[derive(Debug, Clone, Copy)]
pub(crate) struct HiddenFirstPerson;

impl Component for HiddenFirstPerson {
    type Storage = SparseSetStorage<Self>;
}

/// Separate from animation visibility: a NIF controller must not reveal a
/// staged body part or superseded outfit. Applied only to mesh entities.
pub(crate) struct NpcAppearanceHidden;
impl Component for NpcAppearanceHidden {
    type Storage = SparseSetStorage<Self>;
}
