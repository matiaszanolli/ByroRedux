//! #6106 — retry a full-detail cell whose geometry uploads the pool refused.
//!
//! A cell applied while the scene-geometry pool's admission was closed loses its uncached statics
//! (or its terrain mesh) and is inserted into `loaded` like any other; nothing re-ran it, so it
//! stayed incomplete for its whole residency. The retry reuses the paths a boundary crossing
//! already takes, and leaves compaction timing alone:
//!
//! 1. A cell whose own apply recorded refusals is noted ([`RefusalRetryState::note_refused`]).
//! 2. Once admission is open on both tiers and no load batch is in flight, the noted cells still
//!    loaded are handed back for unloading ([`RefusalRetry::Unload`]).
//! 3. Once the pool has compacted the space they held (geometry not dirty, no rebuild running), they
//!    are handed back for queueing ([`RefusalRetry::Reload`]) whether or not admission is still open:
//!    an unloaded cell must not stay absent, and at worst it comes back as incomplete as it was.
//!
//! The LOD rings refill freed pool space as soon as admission reopens, so the caller holds their
//! re-arm while [`RefusalRetryState::is_idle`] is false; otherwise the retry never gets the room.
//!
//! Reloading in the same step as the unload would repeat the original fault: the unloaded spans
//! stay counted until the compaction that only runs between batches. Each cell gets at most
//! [`MAX_REFUSAL_RETRIES`] retries per residency, so a pool that is genuinely too small for the
//! resident set settles instead of cycling.

use std::collections::{HashMap, HashSet};

/// Retries one cell gets before its refusal is accepted for the rest of its residency.
pub(crate) const MAX_REFUSAL_RETRIES: u8 = 2;

/// What the streaming step should do this frame for refused cells.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RefusalRetry {
    None,
    /// Unload these loaded cells; they are queued again once the pool has compacted.
    Unload(Vec<(i32, i32)>),
    /// Queue these cells again (they were unloaded for a retry).
    Reload(Vec<(i32, i32)>),
}

/// The pool-pressure inputs [`RefusalRetryState::plan`] reads, sampled once per streaming step.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PoolState {
    /// A load batch (queued, parsing or applying) or a LOD reconcile is in flight.
    pub(crate) batch_in_progress: bool,
    /// Both admission tiers are open (`MeshRegistry::prop_geometry_admission_open`).
    pub(crate) admission_open: bool,
    /// No geometry rebuild is pending or running, so dead spans have been compacted away.
    pub(crate) geometry_settled: bool,
}

#[derive(Debug, Default)]
pub(crate) struct RefusalRetryState {
    /// Loaded cells that came in with refused uploads.
    refused: HashSet<(i32, i32)>,
    /// Retries spent per cell this residency.
    retries: HashMap<(i32, i32), u8>,
    /// Cells unloaded for a retry, waiting for the compaction before they are queued.
    waiting: Vec<(i32, i32)>,
}

impl RefusalRetryState {
    /// A cell finished its full-detail apply with `refused` uploads refused at the pool limit.
    pub(crate) fn note_applied(&mut self, coord: (i32, i32), refused: u64) {
        if refused > 0 {
            self.refused.insert(coord);
        } else {
            // A clean apply (first or retried) ends the cell's retry history.
            self.retries.remove(&coord);
        }
    }

    /// No cell is noted or waiting. While this is false the caller holds the LOD rings' re-arm,
    /// so the space a reopened pool offers goes to the refused full-detail cells first.
    pub(crate) fn is_idle(&self) -> bool {
        self.refused.is_empty() && self.waiting.is_empty()
    }

    /// Decide this frame's retry work. `is_loaded` / `in_radius` describe the cell map: a noted cell
    /// that has been unloaded (or left the load radius) by a crossing is forgotten, as is a waiting
    /// cell that left the radius before it could be queued.
    pub(crate) fn plan(
        &mut self,
        pool: PoolState,
        is_loaded: impl Fn((i32, i32)) -> bool,
        in_radius: impl Fn((i32, i32)) -> bool,
    ) -> RefusalRetry {
        self.refused.retain(|&coord| is_loaded(coord));
        self.retries.retain(|&coord, _| in_radius(coord));
        if pool.batch_in_progress {
            return RefusalRetry::None;
        }
        if !self.waiting.is_empty() {
            if !pool.geometry_settled {
                return RefusalRetry::None;
            }
            let reload: Vec<_> = self.waiting.drain(..).filter(|&coord| in_radius(coord)).collect();
            return if reload.is_empty() { RefusalRetry::None } else { RefusalRetry::Reload(reload) };
        }
        if !pool.admission_open {
            return RefusalRetry::None;
        }
        let mut unload: Vec<_> = Vec::new();
        for coord in self.refused.drain() {
            let spent = self.retries.entry(coord).or_insert(0);
            if *spent >= MAX_REFUSAL_RETRIES {
                log::warn!(
                    "Exterior cell ({},{}) still had geometry refused after {MAX_REFUSAL_RETRIES} retries; \
                     it keeps the meshes it has for this residency (#6106)",
                    coord.0,
                    coord.1,
                );
                continue;
            }
            *spent += 1;
            unload.push(coord);
        }
        if unload.is_empty() {
            return RefusalRetry::None;
        }
        unload.sort_unstable();
        self.waiting.extend_from_slice(&unload);
        RefusalRetry::Unload(unload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPEN: PoolState = PoolState { batch_in_progress: false, admission_open: true, geometry_settled: true };

    fn all(_: (i32, i32)) -> bool {
        true
    }

    /// A refused cell is unloaded only once admission is open and nothing is loading, then queued
    /// only after the compaction has settled.
    #[test]
    fn a_refused_cell_is_unloaded_then_reloaded_after_the_compaction() {
        let mut state = RefusalRetryState::default();
        state.note_applied((3, 4), 2);

        let closed = PoolState { admission_open: false, ..OPEN };
        assert_eq!(state.plan(closed, all, all), RefusalRetry::None, "admission still closed");
        let busy = PoolState { batch_in_progress: true, ..OPEN };
        assert_eq!(state.plan(busy, all, all), RefusalRetry::None, "a batch is in flight");

        assert_eq!(state.plan(OPEN, all, all), RefusalRetry::Unload(vec![(3, 4)]));
        let dirty = PoolState { geometry_settled: false, ..OPEN };
        assert_eq!(state.plan(dirty, |_| false, all), RefusalRetry::None, "wait for the compaction");
        assert_eq!(state.plan(OPEN, |_| false, all), RefusalRetry::Reload(vec![(3, 4)]));
        assert!(state.is_idle());
    }

    /// A cell unloaded for a retry is queued again once the pool has compacted even if admission
    /// closed meanwhile: it must not stay absent.
    #[test]
    fn an_unloaded_cell_is_reloaded_even_if_admission_closed_again() {
        let mut state = RefusalRetryState::default();
        state.note_applied((2, 2), 3);
        assert_eq!(state.plan(OPEN, all, all), RefusalRetry::Unload(vec![(2, 2)]));
        assert!(!state.is_idle(), "the LOD re-arm is held while a retry is open");
        let closed = PoolState { admission_open: false, ..OPEN };
        assert_eq!(state.plan(closed, |_| false, all), RefusalRetry::Reload(vec![(2, 2)]));
    }

    /// A clean apply needs no retry and clears the cell's history.
    #[test]
    fn a_clean_apply_needs_no_retry() {
        let mut state = RefusalRetryState::default();
        state.note_applied((0, 0), 0);
        assert_eq!(state.plan(OPEN, all, all), RefusalRetry::None);
    }

    /// The retry count is capped per cell, so a pool too small for the resident set settles.
    #[test]
    fn retries_stop_after_the_cap() {
        let mut state = RefusalRetryState::default();
        for _ in 0..MAX_REFUSAL_RETRIES {
            state.note_applied((1, 1), 1);
            assert_eq!(state.plan(OPEN, all, all), RefusalRetry::Unload(vec![(1, 1)]));
            assert_eq!(state.plan(OPEN, |_| false, all), RefusalRetry::Reload(vec![(1, 1)]));
        }
        state.note_applied((1, 1), 1);
        assert_eq!(state.plan(OPEN, all, all), RefusalRetry::None, "out of retries");
        assert!(state.is_idle());
    }

    /// A crossing that unloads a noted cell, or moves a waiting cell out of the load radius, ends
    /// its retry.
    #[test]
    fn a_cell_the_crossing_dropped_is_not_retried() {
        let mut state = RefusalRetryState::default();
        state.note_applied((5, 5), 1);
        assert_eq!(state.plan(OPEN, |_| false, all), RefusalRetry::None, "no longer loaded");

        state.note_applied((6, 6), 1);
        assert_eq!(state.plan(OPEN, all, all), RefusalRetry::Unload(vec![(6, 6)]));
        assert_eq!(state.plan(OPEN, |_| false, |_| false), RefusalRetry::None, "left the radius");
        assert!(state.is_idle());
    }
}
