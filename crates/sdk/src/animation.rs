//! Portable callback-local authored animation state and commands.

use crate::identity::{EntityRef, FormRef};

/// Engine-recognized behavior event emitted by authored animation data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnimationEvent {
    PlayImod,
    IdleFurnitureExit,
    ExitCartEnd,
}

/// Latest authored IDLE request and behavior-event state for one actor.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AnimationSnapshot {
    requested_idle: Option<FormRef>,
    request_generation: u64,
    awaited_event: Option<AnimationEvent>,
    last_event: Option<AnimationEvent>,
    event_generation: u64,
}

impl AnimationSnapshot {
    pub const fn new(
        requested_idle: Option<FormRef>,
        request_generation: u64,
        awaited_event: Option<AnimationEvent>,
        last_event: Option<AnimationEvent>,
        event_generation: u64,
    ) -> Self {
        Self {
            requested_idle,
            request_generation,
            awaited_event,
            last_event,
            event_generation,
        }
    }

    pub const fn requested_idle(self) -> Option<FormRef> {
        self.requested_idle
    }

    pub const fn request_generation(self) -> u64 {
        self.request_generation
    }

    pub const fn awaited_event(self) -> Option<AnimationEvent> {
        self.awaited_event
    }

    pub const fn last_event(self) -> Option<AnimationEvent> {
        self.last_event
    }

    pub const fn event_generation(self) -> u64 {
        self.event_generation
    }
}

/// Deferred request to play one authored IDLE record on a visible actor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlayIdleCommand {
    entity: EntityRef,
    idle: FormRef,
}

impl PlayIdleCommand {
    pub const fn new(entity: EntityRef, idle: FormRef) -> Self {
        Self { entity, idle }
    }

    pub const fn entity(self) -> EntityRef {
        self.entity
    }

    pub const fn idle(self) -> FormRef {
        self.idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animation_state_preserves_portable_idle_and_generations() {
        let idle = FormRef::new([7; 16], 42);
        let snapshot = AnimationSnapshot::new(
            Some(idle),
            9,
            Some(AnimationEvent::ExitCartEnd),
            Some(AnimationEvent::PlayImod),
            11,
        );
        assert_eq!(snapshot.requested_idle(), Some(idle));
        assert_eq!(snapshot.request_generation(), 9);
        assert_eq!(snapshot.awaited_event(), Some(AnimationEvent::ExitCartEnd));
        assert_eq!(snapshot.last_event(), Some(AnimationEvent::PlayImod));
        assert_eq!(snapshot.event_generation(), 11);
    }
}
