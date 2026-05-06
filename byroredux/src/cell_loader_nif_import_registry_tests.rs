//! Tests for `nif_import_registry_tests` extracted from ../cell_loader.rs (refactor stage A).
//!
//! Same qualified path preserved (`nif_import_registry_tests::FOO`).

//! Regression tests for #381 — process-lifetime NIF import cache.
//! Doesn't exercise the cell loader end-to-end (which would require
//! a real BSA + ESM); instead verifies the registry's hit/miss
//! counters and `hit_rate_pct` math, which is the contract the
//! `mesh.cache` debug command surfaces.
use super::*;

fn dummy_cached() -> Arc<CachedNifImport> {
    Arc::new(CachedNifImport {
        meshes: Vec::new(),
        collisions: Vec::new(),
        lights: Vec::new(),
        particle_emitters: Vec::new(),
        embedded_clip: None,
    })
}

#[test]
fn fresh_registry_has_zero_counters_and_zero_hit_rate() {
    let reg = NifImportRegistry::new();
    assert_eq!(reg.len(), 0);
    assert_eq!(reg.hits, 0);
    assert_eq!(reg.misses, 0);
    assert_eq!(reg.parsed_count, 0);
    assert_eq!(reg.failed_count, 0);
    // Avoid NaN when no lookups have happened.
    assert_eq!(reg.hit_rate_pct(), 0.0);
}

#[test]
fn hit_rate_reflects_hit_miss_ratio() {
    let mut reg = NifImportRegistry::new();
    // Simulate the cell-loader workflow: 1 miss + 3 hits on the
    // same model path → 75% lifetime hit rate.
    reg.cache.insert("torch.nif".into(), Some(dummy_cached()));
    reg.misses += 1;
    reg.parsed_count += 1;
    for _ in 0..3 {
        reg.hits += 1;
    }
    assert_eq!(reg.hits, 3);
    assert_eq!(reg.misses, 1);
    assert!((reg.hit_rate_pct() - 75.0).abs() < 1e-6);
}

#[test]
fn clear_drops_entries_but_preserves_lifetime_counters() {
    let mut reg = NifImportRegistry::new();
    reg.cache.insert("a".into(), Some(dummy_cached()));
    reg.cache.insert("b".into(), None);
    reg.parsed_count = 1;
    reg.failed_count = 1;
    reg.hits = 5;
    reg.misses = 2;

    reg.clear();
    assert_eq!(reg.len(), 0);
    assert_eq!(reg.parsed_count, 0);
    assert_eq!(reg.failed_count, 0);
    // Lifetime counters survive — debug command can still report
    // historical activity after a forced cache flush.
    assert_eq!(reg.hits, 5);
    assert_eq!(reg.misses, 2);
}

#[test]
fn failed_parse_entry_is_remembered_and_reused() {
    // The cell loader inserts `None` on parse failure so subsequent
    // placements of the same broken model don't re-attempt the parse.
    // Verifies the cache contract that lookups distinguish "not yet
    // tried" from "tried, failed".
    let mut reg = NifImportRegistry::new();
    reg.cache.insert("broken.nif".into(), None);
    reg.misses += 1;
    reg.failed_count += 1;

    // Subsequent get → Some(None) (entry exists, value is None) —
    // distinct from None (entry doesn't exist).
    let entry = reg.cache.get("broken.nif");
    assert!(matches!(entry, Some(None)));
    assert_eq!(reg.failed_count, 1);
    assert_eq!(reg.parsed_count, 0);
}

/// #635 / FNV-D3-05 — opt-in LRU eviction. With `max_entries == 0`
/// (the default, mirrors pre-#635 behavior) the cache grows without
/// bound and `evictions` stays at 0.
#[test]
fn unlimited_mode_never_evicts() {
    let mut reg = NifImportRegistry::new();
    assert_eq!(reg.max_entries(), 0, "default cap is unlimited");
    for i in 0..100u32 {
        reg.insert(format!("mesh_{i}.nif"), Some(dummy_cached()));
    }
    assert_eq!(reg.len(), 100);
    assert_eq!(reg.evictions, 0);
    assert_eq!(reg.parsed_count, 100);
}

/// #635 / FNV-D3-05 — explicit cap evicts the least-recently inserted
/// entry once the cache exceeds the threshold. Mirrors a doorwalking
/// session that touches more meshes than fit in the configured budget.
#[test]
fn lru_cap_evicts_least_recently_inserted_entry() {
    let mut reg = NifImportRegistry {
        cache: HashMap::new(),
        access_tick: HashMap::new(),
        next_tick: 0,
        max_entries: 3,
        hits: 0,
        misses: 0,
        parsed_count: 0,
        failed_count: 0,
        evictions: 0,
        clip_handles: HashMap::new(),
    };
    reg.insert("a.nif".into(), Some(dummy_cached()));
    reg.insert("b.nif".into(), Some(dummy_cached()));
    reg.insert("c.nif".into(), Some(dummy_cached()));
    assert_eq!(reg.len(), 3);
    assert_eq!(reg.evictions, 0);

    // Fourth insert evicts the oldest entry (a.nif).
    reg.insert("d.nif".into(), Some(dummy_cached()));
    assert_eq!(reg.len(), 3);
    assert_eq!(reg.evictions, 1);
    assert!(!reg.cache.contains_key("a.nif"), "a.nif must have been evicted");
    assert!(reg.cache.contains_key("b.nif"));
    assert!(reg.cache.contains_key("c.nif"));
    assert!(reg.cache.contains_key("d.nif"));
    assert_eq!(reg.parsed_count, 3, "evicted slot drops parsed_count");
}

/// #635 / FNV-D3-05 — `touch_keys` bumps the access tick of hit keys
/// so they survive an eviction sweep that would otherwise drop them.
/// Without this, an LRU cap would degrade to "least-recently inserted",
/// flushing out frequently-revisited shared meshes (door frames, sky
/// planes) on every cell load — exactly the case M40 doorwalking
/// can't afford.
#[test]
fn touch_keys_protects_recently_hit_entries_from_lru() {
    let mut reg = NifImportRegistry {
        cache: HashMap::new(),
        access_tick: HashMap::new(),
        next_tick: 0,
        max_entries: 3,
        hits: 0,
        misses: 0,
        parsed_count: 0,
        failed_count: 0,
        evictions: 0,
        clip_handles: HashMap::new(),
    };
    reg.insert("door.nif".into(), Some(dummy_cached())); // tick 0
    reg.insert("wall.nif".into(), Some(dummy_cached())); // tick 1
    reg.insert("sky.nif".into(), Some(dummy_cached()));  // tick 2

    // Simulate a cell load that hits door.nif again — it must rise
    // above wall.nif and sky.nif in LRU order.
    reg.touch_keys(["door.nif"].iter().copied());

    // Adding a fresh entry now evicts wall.nif (now the oldest tick).
    reg.insert("table.nif".into(), Some(dummy_cached()));
    assert_eq!(reg.evictions, 1);
    assert!(reg.cache.contains_key("door.nif"), "touched key must survive");
    assert!(!reg.cache.contains_key("wall.nif"), "untouched-and-oldest is the victim");
    assert!(reg.cache.contains_key("sky.nif"));
    assert!(reg.cache.contains_key("table.nif"));
}

/// #635 / FNV-D3-05 — `insert` keeps `parsed_count` and `failed_count`
/// in lockstep with the cache contents across overwrites. A failed
/// parse upgraded to a successful re-parse must move from the failed
/// bucket to the parsed bucket without leaking a phantom entry in
/// either counter.
#[test]
fn insert_overwrite_transitions_parsed_failed_counters() {
    let mut reg = NifImportRegistry::new();
    reg.insert("broken.nif".into(), None);
    assert_eq!(reg.parsed_count, 0);
    assert_eq!(reg.failed_count, 1);

    // Replace with a successful parse.
    reg.insert("broken.nif".into(), Some(dummy_cached()));
    assert_eq!(reg.parsed_count, 1);
    assert_eq!(reg.failed_count, 0);
    assert_eq!(reg.len(), 1);

    // Replace with a failed parse again (e.g., the BSA file rotted).
    reg.insert("broken.nif".into(), None);
    assert_eq!(reg.parsed_count, 0);
    assert_eq!(reg.failed_count, 1);
}

#[test]
fn batched_commit_matches_per_iteration_semantics() {
    // #523 regression — the batched commit path (`pending_new`
    // staging HashMap + single write lock) must produce the same
    // final counter state as the pre-fix per-iteration writes.
    // Simulates 5 REFRs across 2 unique model paths: chair.nif ×3,
    // lamp.nif ×2. Expected: 2 misses (unique parses) + 3 hits
    // (the subsequent encounters), all committed in one lock.
    let mut reg = NifImportRegistry::new();

    let mut this_call_hits: u64 = 0;
    let mut this_call_misses: u64 = 0;
    let mut this_call_parsed: u64 = 0;
    let mut pending_new: HashMap<String, Option<Arc<CachedNifImport>>> = HashMap::new();

    let refs = [
        "chair.nif",
        "lamp.nif",
        "chair.nif",
        "chair.nif",
        "lamp.nif",
    ];
    for path in refs {
        let key = path.to_string();
        if pending_new.contains_key(&key) {
            this_call_hits += 1;
        } else if reg.cache.contains_key(&key) {
            this_call_hits += 1;
        } else {
            // Simulate a successful parse.
            pending_new.insert(key, Some(dummy_cached()));
            this_call_misses += 1;
            this_call_parsed += 1;
        }
    }

    // Batched commit — mirrors the `resource_mut` write-lock scope
    // at the end of `load_references`.
    reg.hits += this_call_hits;
    reg.misses += this_call_misses;
    reg.parsed_count += this_call_parsed;
    for (k, v) in pending_new {
        reg.cache.insert(k, v);
    }

    assert_eq!(reg.hits, 3, "3 subsequent encounters (2 chairs + 1 lamp)");
    assert_eq!(reg.misses, 2, "2 unique parses");
    assert_eq!(reg.parsed_count, 2);
    assert_eq!(reg.len(), 2);
    assert!(reg.cache.contains_key("chair.nif"));
    assert!(reg.cache.contains_key("lamp.nif"));
    assert!(
        (reg.hit_rate_pct() - 60.0).abs() < 1e-6,
        "3 hits / 5 lookups = 60.0%, got {}",
        reg.hit_rate_pct()
    );
}

// ── #544: embedded animation clip handle cache ──────────────────────

/// #544 — `clip_handle_for` returns `None` for keys that have never
/// been registered so the spawn loop can fall through to its
/// "register now" branch on the first REFR of a new NIF.
#[test]
fn clip_handle_for_returns_none_for_unregistered_key() {
    let reg = NifImportRegistry::new();
    assert!(reg.clip_handle_for("torch.nif").is_none());
}

/// #544 — `set_clip_handle` round-trips through `clip_handle_for`
/// without touching the rest of the registry's state. Subsequent
/// REFRs of the same model — within a load or across cells — pick up
/// the memoised handle through the read-only lookup.
#[test]
fn set_clip_handle_round_trips_through_clip_handle_for() {
    let mut reg = NifImportRegistry::new();
    reg.set_clip_handle("torch.nif".into(), 7);
    assert_eq!(reg.clip_handle_for("torch.nif"), Some(7));
    assert_eq!(
        reg.clip_handle_for("OTHER.nif"),
        None,
        "lookup must not leak across keys"
    );
    // Cache content is unaffected — clip_handles is an orthogonal
    // index, not derived from `cache`.
    assert_eq!(reg.len(), 0);
}

/// #544 — re-registering a clip for the same key overwrites the
/// previous handle. Future cell loads of a NIF whose embedded clip
/// changes (cache eviction → re-parse) reach the fresh handle.
#[test]
fn set_clip_handle_overwrite_replaces_previous_value() {
    let mut reg = NifImportRegistry::new();
    reg.set_clip_handle("torch.nif".into(), 1);
    reg.set_clip_handle("torch.nif".into(), 42);
    assert_eq!(reg.clip_handle_for("torch.nif"), Some(42));
}

/// #544 — `clear()` drops the clip-handle map in lockstep with the
/// cache so a forced flush can never produce stale handles. The
/// `mesh.cache` operator command relies on this invariant when
/// triggering a manual reset.
#[test]
fn clear_drops_clip_handles_alongside_cache() {
    let mut reg = NifImportRegistry::new();
    reg.cache
        .insert("torch.nif".into(), Some(dummy_cached()));
    reg.parsed_count = 1;
    reg.set_clip_handle("torch.nif".into(), 12);
    assert_eq!(reg.clip_handle_for("torch.nif"), Some(12));

    reg.clear();
    assert_eq!(reg.len(), 0);
    assert!(
        reg.clip_handle_for("torch.nif").is_none(),
        "clear must drop the clip-handle map in lockstep with the cache"
    );
}

/// #544 — when the LRU sweep evicts a cache entry, its clip handle
/// must drop in the same step so a future re-parse of the key
/// registers a fresh clip. Without this, the registry would hand out
/// a stale handle pointing at a clip that's still in the
/// `AnimationClipRegistry` but is logically discarded; the player
/// would mis-bind silently. Mirrors the eviction-counter bookkeeping
/// in #635.
#[test]
fn lru_eviction_drops_clip_handle_for_victim() {
    let mut reg = NifImportRegistry {
        cache: HashMap::new(),
        access_tick: HashMap::new(),
        next_tick: 0,
        max_entries: 2,
        hits: 0,
        misses: 0,
        parsed_count: 0,
        failed_count: 0,
        evictions: 0,
        clip_handles: HashMap::new(),
    };
    reg.insert("a.nif".into(), Some(dummy_cached()));
    reg.set_clip_handle("a.nif".into(), 1);
    reg.insert("b.nif".into(), Some(dummy_cached()));
    reg.set_clip_handle("b.nif".into(), 2);
    assert_eq!(reg.clip_handle_for("a.nif"), Some(1));
    assert_eq!(reg.clip_handle_for("b.nif"), Some(2));

    // Triggers eviction of `a.nif` (least-recently inserted).
    reg.insert("c.nif".into(), Some(dummy_cached()));

    assert_eq!(reg.evictions, 1);
    assert!(
        !reg.cache.contains_key("a.nif"),
        "a.nif must have been evicted"
    );
    assert!(
        reg.clip_handle_for("a.nif").is_none(),
        "eviction must drop the matching clip handle so a future re-parse \
         registers a fresh clip rather than reaching for a stale handle"
    );
    // Surviving entries' handles untouched.
    assert_eq!(reg.clip_handle_for("b.nif"), Some(2));
}

/// Regression for #862 / FNV-D3-NEW-03: the cell-stream worker filters
/// its `model_paths` against `NifImportRegistry::snapshot_keys()` so a
/// 7×7 grid traversal in WastelandNV doesn't re-extract+parse every
/// shared rock / roadway / junkpile NIF on every cell crossing. The
/// snapshot must contain every cached key (positive AND negative
/// entries — known-failed parses don't get re-tried either).
#[test]
fn snapshot_keys_includes_positive_and_negative_cache_entries() {
    let mut reg = NifImportRegistry::new();
    reg.insert("rock_cliff.nif".into(), Some(dummy_cached()));
    reg.insert("missing_model.nif".into(), None); // negative cache
    reg.insert("junkpile.nif".into(), Some(dummy_cached()));

    let snap = reg.snapshot_keys();
    assert_eq!(snap.len(), 3);
    assert!(snap.contains("rock_cliff.nif"));
    assert!(snap.contains("missing_model.nif"));
    assert!(snap.contains("junkpile.nif"));
}

#[test]
fn snapshot_keys_returns_empty_set_for_fresh_registry() {
    let reg = NifImportRegistry::new();
    assert!(reg.snapshot_keys().is_empty());
}

#[test]
fn snapshot_keys_decoupled_from_registry_after_capture() {
    // Snapshot captures keys at call time; subsequent registry
    // mutations don't appear in the previously-returned Arc. This
    // matters because the cell-stream worker holds the snapshot
    // across its rayon parse loop while the main thread keeps
    // inserting new payloads — without decoupling, the worker would
    // see a moving target and produce non-deterministic skip
    // decisions.
    let mut reg = NifImportRegistry::new();
    reg.insert("a.nif".into(), Some(dummy_cached()));
    let snap_before = reg.snapshot_keys();
    assert_eq!(snap_before.len(), 1);

    reg.insert("b.nif".into(), Some(dummy_cached()));
    // The previously-captured snapshot must NOT see "b.nif".
    assert_eq!(snap_before.len(), 1);
    assert!(!snap_before.contains("b.nif"));
    // A fresh snapshot DOES see it.
    let snap_after = reg.snapshot_keys();
    assert_eq!(snap_after.len(), 2);
    assert!(snap_after.contains("b.nif"));
}
