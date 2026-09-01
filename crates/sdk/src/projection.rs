//! Immutable, bounded views of live engine entities.

use std::collections::BTreeMap;

use thiserror::Error;

use crate::actor_values::{ActorValueState, MAX_ACTOR_VALUES_PER_ENTITY};
use crate::factions::FactionSnapshot;
use crate::identity::{EntityRef, FormRef};
use crate::inventory::InventorySnapshot;

/// Maximum UTF-8 bytes exposed for one entity display name.
pub const MAX_ENTITY_NAME_BYTES: usize = 1_024;

/// Renderer-neutral world transform exposed to extensions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldTransform {
    translation: [f32; 3],
    rotation: [f32; 4],
    scale: f32,
}

impl WorldTransform {
    /// Construct a finite transform. Rotation uses `(x, y, z, w)` order.
    pub fn new(
        translation: [f32; 3],
        rotation: [f32; 4],
        scale: f32,
    ) -> Result<Self, ProjectionError> {
        if translation
            .iter()
            .chain(rotation.iter())
            .chain(std::iter::once(&scale))
            .any(|value| !value.is_finite())
        {
            return Err(ProjectionError::NonFiniteTransform);
        }
        Ok(Self {
            translation,
            rotation,
            scale,
        })
    }

    pub const fn translation(&self) -> [f32; 3] {
        self.translation
    }

    pub const fn rotation(&self) -> [f32; 4] {
        self.rotation
    }

    pub const fn scale(&self) -> f32 {
        self.scale
    }
}

/// One callback-local, read-only entity view.
///
/// Hosts replace these snapshots at callback boundaries. A projection never
/// grants authority to mutate the entity and never contains an ECS slot or
/// engine pointer.
#[derive(Clone, Debug, PartialEq)]
pub struct EntityProjection {
    entity: EntityRef,
    form: Option<FormRef>,
    name: Option<String>,
    world_transform: Option<WorldTransform>,
    actor_values: Option<BTreeMap<FormRef, ActorValueState>>,
    inventory: Option<InventorySnapshot>,
    factions: Option<FactionSnapshot>,
}

impl EntityProjection {
    pub fn new(
        entity: EntityRef,
        form: Option<FormRef>,
        name: Option<String>,
        world_transform: Option<WorldTransform>,
    ) -> Result<Self, ProjectionError> {
        if let Some(name) = &name {
            if name.len() > MAX_ENTITY_NAME_BYTES {
                return Err(ProjectionError::NameTooLarge {
                    actual: name.len(),
                    maximum: MAX_ENTITY_NAME_BYTES,
                });
            }
        }
        Ok(Self {
            entity,
            form,
            name,
            world_transform,
            actor_values: None,
            inventory: None,
            factions: None,
        })
    }

    /// Attach a bounded snapshot of canonical actor values. `Some(empty)`
    /// still identifies an actor carrying the component.
    pub fn with_actor_values(
        mut self,
        values: impl IntoIterator<Item = (FormRef, ActorValueState)>,
    ) -> Result<Self, ProjectionError> {
        let actor_values = values.into_iter().collect::<BTreeMap<_, _>>();
        if actor_values.len() > MAX_ACTOR_VALUES_PER_ENTITY {
            return Err(ProjectionError::ActorValueBudgetExceeded {
                maximum: MAX_ACTOR_VALUES_PER_ENTITY,
            });
        }
        self.actor_values = Some(actor_values);
        Ok(self)
    }

    pub const fn entity(&self) -> EntityRef {
        self.entity
    }

    pub const fn form(&self) -> Option<FormRef> {
        self.form
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub const fn world_transform(&self) -> Option<WorldTransform> {
        self.world_transform
    }

    pub fn actor_values(&self) -> Option<&BTreeMap<FormRef, ActorValueState>> {
        self.actor_values.as_ref()
    }

    pub fn actor_value(&self, actor_value: FormRef) -> Option<ActorValueState> {
        self.actor_values.as_ref()?.get(&actor_value).copied()
    }

    /// Attach a complete or explicitly truncated inventory summary.
    pub fn with_inventory(mut self, inventory: InventorySnapshot) -> Self {
        self.inventory = Some(inventory);
        self
    }

    pub fn inventory(&self) -> Option<&InventorySnapshot> {
        self.inventory.as_ref()
    }

    pub fn with_factions(mut self, factions: FactionSnapshot) -> Self {
        self.factions = Some(factions);
        self
    }

    pub fn factions(&self) -> Option<&FactionSnapshot> {
        self.factions.as_ref()
    }
}

/// Rejection while constructing a bounded projection.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ProjectionError {
    #[error("entity name contains {actual} bytes, exceeding the limit of {maximum}")]
    NameTooLarge { actual: usize, maximum: usize },
    #[error("entity transform contains a non-finite value")]
    NonFiniteTransform,
    #[error("actor-value snapshot exceeds the per-entity limit of {maximum}")]
    ActorValueBudgetExceeded { maximum: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projections_reject_unbounded_names_and_non_finite_transforms() {
        let entity = EntityRef::new(1, 1).unwrap();
        assert!(matches!(
            EntityProjection::new(
                entity,
                None,
                Some("x".repeat(MAX_ENTITY_NAME_BYTES + 1)),
                None,
            ),
            Err(ProjectionError::NameTooLarge { .. })
        ));
        assert_eq!(
            WorldTransform::new([f32::NAN, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0),
            Err(ProjectionError::NonFiniteTransform)
        );
    }

    #[test]
    fn projection_preserves_portable_identity_and_finite_pose() {
        let entity = EntityRef::new(2, 3).unwrap();
        let form = FormRef::new([7; 16], 42);
        let transform = WorldTransform::new([1.0, 2.0, 3.0], [0.0, 0.0, 0.0, 1.0], 2.0).unwrap();
        let projection =
            EntityProjection::new(entity, Some(form), Some("Door".to_owned()), Some(transform))
                .unwrap();
        assert_eq!(projection.entity(), entity);
        assert_eq!(projection.form(), Some(form));
        assert_eq!(projection.name(), Some("Door"));
        assert_eq!(projection.world_transform(), Some(transform));
    }
}
