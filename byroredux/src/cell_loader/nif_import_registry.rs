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
//! * [`NifImportRegistry`] is the `World` resource that owns a shared
//!   [`ParsedNifCache`] (the bookkeeping core — keys + lifetime
//!   counters) plus the LRU machinery (`access_tick`, `evictions`)
//!   and per-key animation-clip handle memoisation
//!   (`clip_handles`, #544 / #863).
//!
//! See #381 (process-lifetime promotion), #635 (LRU cap via
//! `BYRO_NIF_CACHE_MAX`), #523 (batched touch invariant), and #544
//! (clip-handle memoisation).

use std::collections::HashMap;
use std::sync::Arc;

use byroredux_core::ecs::{BillboardMode, Resource};

use crate::parsed_nif_cache::ParsedNifCache;

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
    /// root).
    pub(super) embedded_clip: Option<byroredux_nif::anim::AnimationClip>,
    /// Billboard mode to attach to the spawned placement root entity.
    /// `None` for ordinary meshes; `Some` for `NiBillboardNode`-rooted
    /// content and for SpeedTree `.spt` placeholders, which need the
    /// placement root to yaw-track the camera. Populated by the SPT
    /// importer from `ImportedNode { billboard_mode }`; the NIF path
    /// currently leaves this `None` (see #994 — NIF cell-loader has
    /// the same gap, deferred).
    pub(super) placement_root_billboard: Option<BillboardMode>,
}

/// Process-lifetime cache of parsed-and-imported NIF scenes keyed by
/// lowercased model path. Promotes the per-`load_references`
/// `import_cache` (#383) to a world-resource so cell-to-cell traversal
/// re-uses every previously-parsed mesh.
///
/// `None` entries (in `core.cache`) record a model that failed to
/// parse (or had zero useful geometry) so re-parses don't fire on
/// every placement.
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
    /// Shared bookkeeping core — entries, hits, misses, parsed/failed
    /// counts. Counterpart of `SceneImportCache.core`.
    pub(crate) core: ParsedNifCache<CachedNifImport>,
    /// Monotonic access tick per cached key. Bumped on every batched
    /// touch (insert + hit-set commits at end-of-load). Larger value =
    /// more recently accessed. Eviction picks the entry with the
    /// smallest tick when `core.len() > max_entries`.
    pub(super) access_tick: HashMap<String, u64>,
    /// Next access tick value to assign. Wraps at u64::MAX (~10^19
    /// touches — the cache will OOM long before this overflows).
    pub(super) next_tick: u64,
    /// LRU cap; `0` = unlimited (default). Read once at construction
    /// from the `BYRO_NIF_CACHE_MAX` env var.
    pub(super) max_entries: usize,
    /// LRU evictions across the process lifetime. Stays at 0 when
    /// `max_entries == 0` (unlimited mode).
    pub(crate) evictions: u64,
    /// Memoised `AnimationClipRegistry` handles per cache key (#544).
    /// Each parsed NIF whose `embedded_clip` is non-empty is registered
    /// with the global `AnimationClipRegistry` once and the resulting
    /// handle is stashed here so subsequent REFRs of the same model —
    /// within this cell or any future cell — re-use the same clip
    /// without re-converting the channel arrays. Cleared in lockstep
    /// with `core` and `access_tick` on `clear()` / LRU eviction so a
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
            core: ParsedNifCache::new(),
            access_tick: HashMap::new(),
            next_tick: 0,
            max_entries,
            evictions: 0,
            clip_handles: HashMap::new(),
        }
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

    /// Total number of cached entries (parsed + failed). Delegates to
    /// the shared core.
    pub(crate) fn len(&self) -> usize {
        self.core.len()
    }

    /// Configured LRU cap (`0` = unlimited).
    pub(crate) fn max_entries(&self) -> usize {
        self.max_entries
    }

    /// Hit rate as a percentage `[0, 100]`. Delegates to the shared
    /// core. `0.0` when no lookups have happened yet (avoid NaN).
    pub(crate) fn hit_rate_pct(&self) -> f64 {
        self.core.hit_rate_pct()
    }

    /// Look up `key` in the cache (read-only). Access-tick updates
    /// happen via [`Self::touch_keys`] in the batched end-of-load
    /// commit so the read path can stay on a shared lock. Hit/miss
    /// tracking is also batched (see [`Self::accumulate_hits`] /
    /// [`Self::accumulate_misses`]) — `get` itself does NOT bump
    /// the lifetime counters.
    pub(crate) fn get(&self, key: &str) -> Option<&Option<Arc<CachedNifImport>>> {
        self.core.get(key)
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
        std::sync::Arc::new(self.core.keys().cloned().collect())
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

    /// Add a per-call hit tally to the lifetime counter. Used by
    /// `cell_loader::load_references` to commit a whole cell's
    /// accumulated hit count under a single resource write lock at
    /// end-of-load (#523).
    pub(crate) fn accumulate_hits(&mut self, n: u64) {
        self.core.accumulate_hits(n);
    }

    /// Add a per-call miss tally to the lifetime counter (companion
    /// of [`Self::accumulate_hits`]).
    pub(crate) fn accumulate_misses(&mut self, n: u64) {
        self.core.accumulate_misses(n);
    }

    /// Insert (or overwrite) a cache entry, refresh its access tick,
    /// and evict LRU entries while `len > max_entries`. The eviction
    /// loop is a no-op when `max_entries == 0` (unlimited mode), so
    /// the default path stays O(1) per insert.
    ///
    /// Returns a `Vec<u32>` of `AnimationClipRegistry` handles whose
    /// owning cache entries were evicted by this call — the caller
    /// MUST forward each to
    /// [`byroredux_core::animation::AnimationClipRegistry::release`]
    /// to free the underlying keyframe arrays. Pre-#863 the eviction
    /// path silently dropped the path binding and the clip's keyframe
    /// memory leaked under long-running `BYRO_NIF_CACHE_MAX > 0`
    /// sessions. The `must_use` attribute makes a forgetful caller a
    /// compile warning rather than a silent leak. Empty Vec on the
    /// no-eviction path (the default `BYRO_NIF_CACHE_MAX=0` mode); no
    /// allocation cost there.
    #[must_use = "evicted clip handles must be released into AnimationClipRegistry to free their keyframe arrays — see #863"]
    pub(crate) fn insert(&mut self, key: String, value: Option<Arc<CachedNifImport>>) -> Vec<u32> {
        // Core handles parsed/failed counter adjustment + map insertion.
        self.core.insert(key.clone(), value);
        let t = self.next_tick;
        self.next_tick = self.next_tick.wrapping_add(1);
        self.access_tick.insert(key, t);

        let mut freed_clip_handles: Vec<u32> = Vec::new();
        if self.max_entries > 0 {
            while self.core.len() > self.max_entries {
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
                let _ = self.core.remove(&victim_key);
                self.access_tick.remove(&victim_key);
                // #544 — drop the memoised clip handle in lockstep so
                // a future re-parse of the same key registers a fresh
                // clip rather than reaching into the
                // `AnimationClipRegistry` for a stale handle pointing
                // at a clip that was logically discarded.
                //
                // #863 — capture the freed handle (if any) so the
                // caller can `release()` it into the
                // AnimationClipRegistry. Pre-fix the handle was
                // dropped on the floor and the keyframe arrays
                // leaked.
                if let Some(handle) = self.clip_handles.remove(&victim_key) {
                    freed_clip_handles.push(handle);
                }
                self.evictions = self.evictions.saturating_add(1);
            }
        }
        freed_clip_handles
    }
}

impl Resource for NifImportRegistry {}
