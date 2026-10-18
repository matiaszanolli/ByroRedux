//! The named animation sequences of a placed object (M47.3).
//!
//! A mesh with a `NiControllerManager` carries several `NiControllerSequence`s named for what they
//! do — `Forward`, `Backward`, `Idle`, `SpecialIdle`, ... (measured over FalloutNV.esm's object
//! meshes: 217 `forward`, 156 `specialidle`, 145 `backward`, 136 `idle`). The cell loader plays the
//! first one on spawn; this component keeps the rest so a script's `PlayGroup Forward` can choose
//! one, and `IsAnimPlaying Forward` can ask which is playing.

use std::sync::Arc;

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;

/// Sequence name (ASCII-lowercased) to the [`AnimationClipRegistry`](super::AnimationClipRegistry)
/// handle of its clip, shared by every placement of the model.
///
/// `NOT_SAVED_BY_DESIGN`: stamped from the cached model at every spawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedSequences(pub Arc<[(Box<str>, u32)]>);

impl NamedSequences {
    /// The clip handle of the sequence called `name`, ignoring case.
    pub fn handle(&self, name: &str) -> Option<u32> {
        self.0.iter().find(|(sequence, _)| sequence.eq_ignore_ascii_case(name)).map(|(_, handle)| *handle)
    }

    /// The name of the sequence whose clip is `handle`.
    pub fn name_of(&self, handle: u32) -> Option<&str> {
        self.0.iter().find(|(_, h)| *h == handle).map(|(name, _)| &**name)
    }
}

impl Component for NamedSequences {
    type Storage = SparseSetStorage<Self>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sequence_is_found_by_name_ignoring_case_and_named_by_handle() {
        let set = NamedSequences(Arc::from(vec![("forward".into(), 4u32), ("backward".into(), 5)]));
        assert_eq!(set.handle("Forward"), Some(4));
        assert_eq!(set.handle("BACKWARD"), Some(5));
        assert_eq!(set.handle("idle"), None);
        assert_eq!(set.name_of(5), Some("backward"));
        assert_eq!(set.name_of(9), None);
    }
}
