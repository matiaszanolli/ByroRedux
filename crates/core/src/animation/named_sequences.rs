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

    /// The sequence that starts when the object spawns: its `Idle` when that clip loops, else its
    /// `SpecialIdle` (Oblivion's gates and machinery), else none (#6471, #6543). `loops` answers
    /// whether a handle's clip has a looping cycle.
    ///
    /// This is the legacy object-load rule (`TESObjectREFR` animation init: `Fallout3.exe`
    /// `0x75eea9`–`0x75f19c`, `Oblivion.exe` `0x4e36eb`–`0x4e37f8`; FalloutNV.exe carries the same
    /// function's warning strings). For a non-actor reference, the engine
    /// looks up `Idle` and `SpecialIdle` by name and activates `Idle` only when its cycle type is
    /// LOOP. Otherwise it logs "Idle animation must be looping" and skips it; it activates
    /// `SpecialIdle` whatever its cycle. With neither, it activates the first sequence for one
    /// update, so the model takes that sequence's first frame, then deactivates every sequence
    /// and clears the manager's active flag: nothing plays until a script asks (the parked player,
    /// [`Self::parked_handle`]). The engine would play both `Idle` and `SpecialIdle` on a model that
    /// had both; no FNV, FO3 or Oblivion object mesh does, so one handle is enough.
    pub fn spawn_handle(&self, loops: impl Fn(u32) -> bool) -> Option<u32> {
        self.handle("idle").filter(|&idle| loops(idle)).or_else(|| self.handle("specialidle"))
    }

    /// The sequence a stopped player is parked on when [`Self::spawn_handle`] finds nothing to play (#6610): a
    /// script's `PlayGroup` retargets the object's sequence player, and a model with no `Idle` would otherwise
    /// have none, so every `PlayGroup Forward` on a looping activator (the force-field glow, the Lucky 38's
    /// lights) was declined since #6471. It is the first listed sequence, never played at spawn. A stopped
    /// player is still sampled, so the model holds that sequence's first frame, as the legacy load does (#6543).
    pub fn parked_handle(&self) -> Option<u32> {
        self.0.first().map(|(_, handle)| *handle)
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
        let all_loop = |_| true;
        assert_eq!(set(&["open", "close"]).spawn_handle(all_loop), None);
        assert_eq!(set(&["forward", "backward"]).spawn_handle(all_loop), None);
        assert_eq!(set(&["open", "idle"]).spawn_handle(all_loop), Some(1));
        assert_eq!(set(&["specialidle", "forward"]).spawn_handle(all_loop), Some(0));
        assert_eq!(set(&["specialidle", "idle"]).spawn_handle(all_loop), Some(1));
        assert_eq!(set(&[]).spawn_handle(all_loop), None);
    }

    /// #6543 — the legacy load refuses an `Idle` that does not loop ("Idle animation must be looping"; FNV's
    /// `tlpodplayer01.nif` carries a clamped one) and still starts the `SpecialIdle`. A lone looping `Forward`
    /// (the Hoover Dam turbine shafts) is not an idle and does not play at spawn.
    #[test]
    fn spawn_refuses_an_idle_that_does_not_loop() {
        let only_handle_1_loops = |handle| handle == 1;
        assert_eq!(set(&["idle", "forward"]).spawn_handle(only_handle_1_loops), None);
        assert_eq!(set(&["idle", "specialidle"]).spawn_handle(only_handle_1_loops), Some(1));
        assert_eq!(set(&["idle", "specialidle"]).spawn_handle(|_| false), Some(1), "SpecialIdle starts whatever its cycle");
        assert_eq!(set(&["forward"]).spawn_handle(|_| true), None);
    }

    /// #6610 — with no idle nothing plays, but a stopped player is parked on the first sequence so `PlayGroup`
    /// has one to drive.
    #[test]
    fn a_model_without_an_idle_parks_a_player_on_its_first_sequence() {
        let forward_only = set(&["forward", "backward"]);
        assert_eq!(forward_only.spawn_handle(|_| true), None, "nothing plays at spawn");
        assert_eq!(forward_only.parked_handle(), Some(0), "but a player is parked for PlayGroup");
        assert_eq!(set(&[]).parked_handle(), None);
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
