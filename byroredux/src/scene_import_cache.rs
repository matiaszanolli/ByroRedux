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

use std::sync::Arc;

use byroredux_core::ecs::Resource;
use byroredux_nif::import::ImportedScene;

use crate::parsed_nif_cache::ParsedNifCache;

/// Wrapper around the shared `ParsedNifCache` core that adds the
/// bypass-parse counter for the head-FaceGen path that intentionally
/// skips the cache.
pub(crate) struct SceneImportCache {
    core: ParsedNifCache<ImportedScene>,
    /// Parses recorded via [`Self::record_bypass_parse`] —
    /// pre_spawn_hook = Some path that skipped the cache. Tracked
    /// separately from the core's lifetime hits/misses so the test
    /// plan can pin "10 NPCs sharing one skeleton parse exactly
    /// once" while head-with-FaceGen calls still increment a
    /// telemetry counter.
    bypass_parses: u64,
}

impl Default for SceneImportCache {
    fn default() -> Self {
        Self::new()
    }
}

impl SceneImportCache {
    pub(crate) fn new() -> Self {
        Self {
            core: ParsedNifCache::new(),
            bypass_parses: 0,
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
    pub(crate) fn insert(
        &mut self,
        key: String,
        value: Option<Arc<ImportedScene>>,
    ) -> Option<Arc<ImportedScene>> {
        let to_return = value.clone();
        self.core.insert(key, value);
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
    #[allow(dead_code)]
    pub(crate) fn parses(&self) -> u64 {
        self.core
            .parsed_count()
            .saturating_add(self.core.failed_count())
            .saturating_add(self.bypass_parses)
    }

    #[allow(dead_code)]
    pub(crate) fn hits(&self) -> u64 {
        self.core.hits()
    }

    #[allow(dead_code)]
    pub(crate) fn misses(&self) -> u64 {
        self.core.misses()
    }

    #[allow(dead_code)]
    pub(crate) fn len(&self) -> usize {
        self.core.len()
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
            embedded_clip: None,
        })
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
}
