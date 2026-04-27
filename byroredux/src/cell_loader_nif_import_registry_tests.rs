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
