//! Portable actor-value snapshots and deferred mutation commands.

use thiserror::Error;

use crate::identity::{EntityRef, FormRef};

/// Maximum actor-value entries exposed for one entity in one callback.
pub const MAX_ACTOR_VALUES_PER_ENTITY: usize = 1_024;
/// Defensive magnitude bound for one guest-authored actor-value operand.
pub const MAX_ABS_ACTOR_VALUE_OPERAND: f32 = 1_000_000_000.0;

/// Immutable canonical layers of one actor value.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ActorValueState {
    base: f32,
    permanent: f32,
    temporary: f32,
    damage: f32,
}

impl ActorValueState {
    pub fn new(
        base: f32,
        permanent: f32,
        temporary: f32,
        damage: f32,
    ) -> Result<Self, ActorValueError> {
        if [base, permanent, temporary, damage]
            .into_iter()
            .any(|value| !value.is_finite())
        {
            return Err(ActorValueError::NonFiniteValue);
        }
        let state = Self {
            base,
            permanent,
            temporary,
            damage,
        };
        if !state.current().is_finite() {
            return Err(ActorValueError::NonFiniteValue);
        }
        Ok(state)
    }

    pub fn current(self) -> f32 {
        self.base + self.permanent + self.temporary - self.damage
    }

    pub const fn base(self) -> f32 {
        self.base
    }

    pub const fn permanent(self) -> f32 {
        self.permanent
    }

    pub const fn temporary(self) -> f32 {
        self.temporary
    }

    pub const fn damage(self) -> f32 {
        self.damage
    }
}

/// Semantic mutation of one canonical actor-value layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActorValueOperation {
    SetBase,
    ModifyPermanent,
    ModifyTemporary,
    Damage,
    Restore,
}

/// One validated actor-value mutation emitted by a sandbox callback.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActorValueCommand {
    entity: EntityRef,
    actor_value: FormRef,
    operation: ActorValueOperation,
    value: f32,
}

impl ActorValueCommand {
    pub fn new(
        entity: EntityRef,
        actor_value: FormRef,
        operation: ActorValueOperation,
        value: f32,
    ) -> Result<Self, ActorValueError> {
        if actor_value.local() == 0 {
            return Err(ActorValueError::NullActorValue);
        }
        if !value.is_finite() || value.abs() > MAX_ABS_ACTOR_VALUE_OPERAND {
            return Err(ActorValueError::InvalidOperand);
        }
        if matches!(
            operation,
            ActorValueOperation::Damage | ActorValueOperation::Restore
        ) && value < 0.0
        {
            return Err(ActorValueError::NegativeMagnitude);
        }
        Ok(Self {
            entity,
            actor_value,
            operation,
            value,
        })
    }

    pub const fn entity(self) -> EntityRef {
        self.entity
    }

    pub const fn actor_value(self) -> FormRef {
        self.actor_value
    }

    pub const fn operation(self) -> ActorValueOperation {
        self.operation
    }

    pub const fn value(self) -> f32 {
        self.value
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ActorValueError {
    #[error("actor-value state contains a non-finite layer")]
    NonFiniteValue,
    #[error("actor-value form identity reserves local zero")]
    NullActorValue,
    #[error("actor-value operand is non-finite or exceeds its magnitude bound")]
    InvalidOperand,
    #[error("damage and restore operands must be non-negative")]
    NegativeMagnitude,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_composes_canonical_layers_and_rejects_non_finite_values() {
        let state = ActorValueState::new(100.0, 20.0, 10.0, 35.0).unwrap();
        assert_eq!(state.current(), 95.0);
        assert_eq!(
            ActorValueState::new(f32::NAN, 0.0, 0.0, 0.0),
            Err(ActorValueError::NonFiniteValue)
        );
    }

    #[test]
    fn commands_are_portable_and_bounded() {
        let entity = EntityRef::new(1, 2).unwrap();
        let actor_value = FormRef::new([7; 16], 42);
        assert!(ActorValueCommand::new(
            entity,
            actor_value,
            ActorValueOperation::ModifyPermanent,
            -5.0,
        )
        .is_ok());
        assert_eq!(
            ActorValueCommand::new(entity, actor_value, ActorValueOperation::Damage, -1.0,),
            Err(ActorValueError::NegativeMagnitude)
        );
    }
}
