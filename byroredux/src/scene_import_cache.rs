//! Process-lifetime cache of parsed-and-imported NIF scenes for the
//! hierarchical scene-import path (`load_nif_bytes_with_skeleton`).
//!
//! Pre-fix every NPC spawn re-parsed the same skeleton + body + hand
//! NIFs from BSA bytes; for Megaton's ~40 NPCs × ~7 NIFs each that's
//! ~280 redundant parses per cell load. The existing
//! `cell_loader::NifImportRegistry` (#381) caches the FLAT-import
//! shape (`CachedNifImport`) used by REFR placements; NPC spawn needs
//! the HIERARCHICAL `ImportedScene` (with `nodes: Vec<ImportedNode>`)
//! so the bone hierarchy can be spawned. Different output shape →
//! separate wrapper, but the bookkeeping core is shared via
//! [`crate::parsed_nif_cache::ParsedNifCache`]. See #880 / CELL-PERF-02.
//!
//! `pre_spawn_hook` complication: head NIFs with FaceGen morphs apply
//! per-NPC mutations to `imported.meshes[i].positions`. The cache is
//! consulted only when `pre_spawn_hook` is `None` — skeleton, body,
//! hand, and head-without-morph spawns hit the cache; head-with-morph
//! stays on the legacy parse-per-call path. This still captures
//! ≥ 6/7 of the audit's ~280 redundant parses.
//!
//! **Memory bound** (#3760 / SAFE-2026-08-30-D3-01): each entry is a
//! full `Arc<ImportedScene>` — positions/normals/tangents/UVs/indices
//! per mesh, roughly 60+ bytes per vertex retained on the CPU heap in
//! addition to the GPU copy, a substantially heavier per-entry payload
//! than `NifImportRegistry`'s flat `CachedNifImport`. Pre-fix this
//! cache had no cap at all — every distinct NPC skeleton/body/head/
//! armor NIF path ever seen (armor joins the population via
//! `npc_spawn/resumable.rs`'s `hidden_biped_mask == 0` path) stayed
//! resident until process exit; on Skyrim SE/FO4 that population is
//! hundreds-to-thousands across a long exterior-streaming session.
//!
//! Reuses `NifImportRegistry`'s `BYRO_NIF_CACHE_MAX` env var for the
//! `=0` unlimited escape hatch (parity — one knob controls both NIF
//! caches' unlimited mode), but with its own, much lower hardcoded
//! default given the heavier per-entry payload: `NifImportRegistry`'s
//! 2048 default sized for the FLAT-import shape would be a poor fit
//! here. Uses the simpler half-eviction-on-overflow shape
//! `asset_provider/material.rs`'s `bgem_cache`/`failed_paths` already
//! establish (#951/#1430) rather than porting `NifImportRegistry`'s
//! full LRU-with-clip-handle-eviction-bias machinery — this cache has
//! no clip-handle bookkeeping of its own to protect, so the extra
//! complexity that machinery exists for doesn't apply here.

use std::collections::VecDeque;
use std::sync::Arc;

use byroredux_core::ecs::Resource;
use byroredux_nif::import::ImportedScene;

use crate::parsed_nif_cache::ParsedNifCache;

/// Default cap when `BYRO_NIF_CACHE_MAX` is unset. Deliberately much
/// lower than `NifImportRegistry`'s 2048 — see the module doc.
const DEFAULT_MAX_ENTRIES: usize = 300;

/// Wrapper around the shared `ParsedNifCache` core that adds the
/// bypass-parse counter for the head-FaceGen path that intentionally
/// skips the cache, plus a half-eviction memory bound (#3760).
pub(crate) struct SceneImportCache {
    core: ParsedNifCache<ImportedScene>,
    /// Parses recorded via [`Self::record_bypass_parse`] —
    /// pre_spawn_hook = Some path that skipped the cache. Tracked
    /// separately from the core's lifetime hits/misses so the test
    /// plan can pin "10 NPCs sharing one skeleton parse exactly
    /// once" while head-with-FaceGen calls still increment a
    /// telemetry counter.
    bypass_parses: u64,
    /// Insertion-order key tracker driving half-eviction on overflow
    /// (#3760) — same shape as `MaterialProvider::bgem_cache_order`.
    insertion_order: VecDeque<String>,
    /// Cap. `0` = unlimited (`BYRO_NIF_CACHE_MAX=0`). Read once at
    /// construction; defaults to [`DEFAULT_MAX_ENTRIES`] rather than
    /// `NifImportRegistry`'s 2048 — see the module doc.
    max_entries: usize,
    /// Overflow evictions across the process lifetime (each event
    /// removes up to `max_entries / 2` entries at once).
    evictions: u64,
}

impl Default for SceneImportCache {
    fn default() -> Self {
        Self::new()
    }
}

impl SceneImportCache {
    pub(crate) fn new() -> Self {
        let max_entries = std::env::var("BYRO_NIF_CACHE_MAX")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(DEFAULT_MAX_ENTRIES);
        if max_entries == 0 {
            log::warn!(
                "SceneImportCache: memory cap disabled (BYRO_NIF_CACHE_MAX=0). \
                 RAM usage grows without bound during long NPC-spawning sessions.",
            );
        }
        Self {
            core: ParsedNifCache::new(),
            bypass_parses: 0,
            insertion_order: VecDeque::new(),
            max_entries,
            evictions: 0,
        }
    }

    /// Look up a cached scene by lowercased path. Bumps the
    /// hit/miss counter inline (single-shot per call site —
    /// contrast with `cell_loader::load_references`'s batched
    /// accumulation pattern).
    pub(crate) fn get(&mut self, key: &str) -> Option<Option<Arc<ImportedScene>>> {
        let entry = self.core.get(key).cloned();
        if entry.is_some() {
            self.core.record_hit();
        } else {
            self.core.record_miss();
        }
        entry
    }

    /// Insert a freshly-parsed scene (or `None` for a parse failure /
    /// empty scene). Returns the inserted `Arc` (cloned) so the
    /// caller can keep walking the same allocation without a second
    /// lookup. `None` insertion returns `None`.
    ///
    /// #3760 — bounded by `max_entries`: once the cache would exceed
    /// the cap, evicts the oldest half (by insertion order) before
    /// inserting, same shape as `MaterialProvider`'s `bgem_cache`
    /// (#951/#1430). A no-op when `max_entries == 0` (the explicit
    /// `BYRO_NIF_CACHE_MAX=0` unlimited mode).
    pub(crate) fn insert(
        &mut self,
        key: String,
        value: Option<Arc<ImportedScene>>,
    ) -> Option<Arc<ImportedScene>> {
        let is_new_key = self.core.get(&key).is_none();
        if is_new_key && self.max_entries > 0 && self.core.len() >= self.max_entries {
            // Half-eviction: drop the oldest `max_entries / 2` distinct
            // keys so the cache doesn't immediately refill to the cap on
            // the very next insert. `max_entries / 2` is at least 1 for
            // any cap >= 2 (the only realistic configuration).
            for _ in 0..(self.max_entries / 2).max(1) {
                let Some(oldest) = self.insertion_order.pop_front() else {
                    break;
                };
                self.core.remove(&oldest);
            }
            self.evictions = self.evictions.saturating_add(1);
        }
        let to_return = value.clone();
        self.core.insert(key.clone(), value);
        if is_new_key {
            self.insertion_order.push_back(key);
        }
        to_return
    }

    /// Record a parse that bypassed the cache (currently only the
    /// `pre_spawn_hook = Some` path — head NIF with FaceGen morphs).
    /// Bumps `bypass_parses` AND the core's miss counter so the
    /// total `parses()` telemetry reflects every `parse_nif`
    /// invocation, cache-routed or otherwise.
    pub(crate) fn record_bypass_parse(&mut self) {
        self.bypass_parses = self.bypass_parses.saturating_add(1);
        self.core.record_miss();
    }

    /// Total parse_nif + import calls observed across the process
    /// lifetime: cache-miss inserts (every `Some(_)` entry plus
    /// negative-cached `None`) + hook-bypass parses. The cache's
    /// `parsed_count` + `failed_count` give the LIVE entry shape;
    /// `parses()` is the cumulative count that the regression test
    /// pins against ("spawn 10 NPCs sharing one skeleton, count
    /// `parse_nif` calls, assert exactly 1").
    #[cfg(test)]
    pub(crate) fn parses(&self) -> u64 {
        self.core
            .parsed_count()
            .saturating_add(self.core.failed_count())
            .saturating_add(self.bypass_parses)
    }

    #[cfg(test)]
    pub(crate) fn hits(&self) -> u64 {
        self.core.hits()
    }

    #[cfg(test)]
    pub(crate) fn misses(&self) -> u64 {
        self.core.misses()
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.core.len()
    }

    /// Configured cap (`0` = unlimited). #3760.
    #[cfg(test)]
    pub(crate) fn max_entries(&self) -> usize {
        self.max_entries
    }

    /// Half-eviction events across the process lifetime. #3760.
    #[cfg(test)]
    pub(crate) fn evictions(&self) -> u64 {
        self.evictions
    }
}

impl Resource for SceneImportCache {}

#[cfg(test)]
mod tests {
    //! Unit tests for the bookkeeping side of `SceneImportCache`. The
    //! Vulkan-bound integration (`load_nif_bytes_with_skeleton`
    //! cache-routing) is exercised by the live NPC spawn path; this
    //! module covers the pure-state invariants that drive the test
    //! plan from the issue: "spawn 10 NPCs sharing the same skeleton,
    //! count parse_nif calls, assert exactly 1 (cold) or 0
    //! (hot-cached)".
    //!
    //! The bookkeeping primitives themselves
    //! (`ParsedNifCache::insert` / `get` / counter math) are
    //! exercised by `parsed_nif_cache::tests`; this module tests the
    //! wrapper-level glue (bypass_parses tracking, parses()
    //! aggregation, get's hit/miss bumping).
    use super::*;
    use byroredux_nif::import::ImportedScene;

    fn empty_scene() -> Arc<ImportedScene> {
        Arc::new(ImportedScene {
            nodes: Vec::new(),
            meshes: Vec::new(),
            particle_emitters: Vec::new(),
            bsx_flags: None,
            bs_bound: None,
            attach_points: None,
            child_attach_connections: None,
            furniture_markers: Vec::new(),
            embedded_clip: None,
            ragdoll: None,
            lights: Vec::new(),
        })
    }

    /// Build a cache with an explicit cap and otherwise-default state.
    /// Used by every eviction test so they don't depend on reading
    /// `BYRO_NIF_CACHE_MAX` from the environment — same shape as
    /// `nif_import_registry_tests.rs::registry_with_cap`.
    fn cache_with_cap(max_entries: usize) -> SceneImportCache {
        SceneImportCache {
            core: ParsedNifCache::new(),
            bypass_parses: 0,
            insertion_order: VecDeque::new(),
            max_entries,
            evictions: 0,
        }
    }

    /// First insert → `parses()` == 1 (one parsed-count entry).
    /// Subsequent `get` calls produce hits and do NOT bump
    /// `parses()` — the same Arc is handed out without re-parsing.
    /// The "10 NPCs sharing one skeleton parse it once" invariant.
    #[test]
    fn cold_insert_then_warm_hits_only_parse_once() {
        let mut cache = SceneImportCache::new();
        assert_eq!(cache.parses(), 0);

        let arc = empty_scene();
        let returned = cache.insert("skeleton.nif".to_string(), Some(arc.clone()));
        assert!(returned.is_some());
        assert_eq!(cache.parses(), 1, "first insert is the cold parse");

        for _ in 0..9 {
            let hit = cache
                .get("skeleton.nif")
                .expect("present key")
                .expect("positive cache entry");
            assert!(Arc::ptr_eq(&hit, &arc), "cache must hand out the SAME Arc");
        }
        assert_eq!(cache.parses(), 1, "10 NPCs share one parse");
        assert_eq!(cache.hits(), 9);
        assert_eq!(cache.misses(), 0);
    }

    /// Negative cache: a `None` insert (parse failure / empty scene)
    /// makes subsequent `get` calls return `Some(None)` — the
    /// caller can distinguish "never tried" from "tried, parse
    /// failed" and skip the re-parse.
    #[test]
    fn negative_entry_prevents_reparse() {
        let mut cache = SceneImportCache::new();
        cache.insert("broken.nif".to_string(), None);
        assert_eq!(cache.parses(), 1, "negative-cache entry counts as a parse");

        let entry = cache.get("broken.nif").expect("present key");
        assert!(entry.is_none(), "negative entry signals known-failed parse");
        assert_eq!(
            cache.parses(),
            1,
            "warm hit must not re-parse a failed entry"
        );
    }

    /// `record_bypass_parse` bumps the bypass counter AND the core
    /// miss counter so the aggregate `parses()` reflects the full
    /// parse_nif invocation count. Mirrors the head-NIF-with-
    /// FaceGen path that intentionally skips caching for per-NPC
    /// morph uniqueness.
    #[test]
    fn bypass_parses_increment_counter_without_cache_growth() {
        let mut cache = SceneImportCache::new();
        let pre_len = cache.len();
        cache.record_bypass_parse();
        cache.record_bypass_parse();
        assert_eq!(cache.parses(), 2);
        assert_eq!(cache.misses(), 2, "bypass parses are misses too");
        assert_eq!(
            cache.len(),
            pre_len,
            "bypass parses do not populate the cache"
        );
    }

    /// Miss-then-insert flow: an unprimed `get` returns `None` and
    /// bumps the miss counter. The follow-up `insert` populates the
    /// slot, and a subsequent `get` is a hit.
    #[test]
    fn miss_then_insert_routes_correctly() {
        let mut cache = SceneImportCache::new();
        assert!(cache.get("body.nif").is_none(), "unprimed key returns None");
        assert_eq!(cache.misses(), 1);
        let _ = cache.insert("body.nif".to_string(), Some(empty_scene()));
        assert!(cache.get("body.nif").is_some());
        assert_eq!(cache.hits(), 1);
    }

    /// #3760 (SAFE-2026-08-30-D3-01) — pre-fix `SceneImportCache` had no
    /// cap at all; every distinct NPC skeleton/body/head/armor NIF path
    /// stayed resident until process exit. Insert past the cap and
    /// assert the entry count stops growing (the issue's own TESTS
    /// checklist wording).
    #[test]
    fn half_eviction_removes_oldest_entries_on_overflow() {
        let mut cache = cache_with_cap(4);
        for i in 0..4 {
            let _ = cache.insert(format!("mesh_{i}.nif"), Some(empty_scene()));
        }
        assert_eq!(cache.len(), 4);
        assert_eq!(cache.evictions(), 0);

        // The 5th distinct key overflows the cap — half-evicts the two
        // oldest (mesh_0, mesh_1) before inserting.
        let _ = cache.insert("mesh_4.nif".to_string(), Some(empty_scene()));
        assert_eq!(
            cache.len(),
            3,
            "half-eviction removes 2 of 4, then the new entry lands: 4-2+1=3"
        );
        assert_eq!(cache.evictions(), 1);
        assert!(
            cache.get("mesh_0.nif").is_none(),
            "oldest entry must have been evicted"
        );
        assert!(
            cache.get("mesh_1.nif").is_none(),
            "second-oldest entry must have been evicted"
        );
        assert!(
            cache.get("mesh_2.nif").is_some(),
            "third-oldest entry must survive"
        );
        assert!(
            cache.get("mesh_4.nif").is_some(),
            "the entry that triggered eviction must be present"
        );
    }

    /// A session that never approaches the cap sees zero evictions —
    /// half-eviction only fires on genuine overflow, not speculatively.
    /// Also pins the default cap value (`DEFAULT_MAX_ENTRIES`, deliberately
    /// much lower than `NifImportRegistry`'s 2048 given the heavier
    /// per-entry `ImportedScene` payload — see the module doc).
    #[test]
    fn small_session_under_cap_never_evicts() {
        let mut cache = SceneImportCache::new();
        assert_eq!(cache.max_entries(), DEFAULT_MAX_ENTRIES);
        for i in 0..50 {
            let _ = cache.insert(format!("mesh_{i}.nif"), Some(empty_scene()));
        }
        assert_eq!(cache.len(), 50);
        assert_eq!(cache.evictions(), 0);
    }

    /// `BYRO_NIF_CACHE_MAX=0` (unlimited, opt-in) disables the cap
    /// entirely — parity with `NifImportRegistry`'s escape hatch.
    #[test]
    fn unlimited_mode_never_evicts() {
        let mut cache = cache_with_cap(0);
        for i in 0..1000 {
            let _ = cache.insert(format!("mesh_{i}.nif"), Some(empty_scene()));
        }
        assert_eq!(cache.len(), 1000);
        assert_eq!(cache.evictions(), 0);
    }

    /// Re-inserting an already-cached key (overwrite) must not double-count
    /// it in the insertion-order tracker — otherwise a hot key that gets
    /// re-parsed (e.g. a negative-cache entry later corrected) would be
    /// evicted "twice" while a genuinely distinct key only counted once,
    /// undercounting how many real entries the cap actually holds.
    #[test]
    fn re_inserting_an_existing_key_does_not_inflate_the_eviction_queue() {
        let mut cache = cache_with_cap(4);
        let _ = cache.insert("a.nif".to_string(), Some(empty_scene()));
        let _ = cache.insert("a.nif".to_string(), Some(empty_scene())); // overwrite, not a new key
        let _ = cache.insert("b.nif".to_string(), Some(empty_scene()));
        let _ = cache.insert("c.nif".to_string(), Some(empty_scene()));
        assert_eq!(cache.len(), 3, "a.nif counts once despite two inserts");
        assert_eq!(cache.evictions(), 0, "3 distinct keys must not overflow a cap of 4");
    }
}
