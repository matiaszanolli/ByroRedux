//! Process-lifetime cache of parsed-and-imported NIF scenes.
//!
//! Cells frequently re-use the same model across hundreds of placements
//! (e.g. 40 chairs sharing one `chair.nif`); without a process-lifetime
//! cache every cell-load would re-parse every NIF, wasting CPU and
//! flooding the parser-warning log. [`NifImportRegistry`] is the
//! `World` resource that keeps the parsed scenes alive across cell
//! transitions.
//!
//! Two layers:
//!
//! * [`CachedNifImport`] holds the parsed-and-imported scene data for
//!   one model — meshes, collisions, lights, particle emitters, the
//!   embedded animation clip — wrapped in `Arc` so REFR placements
//!   share the same allocation.
//! * [`NifImportRegistry`] is the keyed `HashMap<String,
//!   Option<Arc<…>>>` plus an LRU-eviction tick map and per-key
//!   memoised animation-clip handles (#544).
//!
//! See #381 (process-lifetime promotion), #635 (LRU cap via
//! `BYRO_NIF_CACHE_MAX`), #523 (batched touch invariant), and #544
//! (clip-handle memoisation).

use std::collections::HashMap;
use std::sync::Arc;

use byroredux_core::ecs::Resource;

/// Parsed + imported NIF scene data cached per unique model path.
pub(crate) struct CachedNifImport {
    pub(super) meshes: Vec<byroredux_nif::import::ImportedMesh>,
    pub(super) collisions: Vec<byroredux_nif::import::ImportedCollision>,
    pub(super) lights: Vec<byroredux_nif::import::ImportedLight>,
    /// Particle emitters detected in the NIF scene graph
    /// (`NiParticleSystem` and friends). Carries NIF-local position +
    /// the nearest named ancestor's name. The spawn step composes the
    /// REFR placement on top and translates to a heuristic
    /// [`crate::components::ParticleEmitter`] preset. See #401.
    pub(super) particle_emitters: Vec<byroredux_nif::import::ImportedParticleEmitterFlat>,
    /// Ambient animation clip collecting every mesh-embedded controller
    /// (alpha fade, UV scroll, visibility flicker, material colour
    /// pulse, shader float/colour). Shared across REFR placements: the
    /// clip handle is registered once per cache load; each placement
    /// spawns its own `AnimationPlayer` scoped to the spawned root
    /// entity so the subtree-local name lookup matches the authored
    /// node names. `None` when the NIF authored no supported
    /// controllers. See #261.
    ///
    /// Currently write-only on the cell-loader path (the spawn doesn't
    /// yet attach `Name` components or parent meshes under a placement
    /// root). Field is retained so the follow-up wiring pass doesn't
    /// have to re-thread the parser.
    #[allow(dead_code)]
    pub(super) embedded_clip: Option<byroredux_nif::anim::AnimationClip>,
}

/// Process-lifetime cache of parsed-and-imported NIF scenes keyed by
/// lowercased model path. Promotes the per-`load_references`
/// `import_cache` (#383) to a world-resource so cell-to-cell traversal
/// re-uses every previously-parsed mesh.
///
/// `None` entries record a model that failed to parse (or had zero
/// useful geometry) so re-parses don't fire on every placement.
///
/// **Memory bound:** opt-in LRU via `BYRO_NIF_CACHE_MAX` env var (#635
/// / FNV-D3-05). Default `0` = unlimited, matching pre-#635 behaviour
/// so short-session loads aren't penalised. Setting
/// `BYRO_NIF_CACHE_MAX=N` caps the cache at N entries; the LRU victim
/// is the entry with the smallest access tick (least-recently inserted
/// *or* hit). Eviction counts surface in the `mesh.cache` debug
/// command. M40 doorwalking is the first consumer that genuinely needs
/// the cap — the engine's other registries (texture, mesh) are
/// similarly unbounded today.
pub(crate) struct NifImportRegistry {
    pub(super) cache: HashMap<String, Option<Arc<CachedNifImport>>>,
    /// Monotonic access tick per cached key. Bumped on every batched
    /// touch (insert + hit-set commits at end-of-load). Larger value =
    /// more recently accessed. Eviction picks the entry with the
    /// smallest tick when `cache.len() > max_entries`.
    pub(super) access_tick: HashMap<String, u64>,
    /// Next access tick value to assign. Wraps at u64::MAX (~10^19
    /// touches — the cache will OOM long before this overflows).
    pub(super) next_tick: u64,
    /// LRU cap; `0` = unlimited (default). Read once at construction
    /// from the `BYRO_NIF_CACHE_MAX` env var.
    pub(super) max_entries: usize,
    pub(crate) hits: u64,
    pub(crate) misses: u64,
    /// Successfully-parsed entries currently in the cache. Mirrors
    /// `cache.values().filter(|v| v.is_some()).count()` for O(1) reads
    /// from the `mesh.cache` debug command.
    pub(crate) parsed_count: u64,
    /// Failed-parse entries currently in the cache (`None` entries).
    pub(crate) failed_count: u64,
    /// LRU evictions across the process lifetime. Stays at 0 when
    /// `max_entries == 0` (unlimited mode).
    pub(crate) evictions: u64,
    /// Memoised `AnimationClipRegistry` handles per cache key (#544).
    /// Each parsed NIF whose `embedded_clip` is non-empty is registered
    /// with the global `AnimationClipRegistry` once and the resulting
    /// handle is stashed here so subsequent REFRs of the same model —
    /// within this cell or any future cell — re-use the same clip
    /// without re-converting the channel arrays. Cleared in lockstep
    /// with `cache` and `access_tick` on `clear()` / LRU eviction so a
    /// stale handle can never reach the player after the underlying
    /// cache entry has been dropped.
    pub(super) clip_handles: HashMap<String, u32>,
}

impl Default for NifImportRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl NifImportRegistry {
    pub(crate) fn new() -> Self {
        // `BYRO_NIF_CACHE_MAX=0` and a missing env var both map to
        // "unlimited" — the eviction loop is gated on `max_entries > 0`
        // so the unlimited path stays allocation-free aside from the
        // `access_tick` HashMap inserts.
        let max_entries = std::env::var("BYRO_NIF_CACHE_MAX")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);
        Self {
            cache: HashMap::new(),
            access_tick: HashMap::new(),
            next_tick: 0,
            max_entries,
            hits: 0,
            misses: 0,
            parsed_count: 0,
            failed_count: 0,
            evictions: 0,
            clip_handles: HashMap::new(),
        }
    }

    /// Clear all entries (e.g. before a hard world reset). Lifetime
    /// counters (hits/misses/evictions) are preserved so the debug
    /// command can still display historical activity.
    #[allow(dead_code)]
    pub(crate) fn clear(&mut self) {
        self.cache.clear();
        self.access_tick.clear();
        self.clip_handles.clear();
        self.parsed_count = 0;
        self.failed_count = 0;
    }

    /// Look up a previously-registered embedded-clip handle for `key`.
    /// Returns `None` when the cache key has never been parsed, when
    /// the parsed NIF authored no controllers, or when the entry was
    /// evicted by the LRU sweep. The lookup is read-only — used in the
    /// spawn hot path before falling through to the (write-locked)
    /// registration site. See #544.
    pub(crate) fn clip_handle_for(&self, key: &str) -> Option<u32> {
        self.clip_handles.get(key).copied()
    }

    /// Memoise an embedded-clip handle for `key`. Called once per
    /// unique parsed NIF that authored controllers — the spawn loop
    /// then reads the handle through [`Self::clip_handle_for`] without
    /// re-converting the channel arrays. See #544.
    pub(crate) fn set_clip_handle(&mut self, key: String, handle: u32) {
        self.clip_handles.insert(key, handle);
    }

    /// Total number of cached entries (parsed + failed).
    pub(crate) fn len(&self) -> usize {
        self.cache.len()
    }

    /// Configured LRU cap (`0` = unlimited).
    pub(crate) fn max_entries(&self) -> usize {
        self.max_entries
    }

    /// Hit rate as a percentage `[0, 100]`. `0.0` when no lookups have
    /// happened yet (avoid NaN).
    pub(crate) fn hit_rate_pct(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            100.0 * self.hits as f64 / total as f64
        }
    }

    /// Look up `key` in the cache (read-only). Access-tick updates
    /// happen via [`Self::touch_keys`] in the batched end-of-load
    /// commit so the read path can stay on a shared lock.
    pub(crate) fn get(&self, key: &str) -> Option<&Option<Arc<CachedNifImport>>> {
        self.cache.get(key)
    }

    /// Snapshot every cached key (positive + negative entries) into an
    /// `Arc<HashSet<String>>` the cell-stream worker can consult to
    /// skip BSA-extract + parse for models the main thread already
    /// holds. See #862 / FNV-D3-NEW-03 — without this, a 7×7 grid
    /// crossing in WastelandNV re-parses every shared rock / roadway /
    /// junkpile NIF on every cell boundary even though >95% are
    /// cached. Snapshot building is O(N) on registry size and runs at
    /// most once per cell-crossing batch, so the cost is bounded.
    ///
    /// Negative entries (`Some(None)` — failed parses) are included so
    /// the worker doesn't pointlessly retry a known-failed parse.
    pub(crate) fn snapshot_keys(&self) -> std::sync::Arc<std::collections::HashSet<String>> {
        std::sync::Arc::new(self.cache.keys().cloned().collect())
    }

    /// Bump the access tick for every key in `keys` so freshly-hit
    /// entries are protected from LRU eviction. Called from the
    /// end-of-load batched commit (one write lock instead of one per
    /// REFR — preserves #523's batching invariant).
    pub(crate) fn touch_keys<'a, I: IntoIterator<Item = &'a str>>(&mut self, keys: I) {
        for key in keys {
            if self.access_tick.contains_key(key) {
                let t = self.next_tick;
                self.next_tick = self.next_tick.wrapping_add(1);
                self.access_tick.insert(key.to_string(), t);
            }
        }
    }

    /// Insert (or overwrite) a cache entry, refresh its access tick,
    /// adjust `parsed_count` / `failed_count` to reflect the
    /// post-insert state, and evict LRU entries while
    /// `len > max_entries`. The eviction loop is a no-op when
    /// `max_entries == 0` (unlimited mode), so the default path stays
    /// O(1) per insert.
    pub(crate) fn insert(&mut self, key: String, value: Option<Arc<CachedNifImport>>) {
        // Adjust parsed/failed state so they stay in lockstep with
        // `cache.values().filter(|v| v.is_some()).count()`.
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
        self.cache.insert(key.clone(), value);
        let t = self.next_tick;
        self.next_tick = self.next_tick.wrapping_add(1);
        self.access_tick.insert(key, t);

        if self.max_entries > 0 {
            while self.cache.len() > self.max_entries {
                // O(N) sweep over `access_tick`. For caches sized in
                // the low thousands this is ~50 ns × N. A min-heap
                // would bookkeep on every touch for the unlimited
                // path's benefit only.
                let victim = self
                    .access_tick
                    .iter()
                    .min_by_key(|(_, &tick)| tick)
                    .map(|(k, _)| k.clone());
                let Some(victim_key) = victim else {
                    break;
                };
                let removed = self.cache.remove(&victim_key);
                self.access_tick.remove(&victim_key);
                // #544 — drop the memoised clip handle in lockstep so
                // a future re-parse of the same key registers a fresh
                // clip rather than reaching into the
                // `AnimationClipRegistry` for a stale handle pointing
                // at a clip that was logically discarded.
                self.clip_handles.remove(&victim_key);
                match removed {
                    Some(Some(_)) => {
                        self.parsed_count = self.parsed_count.saturating_sub(1);
                    }
                    Some(None) => {
                        self.failed_count = self.failed_count.saturating_sub(1);
                    }
                    None => {}
                }
                self.evictions = self.evictions.saturating_add(1);
            }
        }
    }
}

impl Resource for NifImportRegistry {}
