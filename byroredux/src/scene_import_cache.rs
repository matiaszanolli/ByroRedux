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
//! separate cache. See #880 / CELL-PERF-02.
//!
//! `pre_spawn_hook` complication: head NIFs with FaceGen morphs apply
//! per-NPC mutations to `imported.meshes[i].positions`. The cache is
//! consulted only when `pre_spawn_hook` is `None` — skeleton, body,
//! hand, and head-without-morph spawns hit the cache; head-with-morph
//! stays on the legacy parse-per-call path. This still captures
//! ≥ 6/7 of the audit's ~280 redundant parses.

use std::collections::HashMap;
use std::sync::Arc;

use byroredux_core::ecs::Resource;
use byroredux_nif::import::ImportedScene;

/// Cache of `Arc<ImportedScene>` keyed by lowercased model path.
///
/// `None` entries record a model that failed to parse (or had zero
/// useful geometry) so re-parses don't fire on every NPC spawn.
pub(crate) struct SceneImportCache {
    cache: HashMap<String, Option<Arc<ImportedScene>>>,
    /// Total number of `parse_nif` + `import_nif_scene_with_resolver`
    /// invocations triggered by this cache (cache misses + hook-bypass
    /// calls registered via `record_bypass_parse`). Used by the
    /// regression test to pin "10 NPCs sharing one skeleton parse the
    /// scene exactly once".
    parses: u64,
    hits: u64,
    misses: u64,
}

impl Default for SceneImportCache {
    fn default() -> Self {
        Self::new()
    }
}

impl SceneImportCache {
    pub(crate) fn new() -> Self {
        Self {
            cache: HashMap::new(),
            parses: 0,
            hits: 0,
            misses: 0,
        }
    }

    /// Look up a cached scene by lowercased path. Increments the
    /// hit/miss counters in lockstep so the regression test can assert
    /// the cache-routing invariant.
    pub(crate) fn get(&mut self, key: &str) -> Option<Option<Arc<ImportedScene>>> {
        let entry = self.cache.get(key).cloned();
        if entry.is_some() {
            self.hits = self.hits.saturating_add(1);
        } else {
            self.misses = self.misses.saturating_add(1);
        }
        entry
    }

    /// Insert a freshly-parsed scene (or `None` for a parse failure /
    /// empty scene). Bumps the `parses` counter — every insert
    /// represents one `parse_nif` + import call. Returns the inserted
    /// `Arc` so the caller can keep walking the same allocation
    /// without a second lookup. `None` insertion returns `None`.
    pub(crate) fn insert(
        &mut self,
        key: String,
        value: Option<Arc<ImportedScene>>,
    ) -> Option<Arc<ImportedScene>> {
        self.parses = self.parses.saturating_add(1);
        let to_return = value.clone();
        self.cache.insert(key, value);
        to_return
    }

    /// Record a parse that bypassed the cache (currently only the
    /// `pre_spawn_hook = Some` path — head NIF with FaceGen morphs).
    /// Bumps `parses` and `misses` so the counter reflects total
    /// parse_nif invocations, not just cache-driven ones. Tests pin
    /// the bypass-parse rate by mode.
    pub(crate) fn record_bypass_parse(&mut self) {
        self.parses = self.parses.saturating_add(1);
        self.misses = self.misses.saturating_add(1);
    }

    /// Total parse_nif + import calls observed across the process
    /// lifetime (cache-miss inserts + hook-bypass parses). Surfaced
    /// for the regression test (#880) and for telemetry / debug
    /// commands. `#[allow(dead_code)]` because the production
    /// surface today only consumes the cache via `get` / `insert` /
    /// `record_bypass_parse`; these accessors back the unit tests
    /// and a future `mesh.cache` debug command.
    #[allow(dead_code)]
    pub(crate) fn parses(&self) -> u64 {
        self.parses
    }

    #[allow(dead_code)]
    pub(crate) fn hits(&self) -> u64 {
        self.hits
    }

    #[allow(dead_code)]
    pub(crate) fn misses(&self) -> u64 {
        self.misses
    }

    #[allow(dead_code)]
    pub(crate) fn len(&self) -> usize {
        self.cache.len()
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
    use super::*;
    use byroredux_nif::import::ImportedScene;

    fn empty_scene() -> Arc<ImportedScene> {
        Arc::new(ImportedScene {
            nodes: Vec::new(),
            meshes: Vec::new(),
            particle_emitters: Vec::new(),
            bsx_flags: None,
            bs_bound: None,
            embedded_clip: None,
        })
    }

    /// First insert bumps `parses` to 1 (cold parse). Subsequent
    /// `get` of the same key produces a hit and does NOT bump
    /// `parses` — the cached `Arc` is returned without re-parsing.
    /// This is the "10 NPCs sharing one skeleton parse it once"
    /// invariant from the audit.
    #[test]
    fn cold_insert_then_warm_hits_only_parse_once() {
        let mut cache = SceneImportCache::new();
        assert_eq!(cache.parses(), 0);

        // Cold miss → caller parses + inserts.
        let arc = empty_scene();
        let returned = cache.insert("skeleton.nif".to_string(), Some(arc.clone()));
        assert!(returned.is_some());
        assert_eq!(cache.parses(), 1, "first insert is the cold parse");

        // 9 warm hits — none should bump `parses`.
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
        assert_eq!(cache.parses(), 1);

        let entry = cache.get("broken.nif").expect("present key");
        assert!(entry.is_none(), "negative entry signals known-failed parse");
        assert_eq!(cache.parses(), 1, "warm hit must not re-parse a failed entry");
    }

    /// `record_bypass_parse` bumps the `parses` counter without
    /// touching the cache map — mirrors the head-NIF-with-FaceGen
    /// path that intentionally skips caching for per-NPC morph
    /// uniqueness. Pinned so tests can pin the EXACT parse count
    /// across both cache-routed and bypass-routed call patterns.
    #[test]
    fn bypass_parses_increment_counter_without_cache_growth() {
        let mut cache = SceneImportCache::new();
        let pre_len = cache.len();
        cache.record_bypass_parse();
        cache.record_bypass_parse();
        assert_eq!(cache.parses(), 2);
        assert_eq!(cache.misses(), 2);
        assert_eq!(cache.len(), pre_len, "bypass parses do not populate the cache");
    }

    /// Miss-then-insert flow: an unprimed `get` returns `None`
    /// (signalling the caller should parse + insert). The follow-up
    /// `insert` populates the slot for subsequent NPCs.
    #[test]
    fn miss_then_insert_routes_correctly() {
        let mut cache = SceneImportCache::new();
        assert!(cache.get("body.nif").is_none(), "unprimed key returns None");
        assert_eq!(cache.misses(), 1);
        let _ = cache.insert("body.nif".to_string(), Some(empty_scene()));
        let hit = cache.get("body.nif");
        assert!(hit.is_some());
    }
}
