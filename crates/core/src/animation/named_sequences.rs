//! The named animation sequences of a placed object (M47.3).
//!
//! A mesh with a `NiControllerManager` carries several `NiControllerSequence`s named for what they
//! do — `Forward`, `Backward`, `Idle`, `SpecialIdle`, ... (measured over FalloutNV.esm's object
//! meshes: 217 `forward`, 156 `specialidle`, 145 `backward`, 136 `idle`). The list is a catalog, not a
//! playlist: only the object's idle ([`NamedSequences::spawn_handle`]) starts at spawn, and an
//! activation clip (`Open`, `Forward`, ...) waits for its consumer — a script's `PlayGroup Forward`
//! chooses one, and `IsAnimPlaying Forward` can ask which is playing.

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

    /// The sequence that starts when the object spawns: its `Idle`, else its `SpecialIdle`
    /// (Oblivion's gates and machinery), else none (#6471).
    ///
    /// The first listed sequence is usually an activation clip with a clamped cycle — playing it
    /// at spawn leaves a door or container in its post-activation pose, and an activator as
    /// already activated. The legacy engines start a placed object's idle; the rest are played
    /// when something asks for them.
    pub fn spawn_handle(&self) -> Option<u32> {
        self.handle("idle").or_else(|| self.handle("specialidle"))
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

    fn set(names: &[&str]) -> NamedSequences {
        NamedSequences(Arc::from(names.iter().enumerate().map(|(i, n)| ((*n).into(), i as u32)).collect::<Vec<_>>()))
    }

    /// #6471 — a door whose first sequence is the clamped `Open` starts nothing; a model whose idle
    /// is not first still plays it; `Idle` wins over `SpecialIdle`.
    #[test]
    fn spawn_starts_the_idle_never_an_activation_clip() {
        assert_eq!(set(&["open", "close"]).spawn_handle(), None);
        assert_eq!(set(&["forward", "backward"]).spawn_handle(), None);
        assert_eq!(set(&["open", "idle"]).spawn_handle(), Some(1));
        assert_eq!(set(&["specialidle", "forward"]).spawn_handle(), Some(0));
        assert_eq!(set(&["specialidle", "idle"]).spawn_handle(), Some(1));
        assert_eq!(set(&[]).spawn_handle(), None);
    }

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
