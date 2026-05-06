//! Regression tests for `finish_partial_import` — issue #864.
//!
//! The early-out at the top of `finish_partial_import` shorts the
//! main-thread import + clip-conversion + cache-insert pipeline when
//! `NifImportRegistry` already carries an entry for the model path.
//! Without it, a streaming-worker payload arriving for an already-
//! cached model (possible because the cached-keys snapshot in #862
//! lags the registry by the in-flight worker's parse latency) would
//! re-run `convert_nif_clip`, leak the previous clip handle into
//! `AnimationClipRegistry`, and overwrite the cache entry.

use super::*;
use byroredux_core::animation::AnimationClipRegistry;
use byroredux_core::ecs::World;
use byroredux_core::string::StringPool;

fn dummy_cached() -> Arc<CachedNifImport> {
    Arc::new(CachedNifImport {
        meshes: Vec::new(),
        collisions: Vec::new(),
        lights: Vec::new(),
        particle_emitters: Vec::new(),
        embedded_clip: None,
    })
}

fn dummy_partial() -> crate::streaming::PartialNifImport {
    crate::streaming::PartialNifImport {
        scene: byroredux_nif::scene::NifScene::default(),
        bsx: 0,
        lights: Vec::new(),
        particle_emitters: Vec::new(),
        embedded_clip: None,
    }
}

fn world_with_registries() -> World {
    let mut world = World::new();
    world.insert_resource(StringPool::new());
    world.insert_resource(AnimationClipRegistry::new());
    world.insert_resource(NifImportRegistry::new());
    world
}

/// Pre-cached positive entry — `finish_partial_import` must early-out
/// without touching `AnimationClipRegistry` or rebuilding the cached
/// import. The arc identity check verifies the cache entry wasn't
/// overwritten.
#[test]
fn finish_partial_import_early_outs_on_already_cached_positive_entry() {
    let mut world = world_with_registries();
    let original = dummy_cached();
    let original_ptr = Arc::as_ptr(&original) as usize;
    {
        let mut reg = world.resource_mut::<NifImportRegistry>();
        let _ = reg.insert("test.nif".to_string(), Some(original));
    }
    assert_eq!(world.resource::<NifImportRegistry>().len(), 1);
    assert_eq!(world.resource::<AnimationClipRegistry>().len(), 0);

    finish_partial_import(&mut world, None, None, "test.nif", dummy_partial());

    // Cache entry preserved (same Arc pointer — the early-out didn't
    // rebuild and overwrite).
    let reg = world.resource::<NifImportRegistry>();
    let entry = reg.get("test.nif").expect("cache entry preserved");
    let cached = entry.as_ref().expect("positive cache hit preserved");
    assert_eq!(
        Arc::as_ptr(cached) as usize,
        original_ptr,
        "early-out must NOT overwrite the cached Arc",
    );
    drop(reg);
    // AnimationClipRegistry untouched — convert_nif_clip + clip_reg.add
    // were correctly skipped.
    assert_eq!(
        world.resource::<AnimationClipRegistry>().len(),
        0,
        "early-out must skip clip conversion",
    );
}

/// Pre-cached NEGATIVE entry (failed parse memo) — same early-out
/// applies. Re-attempting the parse path would also be wasted work,
/// AND inserting a positive entry over the negative would let the
/// cache thrash between the two on alternating re-parses.
#[test]
fn finish_partial_import_early_outs_on_already_cached_negative_entry() {
    let mut world = world_with_registries();
    {
        let mut reg = world.resource_mut::<NifImportRegistry>();
        let _ = reg.insert("broken.nif".to_string(), None);
    }
    assert_eq!(world.resource::<NifImportRegistry>().len(), 1);

    finish_partial_import(&mut world, None, None, "broken.nif", dummy_partial());

    // Cache entry stays negative — the worker's payload (which would
    // have produced a positive entry) is dropped silently.
    let reg = world.resource::<NifImportRegistry>();
    let entry = reg.get("broken.nif").expect("cache entry preserved");
    assert!(entry.is_none(), "negative cache stays negative");
    drop(reg);
    assert_eq!(world.resource::<AnimationClipRegistry>().len(), 0);
}

/// Path-case round-trip: the cache key is lowercased on insert and on
/// lookup, so a model_path with mixed case still hits the early-out.
/// Catches any regression where `to_ascii_lowercase()` migration
/// breaks the key normalisation contract.
#[test]
fn finish_partial_import_early_outs_with_mixed_case_model_path() {
    let mut world = world_with_registries();
    {
        let mut reg = world.resource_mut::<NifImportRegistry>();
        let _ = reg.insert("rock_cliff.nif".to_string(), Some(dummy_cached()));
    }
    finish_partial_import(
        &mut world,
        None,
        None,
        "Rock_Cliff.NIF", // mixed case — should normalise to the same lowercase key
        dummy_partial(),
    );
    let reg = world.resource::<NifImportRegistry>();
    assert_eq!(reg.len(), 1, "early-out must not append a duplicate-case entry");
    assert!(reg.get("rock_cliff.nif").is_some());
}
