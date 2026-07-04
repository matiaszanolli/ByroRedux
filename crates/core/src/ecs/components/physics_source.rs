//! Diagnostic-only physics-entity → REFR backlink.
//!
//! Some physics bodies are spawned as standalone entities decoupled from
//! their placement's render hierarchy — e.g. `bhk`-authored collision
//! shapes (`byroredux::cell_loader::spawn::spawn_placed_instances`), which
//! become bare entities carrying only `Transform` / `GlobalTransform` /
//! `CollisionShape` / `RigidBodyData`. Runtime diagnostics (e.g. #1698's
//! awake-faller dump) need a way to resolve such a body back to the REFR
//! it came from.
//!
//! [`PhysicsSourceForm`] is deliberately NOT [`FormIdComponent`](super::FormIdComponent):
//! that component backs [`World::find_by_form_id`](crate::ecs::World::find_by_form_id)
//! (console `prid`, Papyrus `ObjectReference` resolution), which returns
//! the *first* entity carrying a given form id and assumes at most one
//! canonical entity per id. A REFR's compound bhk shape can spawn several
//! collision entities sharing its placement's form id — attaching
//! `FormIdComponent` to all of them would make that lookup pick an
//! arbitrary collision proxy instead of the placement root that actually
//! carries `Name` / `MeshHandle` / console-facing state. This component
//! is read only by diagnostics, never by `find_by_form_id`.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;
use crate::form_id::FormId;

/// The [`FormId`] of the REFR placement that spawned this physics-only
/// entity. Resolve through [`FormIdPool`](crate::form_id::FormIdPool) to
/// get the stable [`FormIdPair`](crate::form_id::FormIdPair).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicsSourceForm(pub FormId);

impl Component for PhysicsSourceForm {
    type Storage = SparseSetStorage<Self>;
}
