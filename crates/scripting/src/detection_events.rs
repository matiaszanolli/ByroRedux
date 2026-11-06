//! `ObjectReference.CreateDetectionEvent(akOwner, aiSoundLevel)` (M47.2).
//!
//! CK *CreateDetectionEvent - ObjectReference*: "Creates a detection event at
//! this object's location." `akOwner` is "the 'owner' of the detection event"
//! and `aiSoundLevel` its "sound level (0 to 100)", default 0.
//!
//! The engine has no hearing: an actor notices a target by sight alone
//! (`faction_hostility`'s detection range and line-of-sight test), and nothing
//! maps a sound level to a distance. So the events are recorded here, in order,
//! for the hearing system that will read them, and none reacts to them yet —
//! a tripwire made of this call plays its sound and animation, and wakes no
//! one. The queue is capped so an unread one cannot grow without bound.

use byroredux_core::ecs::resource::Resource;

/// The most events kept while nothing reads them; the oldest are dropped.
pub const MAX_PENDING_DETECTION_EVENTS: usize = 256;

/// One `CreateDetectionEvent` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetectionEvent {
    /// The reference the event is created at (the one the call was made on).
    pub source: u32,
    /// The event's owner, or 0 for none.
    pub owner: u32,
    /// The sound level, 0 to 100 in vanilla.
    pub sound_level: i32,
}

/// The detection events nothing has read yet.
#[derive(Debug, Default)]
pub struct PendingDetectionEvents(pub Vec<DetectionEvent>);

impl Resource for PendingDetectionEvents {}

impl PendingDetectionEvents {
    pub fn push(&mut self, event: DetectionEvent) {
        if self.0.len() >= MAX_PENDING_DETECTION_EVENTS {
            self.0.remove(0);
        }
        self.0.push(event);
    }
}

/// #6706 — this subsystem's world setup, owned here instead of the crate's `register` hub.
pub fn install(world: &mut byroredux_core::ecs::World) {
    world.init_resource::<PendingDetectionEvents>();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_queue_drops_the_oldest_past_its_cap() {
        let mut pending = PendingDetectionEvents::default();
        for level in 0..(MAX_PENDING_DETECTION_EVENTS as i32 + 3) {
            pending.push(DetectionEvent { source: 1, owner: 2, sound_level: level });
        }
        assert_eq!(pending.0.len(), MAX_PENDING_DETECTION_EVENTS);
        assert_eq!(pending.0[0].sound_level, 3, "the first three were dropped");
    }
}
