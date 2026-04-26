//! KFM-driven animation state machine — the `NiControllerManager`
//! equivalent for Redux.
//!
//! # What this is
//!
//! An [`AnimationController`] component carries a catalog of sequences
//! (`sequence_id → clip_handle`) and a transition table
//! (`(src_id, dst_id) → blend metadata`) — exactly the shape Gamebryo's
//! `NiKFMTool` exports into a KFM file. [`apply_pending_transition`]
//! consumes a pending sequence request, looks up the transition
//! metadata, and drives [`AnimationStack::play`] with the right blend
//! duration.
//!
//! This closes the "missing glue" gap noted in the 2026-04-15 legacy
//! audit (AR-09 / #338): the KFM parser provides catalog data,
//! `AnimationStack` provides the blend mechanism, and this module
//! connects them without coupling `byroredux-core` to the NIF /
//! KFM parser crate.
//!
//! # Crate independence
//!
//! The controller deliberately does NOT take a
//! `byroredux_nif::kfm::KfmFile` directly — that would pull the NIF
//! parser into every consumer of `byroredux-core`. Instead the caller
//! assembles the state machine from the KFM in their own crate:
//!
//! ```ignore
//! use byroredux_core::animation::{
//!     AnimationController, ControllerTransitionDefaults, TransitionKind,
//! };
//!
//! let kfm = byroredux_nif::kfm::parse_kfm(&bytes)?;
//! let mut ctrl = AnimationController::new(
//!     ControllerTransitionDefaults::from_kfm(&kfm.default_sync_transition),
//!     ControllerTransitionDefaults::from_kfm(&kfm.default_nonsync_transition),
//! );
//! for seq in &kfm.sequences {
//!     let clip_handle = registry.register(load_kf(&seq.filename)?);
//!     ctrl.add_sequence(seq.sequence_id, clip_handle);
//!     for t in &seq.transitions {
//!         ctrl.add_transition(
//!             seq.sequence_id, t.dest_sequence_id,
//!             TransitionKind::from_kfm(t.transition_type),
//!             t.duration,
//!         );
//!     }
//! }
//! for group in &kfm.sequence_groups {
//!     for m in &group.members {
//!         ctrl.set_sync_group(m.sequence_id, group.group_id);
//!     }
//! }
//! // Gameplay code requests a sequence — transition fires on next tick:
//! ctrl.request_sequence(WALK_SEQ_ID);
//! apply_pending_transition(&mut ctrl, &mut stack);
//! ```

use std::collections::HashMap;

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;

use super::stack::AnimationStack;

/// Transition style picked by an individual `KfmTransition`. The enum
/// values mirror `byroredux_nif::kfm::KfmTransitionType` without the
/// cross-crate dependency — convert at the caller via
/// [`TransitionKind::from_kfm_discriminant`] or a manual `match`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionKind {
    /// Straight blend between the outgoing and incoming clips.
    Blend,
    /// Morph transition — needs `blend_pairs` in the source KFM to
    /// align text keys; the controller treats it the same as `Blend`
    /// today because text-key-driven morphing is future work.
    Morph,
    /// Crossfade with overlapped playback.
    Crossfade,
    /// Multi-step chain through intermediate sequences. The current
    /// implementation fires a single `play()` to the final target;
    /// intermediate chain steps are a follow-up once gameplay needs
    /// them.
    Chain,
    /// Inherit the catalog's `default_sync_transition` duration.
    DefaultSync,
    /// Inherit the catalog's `default_nonsync_transition` duration.
    DefaultNonSync,
}

impl TransitionKind {
    /// Convert a raw KFM transition-type discriminant (as exposed by
    /// `byroredux_nif::kfm::KfmTransitionType`) into the controller's
    /// enum. Unknown values fall through to `Blend` — the safest
    /// interpretation: "cross-fade at the stated duration."
    pub fn from_kfm_discriminant(value: i32) -> Self {
        match value {
            0 => Self::Blend,
            1 => Self::Morph,
            2 => Self::Crossfade,
            3 => Self::Chain,
            4 => Self::DefaultSync,
            5 => Self::DefaultNonSync,
            _ => Self::Blend,
        }
    }
}

/// Defaults applied when an individual transition entry references
/// `DefaultSync` / `DefaultNonSync`, or when no explicit transition
/// exists between two sequences.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControllerTransitionDefaults {
    pub kind: TransitionKind,
    pub duration: f32,
}

impl Default for ControllerTransitionDefaults {
    fn default() -> Self {
        Self {
            kind: TransitionKind::Blend,
            duration: 0.0,
        }
    }
}

/// Single entry in the controller's transition table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControllerTransition {
    pub kind: TransitionKind,
    pub duration: f32,
}

/// KFM-backed animation state machine. The `NiControllerManager`
/// equivalent for Redux — see module docs.
#[derive(Debug, Default)]
pub struct AnimationController {
    /// Catalog of available sequences, keyed by the KFM-assigned
    /// `sequence_id`. Values are the animation-clip handles the
    /// consumer registered in `AnimationClipRegistry`.
    pub sequences: HashMap<u32, u32>,
    /// Transition table. Keys are `(src_sequence_id, dst_sequence_id)`.
    pub transitions: HashMap<(u32, u32), ControllerTransition>,
    /// Sync-group membership for each sequence. Used to decide
    /// between the sync and non-sync defaults when no explicit
    /// transition exists. Sequences not in this map are treated as
    /// "not sync-compatible with anything."
    pub sync_groups: HashMap<u32, u32>,
    /// Applied when no explicit transition is authored and the
    /// source + destination share a sync group.
    pub default_sync: ControllerTransitionDefaults,
    /// Applied when no explicit transition is authored and the
    /// source + destination don't share a sync group (or either is
    /// ungrouped).
    pub default_nonsync: ControllerTransitionDefaults,
    /// Sequence currently playing on the bound `AnimationStack`.
    /// `None` before the first transition fires.
    pub current_sequence_id: Option<u32>,
    /// Pending sequence change queued by `request_sequence`. Cleared
    /// by `apply_pending_transition` once the stack play call fires.
    pub pending_request: Option<u32>,
}

impl Component for AnimationController {
    type Storage = SparseSetStorage<Self>;
}

impl AnimationController {
    /// Create an empty controller seeded with the default transition
    /// durations from a KFM file (or hand-built equivalents). Use
    /// `add_sequence` / `add_transition` / `set_sync_group` to fill
    /// the catalog.
    pub fn new(
        default_sync: ControllerTransitionDefaults,
        default_nonsync: ControllerTransitionDefaults,
    ) -> Self {
        Self {
            sequences: HashMap::new(),
            transitions: HashMap::new(),
            sync_groups: HashMap::new(),
            default_sync,
            default_nonsync,
            current_sequence_id: None,
            pending_request: None,
        }
    }

    /// Register a `sequence_id → clip_handle` mapping. Later calls
    /// with the same `sequence_id` overwrite — the last registration
    /// wins, matching how KFM catalogs are authored (duplicates are
    /// documented as "last entry wins" in `NiKFMTool::AddSequence`).
    pub fn add_sequence(&mut self, sequence_id: u32, clip_handle: u32) {
        self.sequences.insert(sequence_id, clip_handle);
    }

    /// Record an explicit transition between two sequences.
    pub fn add_transition(
        &mut self,
        src_sequence_id: u32,
        dst_sequence_id: u32,
        kind: TransitionKind,
        duration: f32,
    ) {
        self.transitions.insert(
            (src_sequence_id, dst_sequence_id),
            ControllerTransition { kind, duration },
        );
    }

    /// Place `sequence_id` into the sync group `group_id`.
    pub fn set_sync_group(&mut self, sequence_id: u32, group_id: u32) {
        self.sync_groups.insert(sequence_id, group_id);
    }

    /// Queue a transition to `target_sequence_id`. The change takes
    /// effect on the next `apply_pending_transition` call.
    pub fn request_sequence(&mut self, target_sequence_id: u32) {
        self.pending_request = Some(target_sequence_id);
    }

    /// Resolve the blend-in duration for a `from → to` transition per
    /// the KFM transition-table rules:
    ///
    /// 1. `from == None` (first-ever play): snap with zero blend.
    /// 2. Explicit transition exists: use its duration (unless the
    ///    transition kind is `DefaultSync` / `DefaultNonSync`, in
    ///    which case the corresponding top-level default duration
    ///    wins — matching `NiKFMTool::GetTransitionType` semantics).
    /// 3. No explicit transition: pick between `default_sync` and
    ///    `default_nonsync` based on sync-group compatibility.
    pub fn resolve_blend_time(&self, from: Option<u32>, to: u32) -> f32 {
        let Some(from_id) = from else {
            return 0.0;
        };
        if let Some(transition) = self.transitions.get(&(from_id, to)) {
            return match transition.kind {
                TransitionKind::DefaultSync => self.default_sync.duration,
                TransitionKind::DefaultNonSync => self.default_nonsync.duration,
                _ => transition.duration,
            };
        }
        let same_group = match (self.sync_groups.get(&from_id), self.sync_groups.get(&to)) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        };
        if same_group {
            self.default_sync.duration
        } else {
            self.default_nonsync.duration
        }
    }
}

/// Consume any pending sequence request on `controller`: resolve the
/// blend duration from the transition catalog and call
/// [`AnimationStack::play`] with the matching clip handle. Updates
/// `current_sequence_id` on success. No-op when there's no pending
/// request or the requested `sequence_id` isn't in the catalog.
///
/// Returns `true` when a transition fired, `false` otherwise — lets
/// the caller branch on whether to visit downstream systems that
/// care about sequence changes (e.g. animation-event bookkeeping).
pub fn apply_pending_transition(
    controller: &mut AnimationController,
    stack: &mut AnimationStack,
) -> bool {
    let Some(target) = controller.pending_request else {
        return false;
    };
    let Some(&clip_handle) = controller.sequences.get(&target) else {
        // Silently drop the request — the KFM's catalog may have
        // been trimmed after the controller was built. Clearing the
        // `pending_request` prevents the missing sequence from
        // re-firing every tick.
        controller.pending_request = None;
        return false;
    };
    let blend_time = controller.resolve_blend_time(controller.current_sequence_id, target);
    stack.play(clip_handle, blend_time);
    controller.current_sequence_id = Some(target);
    controller.pending_request = None;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_defaults(duration: f32) -> ControllerTransitionDefaults {
        ControllerTransitionDefaults {
            kind: TransitionKind::Blend,
            duration,
        }
    }

    #[test]
    fn first_play_has_zero_blend_time() {
        let ctrl = AnimationController::new(mk_defaults(0.3), mk_defaults(0.5));
        assert_eq!(ctrl.resolve_blend_time(None, 10), 0.0);
    }

    #[test]
    fn explicit_transition_duration_wins() {
        let mut ctrl = AnimationController::new(mk_defaults(0.3), mk_defaults(0.5));
        ctrl.add_transition(1, 2, TransitionKind::Crossfade, 0.8);
        assert_eq!(ctrl.resolve_blend_time(Some(1), 2), 0.8);
    }

    #[test]
    fn transition_referencing_default_sync_falls_back_to_default_duration() {
        let mut ctrl = AnimationController::new(mk_defaults(0.3), mk_defaults(0.5));
        // Explicit transition entry exists but its `kind` defers to
        // the top-level default — duration on the per-entry record
        // is ignored per `NiKFMTool::GetTransitionType`.
        ctrl.add_transition(1, 2, TransitionKind::DefaultSync, 999.0);
        assert_eq!(ctrl.resolve_blend_time(Some(1), 2), 0.3);
    }

    #[test]
    fn transition_referencing_default_nonsync_falls_back_to_default_duration() {
        let mut ctrl = AnimationController::new(mk_defaults(0.3), mk_defaults(0.5));
        ctrl.add_transition(1, 2, TransitionKind::DefaultNonSync, 999.0);
        assert_eq!(ctrl.resolve_blend_time(Some(1), 2), 0.5);
    }

    #[test]
    fn implicit_transition_uses_sync_default_when_groups_match() {
        let mut ctrl = AnimationController::new(mk_defaults(0.3), mk_defaults(0.5));
        ctrl.set_sync_group(1, 100);
        ctrl.set_sync_group(2, 100);
        assert_eq!(ctrl.resolve_blend_time(Some(1), 2), 0.3);
    }

    #[test]
    fn implicit_transition_uses_nonsync_default_when_groups_differ() {
        let mut ctrl = AnimationController::new(mk_defaults(0.3), mk_defaults(0.5));
        ctrl.set_sync_group(1, 100);
        ctrl.set_sync_group(2, 200);
        assert_eq!(ctrl.resolve_blend_time(Some(1), 2), 0.5);
    }

    #[test]
    fn implicit_transition_uses_nonsync_default_when_either_is_ungrouped() {
        let mut ctrl = AnimationController::new(mk_defaults(0.3), mk_defaults(0.5));
        ctrl.set_sync_group(1, 100);
        // Sequence 2 has no group → pair is not sync-compatible.
        assert_eq!(ctrl.resolve_blend_time(Some(1), 2), 0.5);
    }

    #[test]
    fn apply_pending_transition_plays_requested_clip_with_blend_time() {
        let mut ctrl = AnimationController::new(mk_defaults(0.4), mk_defaults(0.6));
        ctrl.add_sequence(10, 100); // seq 10 → clip handle 100
        ctrl.add_sequence(20, 200);
        ctrl.add_transition(10, 20, TransitionKind::Crossfade, 0.75);

        let mut stack = AnimationStack::new();

        // First request: no current sequence → zero blend.
        ctrl.request_sequence(10);
        assert!(apply_pending_transition(&mut ctrl, &mut stack));
        assert_eq!(ctrl.current_sequence_id, Some(10));
        assert!(ctrl.pending_request.is_none());
        assert_eq!(stack.layers.len(), 1);
        assert_eq!(stack.layers[0].clip_handle, 100);
        // Zero blend → weight jumps to 1.0.
        assert_eq!(stack.layers[0].weight, 1.0);
        assert_eq!(stack.layers[0].blend_in_total, 0.0);

        // Second request: explicit transition → 0.75 s cross-fade.
        ctrl.request_sequence(20);
        assert!(apply_pending_transition(&mut ctrl, &mut stack));
        assert_eq!(ctrl.current_sequence_id, Some(20));
        // Old layer now fading out; new layer blending in.
        assert_eq!(stack.layers.len(), 2);
        let new_layer = stack.layers.iter().find(|l| l.clip_handle == 200).unwrap();
        assert_eq!(new_layer.blend_in_total, 0.75);
    }

    #[test]
    fn apply_pending_transition_is_noop_without_request() {
        let mut ctrl = AnimationController::new(mk_defaults(0.3), mk_defaults(0.5));
        ctrl.add_sequence(10, 100);
        let mut stack = AnimationStack::new();
        assert!(!apply_pending_transition(&mut ctrl, &mut stack));
        assert!(stack.layers.is_empty());
        assert!(ctrl.current_sequence_id.is_none());
    }

    #[test]
    fn apply_pending_transition_silently_drops_unknown_sequence() {
        let mut ctrl = AnimationController::new(mk_defaults(0.3), mk_defaults(0.5));
        // Sequence 99 isn't registered — request must not panic and
        // must clear `pending_request` so the caller doesn't re-fire
        // it every tick.
        ctrl.request_sequence(99);
        let mut stack = AnimationStack::new();
        assert!(!apply_pending_transition(&mut ctrl, &mut stack));
        assert!(ctrl.pending_request.is_none());
        assert!(ctrl.current_sequence_id.is_none());
        assert!(stack.layers.is_empty());
    }

    #[test]
    fn transition_kind_from_kfm_discriminant_maps_known_values() {
        assert_eq!(
            TransitionKind::from_kfm_discriminant(0),
            TransitionKind::Blend
        );
        assert_eq!(
            TransitionKind::from_kfm_discriminant(1),
            TransitionKind::Morph
        );
        assert_eq!(
            TransitionKind::from_kfm_discriminant(2),
            TransitionKind::Crossfade
        );
        assert_eq!(
            TransitionKind::from_kfm_discriminant(3),
            TransitionKind::Chain
        );
        assert_eq!(
            TransitionKind::from_kfm_discriminant(4),
            TransitionKind::DefaultSync
        );
        assert_eq!(
            TransitionKind::from_kfm_discriminant(5),
            TransitionKind::DefaultNonSync
        );
        // Unknown falls back to safe Blend.
        assert_eq!(
            TransitionKind::from_kfm_discriminant(99),
            TransitionKind::Blend
        );
        assert_eq!(
            TransitionKind::from_kfm_discriminant(-1),
            TransitionKind::Blend
        );
    }

    #[test]
    fn add_sequence_overwrite_is_last_write_wins() {
        let mut ctrl = AnimationController::new(mk_defaults(0.3), mk_defaults(0.5));
        ctrl.add_sequence(1, 100);
        ctrl.add_sequence(1, 200); // re-register — last wins.
        let mut stack = AnimationStack::new();
        ctrl.request_sequence(1);
        apply_pending_transition(&mut ctrl, &mut stack);
        assert_eq!(stack.layers[0].clip_handle, 200);
    }
}
