//! Authored lock state for a placed reference.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;

/// REFR placement carries an `XLOC` lock — this entity (door or
/// container) is locked and must not be treated as activatable until
/// the deferred key-check / lockpicking policy exists.
///
/// Captured from `PlacedRef.lock` at spawn time (#3098). Deliberately a
/// component of its own rather than a field bolted onto `DoorTeleport`:
/// containers carry `XLOC` too and have no teleport data at all, so a
/// shared component is the only shape that covers both.
///
/// Lives in core rather than the binary (#3159) because two crates now
/// own halves of the lifecycle: the binary's cell loader inserts it and
/// its interaction system reads it as an activation gate, while the
/// scripting crate's `Effect::SetLocked` inserts and removes it on behalf
/// of a Papyrus `ObjectReference.Lock(..)` call. Before that effect
/// existed there was one insert and one read and *nothing anywhere that
/// removed it*, which made an authored lock a one-way door for the whole
/// session.
///
/// Key checks and lock-level-gated lockpicking are still deferred: the
/// only consumers today are the interaction gate and the two scripting
/// effects.
///
/// Sparse storage — locked REFRs are a small minority of placements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Locked {
    /// Lock difficulty as authored (see `byroredux_plugin::esm::cell::LockData`
    /// for the wire-level meaning).
    ///
    /// Written by `Effect::SetLockLevel` (Papyrus
    /// `ObjectReference.SetLockLevel`), but not yet *consumed* by any
    /// policy — no lockpicking system reads it. Kept so the eventual one
    /// does not need a second data-plumbing pass, and so a fragment that
    /// sets it does not have to decline wholesale.
    pub lock_level: u8,
    /// FormID of the key that opens this lock, if any. Not yet consumed
    /// — key-carry / key-check gameplay is deferred (see struct doc).
    pub key_form_id: Option<u32>,
}

impl Component for Locked {
    type Storage = SparseSetStorage<Self>;
}
