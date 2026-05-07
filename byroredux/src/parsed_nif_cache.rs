//! Generic bookkeeping core shared by every NIF cache resource —
//! `cell_loader::NifImportRegistry` (flat-import / REFR placements,
//! #381) and `scene_import_cache::SceneImportCache` (hierarchical /
//! NPC spawn, #880).
//!
//! Both wrappers store `Arc<T>` (or `None` for negative-cached parse
//! failures) keyed by lowercased model path, track lifetime
//! hits/misses/parsed/failed counts, and expose telemetry accessors.
//! That shared shape is collected here so each wrapper can layer its
//! own extensions on top:
//!
//!   * `NifImportRegistry` adds LRU eviction (`BYRO_NIF_CACHE_MAX`,
//!     `access_tick`, `evictions`) + per-key animation-clip handle
//!     memoisation (`clip_handles`, #544 / #863) + the streaming
//!     snapshot path (#862).
//!   * `SceneImportCache` adds a `bypass_parses` counter for the
//!     pre_spawn_hook = Some path that intentionally skips the
//!     cache (head FaceGen morphs, #880).
//!
//! Only the bookkeeping core lives here — wrapper-specific machinery
//! (LRU eviction, clip handles, snapshot keys) stays in the wrappers
//! so this module's API surface stays small.

use std::collections::HashMap;
use std::sync::Arc;

/// Shared cache of `Arc<T>` keyed by lowercased model path. `None`
/// entries record a model that failed to parse (or had zero useful
/// geometry) so re-parses don't fire on every placement / NPC spawn.
pub(crate) struct ParsedNifCache<T> {
    /// Storage map. Public to the crate so the wrapper-level
    /// extensions (LRU eviction in `NifImportRegistry`, etc.) can do
    /// O(N) sweeps over keys without paying a method-dispatch
    /// indirection. Direct mutation of this field is discouraged
    /// outside the wrapper layer — the parsed/failed counters
    /// maintained by `insert` / `remove` would drift.
    pub(crate) cache: HashMap<String, Option<Arc<T>>>,
    hits: u64,
    misses: u64,
    /// Live count of `Some(_)` entries — mirrors
    /// `cache.values().filter(|v| v.is_some()).count()` for O(1)
    /// telemetry reads. Maintained in lockstep with `insert` /
    /// `remove`.
    parsed_count: u64,
    /// Live count of `None` entries (failed-parse negative cache).
    failed_count: u64,
}

impl<T> Default for ParsedNifCache<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> ParsedNifCache<T> {
    pub(crate) fn new() -> Self {
        Self {
            cache: HashMap::new(),
            hits: 0,
            misses: 0,
            parsed_count: 0,
            failed_count: 0,
        }
    }

    /// Read-only probe. Does NOT bump hit/miss counters — call
    /// [`Self::record_hit`] / [`Self::record_miss`] separately when
    /// the call site wants per-lookup tracking. Sites that batch
    /// their own counts (`load_references` end-of-cell commit at
    /// `cell_loader.rs:1322`) bypass tracking here and use
    /// [`Self::accumulate_hits`] / [`Self::accumulate_misses`] once
    /// at the commit point.
    pub(crate) fn get(&self, key: &str) -> Option<&Option<Arc<T>>> {
        self.cache.get(key)
    }

    /// Insert (or overwrite) an entry. Adjusts `parsed_count` /
    /// `failed_count` to keep them in lockstep with the new value's
    /// `Some(_)` / `None` shape. Does NOT do LRU eviction — that's
    /// the wrapper's responsibility (only `NifImportRegistry`
    /// supports LRU today).
    pub(crate) fn insert(&mut self, key: String, value: Option<Arc<T>>) {
        match (self.cache.get(&key), &value) {
            (None, Some(_)) => {
                self.parsed_count = self.parsed_count.saturating_add(1);
            }
            (None, None) => {
                self.failed_count = self.failed_count.saturating_add(1);
            }
            (Some(Some(_)), None) => {
                self.parsed_count = self.parsed_count.saturating_sub(1);
                self.failed_count = self.failed_count.saturating_add(1);
            }
            (Some(None), Some(_)) => {
                self.failed_count = self.failed_count.saturating_sub(1);
                self.parsed_count = self.parsed_count.saturating_add(1);
            }
            // Same-state overwrite: counts unchanged.
            (Some(Some(_)), Some(_)) | (Some(None), None) => {}
        }
        self.cache.insert(key, value);
    }

    /// Remove an entry — used by LRU eviction in
    /// `NifImportRegistry`. Returns the removed value so the wrapper
    /// can release any extension state (clip handles, in #863's
    /// case) before dropping it. Adjusts `parsed_count` /
    /// `failed_count` to match the post-remove state.
    pub(crate) fn remove(&mut self, key: &str) -> Option<Option<Arc<T>>> {
        let removed = self.cache.remove(key)?;
        match &removed {
            Some(_) => {
                self.parsed_count = self.parsed_count.saturating_sub(1);
            }
            None => {
                self.failed_count = self.failed_count.saturating_sub(1);
            }
        }
        Some(removed)
    }

    /// Per-call-site hit/miss tracking — bumps the lifetime counters
    /// by 1. Counterpart of [`Self::accumulate_hits`] for sites that
    /// commit per-lookup rather than batched.
    pub(crate) fn record_hit(&mut self) {
        self.hits = self.hits.saturating_add(1);
    }

    pub(crate) fn record_miss(&mut self) {
        self.misses = self.misses.saturating_add(1);
    }

    /// Batched-commit hit/miss accumulation — adds an entire
    /// per-call tally to the lifetime counters in one shot. Used by
    /// `cell_loader::load_references` which counts hits/misses
    /// inline across hundreds of REFR placements and commits the
    /// total under a single resource write lock at end-of-cell
    /// (#523).
    pub(crate) fn accumulate_hits(&mut self, n: u64) {
        self.hits = self.hits.saturating_add(n);
    }

    pub(crate) fn accumulate_misses(&mut self, n: u64) {
        self.misses = self.misses.saturating_add(n);
    }

    pub(crate) fn hits(&self) -> u64 {
        self.hits
    }

    pub(crate) fn misses(&self) -> u64 {
        self.misses
    }

    pub(crate) fn parsed_count(&self) -> u64 {
        self.parsed_count
    }

    pub(crate) fn failed_count(&self) -> u64 {
        self.failed_count
    }

    pub(crate) fn len(&self) -> usize {
        self.cache.len()
    }

    #[allow(dead_code)]
    pub(crate) fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Hit rate as a percentage `[0, 100]`. `0.0` when no lookups
    /// have happened yet (avoid NaN).
    pub(crate) fn hit_rate_pct(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            100.0 * self.hits as f64 / total as f64
        }
    }

    /// Iterate over all keys (positive + negative entries). Used by
    /// `NifImportRegistry::snapshot_keys` (#862 streaming) — the
    /// streaming worker consults the snapshot to skip BSA-extract +
    /// parse for paths the main thread already holds.
    pub(crate) fn keys(&self) -> impl Iterator<Item = &String> {
        self.cache.keys()
    }

    /// Clear all entries and reset live parsed/failed counts.
    /// Lifetime hit/miss counters are preserved so the debug
    /// command can still display historical activity. Counterpart
    /// of `NifImportRegistry::clear` (#544 hard-reset path).
    #[allow(dead_code)]
    pub(crate) fn clear_entries(&mut self) {
        self.cache.clear();
        self.parsed_count = 0;
        self.failed_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Insert with `None` only bumps `failed_count`; insert with
    /// `Some(_)` only bumps `parsed_count`. Same-state overwrite
    /// leaves both unchanged.
    #[test]
    fn insert_maintains_parsed_failed_counts() {
        let mut cache: ParsedNifCache<u32> = ParsedNifCache::new();
        cache.insert("a.nif".into(), Some(Arc::new(1)));
        assert_eq!(cache.parsed_count(), 1);
        assert_eq!(cache.failed_count(), 0);

        cache.insert("b.nif".into(), None);
        assert_eq!(cache.parsed_count(), 1);
        assert_eq!(cache.failed_count(), 1);

        // Failure → success transition (re-parse succeeded after a
        // first-pass failure).
        cache.insert("b.nif".into(), Some(Arc::new(2)));
        assert_eq!(cache.parsed_count(), 2);
        assert_eq!(cache.failed_count(), 0);

        // Same-state overwrite must not double-count.
        cache.insert("a.nif".into(), Some(Arc::new(3)));
        assert_eq!(cache.parsed_count(), 2);
        assert_eq!(cache.failed_count(), 0);
    }

    /// `remove` adjusts the counts symmetrically — used by LRU
    /// eviction in `NifImportRegistry`.
    #[test]
    fn remove_decrements_correct_count() {
        let mut cache: ParsedNifCache<u32> = ParsedNifCache::new();
        cache.insert("a.nif".into(), Some(Arc::new(1)));
        cache.insert("b.nif".into(), None);
        assert_eq!(cache.parsed_count(), 1);
        assert_eq!(cache.failed_count(), 1);

        let removed = cache.remove("a.nif").expect("present");
        assert!(matches!(removed, Some(_)));
        assert_eq!(cache.parsed_count(), 0);
        assert_eq!(cache.failed_count(), 1);

        let removed = cache.remove("b.nif").expect("present");
        assert!(removed.is_none());
        assert_eq!(cache.parsed_count(), 0);
        assert_eq!(cache.failed_count(), 0);

        // Removing a non-existent key is a no-op.
        assert!(cache.remove("missing.nif").is_none());
    }

    /// Per-call hit/miss tracking and batched accumulation share
    /// the same lifetime counter — both increments are visible in
    /// `hits()` / `misses()` / `hit_rate_pct()`.
    #[test]
    fn record_and_accumulate_share_lifetime_counters() {
        let mut cache: ParsedNifCache<u32> = ParsedNifCache::new();
        cache.record_hit();
        cache.record_hit();
        cache.record_miss();
        cache.accumulate_hits(8);
        cache.accumulate_misses(2);
        assert_eq!(cache.hits(), 10);
        assert_eq!(cache.misses(), 3);
        // 10 / 13 ≈ 76.9%
        assert!((cache.hit_rate_pct() - (1000.0 / 13.0)).abs() < 1e-6);
    }

    /// `clear_entries` resets the live parsed/failed counts but
    /// preserves the lifetime hit/miss totals — telemetry continues
    /// across a hard reset.
    #[test]
    fn clear_entries_preserves_lifetime_totals() {
        let mut cache: ParsedNifCache<u32> = ParsedNifCache::new();
        cache.insert("a.nif".into(), Some(Arc::new(1)));
        cache.insert("b.nif".into(), None);
        cache.record_hit();
        cache.record_miss();

        cache.clear_entries();

        assert_eq!(cache.len(), 0);
        assert_eq!(cache.parsed_count(), 0);
        assert_eq!(cache.failed_count(), 0);
        // Lifetime counters survive.
        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.misses(), 1);
    }
}
