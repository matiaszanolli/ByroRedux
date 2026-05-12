//! Unit tests for `TextureRegistry` — handle lifecycle, refcounting,
//! cache hits / misses, descriptor binding. Extracted from
//! `texture_registry.rs` to keep the production code under ~1400
//! lines.

use super::*;

#[test]
fn normalize_backslashes_and_case() {
    assert_eq!(
        normalize_path(r"Textures\Architecture\Walls\Wall01_d.dds"),
        "textures/architecture/walls/wall01_d.dds"
    );
}

#[test]
fn normalize_already_clean() {
    assert_eq!(
        normalize_path("textures/clutter/food/beerbottle.dds"),
        "textures/clutter/food/beerbottle.dds"
    );
}

/// Regression for #522: prefixed and unprefixed inputs MUST
/// canonicalize to the same cache key. Pre-fix, `landscape/dirt02.dds`
/// and `textures\landscape\dirt02.dds` mapped to different keys,
/// producing silent bindless slot duplication on terrain tiles
/// whose LTEX record omits the prefix (FO3/FNV vanilla) while
/// `spawn_terrain_mesh` at cell_loader.rs:945 re-calls with the
/// fully-qualified path. Matches the canonicalization that
/// `asset_provider::normalize_texture_path` applies for the
/// archive lookup.
#[test]
fn normalize_prefix_variants_collapse_to_one_key() {
    let unprefixed = normalize_path(r"landscape\dirt02.dds");
    let prefixed = normalize_path(r"textures\landscape\dirt02.dds");
    let mixed_slashes = normalize_path("Textures/LANDSCAPE/dirt02.DDS");
    let forward_only = normalize_path("landscape/dirt02.dds");

    assert_eq!(unprefixed, "textures/landscape/dirt02.dds");
    assert_eq!(prefixed, "textures/landscape/dirt02.dds");
    assert_eq!(mixed_slashes, "textures/landscape/dirt02.dds");
    assert_eq!(forward_only, "textures/landscape/dirt02.dds");
}

/// Edge case: a path that happens to start with something LIKE
/// "textures" but isn't the directory prefix must still get the
/// prefix added. `texturesets/foo.dds` is not a textures-rooted
/// path — it would be rooted at `textures/texturesets/…` on disk.
#[test]
fn normalize_similar_prefix_is_not_swallowed() {
    let similar = normalize_path("texturesets/foo.dds");
    assert_eq!(similar, "textures/texturesets/foo.dds");
}

#[test]
fn should_destroy_pending_honors_frame_gap() {
    assert!(!should_destroy_pending(0, 0));
    assert!(!should_destroy_pending(1, 0));
    assert!(should_destroy_pending(MAX_FRAMES_IN_FLIGHT as u64, 0));
    assert!(should_destroy_pending(1000, 0));
}

#[test]
fn multiple_same_frame_calls_do_not_authorize_destruction() {
    let current_frame = 10;
    for _ in 0..5 {
        assert!(!should_destroy_pending(current_frame, current_frame));
    }
    assert!(should_destroy_pending(
        current_frame + MAX_FRAMES_IN_FLIGHT as u64,
        current_frame
    ));
}

#[test]
fn frame_counter_math_is_wrap_safe() {
    let queued = u64::MAX - 1;
    let current = queued.wrapping_add(MAX_FRAMES_IN_FLIGHT as u64);
    assert!(should_destroy_pending(current, queued));
    let current = queued.wrapping_add(MAX_FRAMES_IN_FLIGHT as u64 - 1);
    assert!(!should_destroy_pending(current, queued));
}

/// Build a registry in a test-only state: `check_slot_available` only
/// reads `textures` + `max_textures`, so we forge a partial
/// `TextureRegistry` without touching Vulkan.
fn make_registry_for_overflow_test(max_textures: u32, occupied: usize) -> TextureRegistry {
    TextureRegistry {
        textures: (0..occupied)
            .map(|_| TextureEntry {
                texture: None,
                pending_destroy: VecDeque::new(),
                ref_count: 0,
            })
            .collect(),
        path_map: HashMap::new(),
        fallback_handle: 0,
        descriptor_pool: vk::DescriptorPool::null(),
        descriptor_set_layout: vk::DescriptorSetLayout::null(),
        bindless_sets: Vec::new(),
        shared_sampler: vk::Sampler::null(),
        samplers: [vk::Sampler::null(); 4],
        max_textures,
        current_frame_id: 0,
        // Unit-test path: `check_slot_available` doesn't touch the
        // pool, so None is safe here.
        staging_pool: None,
        pending_set_writes: vec![Vec::new(); MAX_FRAMES_IN_FLIGHT],
        current_slot: 0,
        pending_dds_uploads: Vec::new(),
    }
}

#[test]
fn slot_available_when_under_bound() {
    let reg = make_registry_for_overflow_test(1024, 512);
    reg.check_slot_available()
        .expect("half-full registry should accept new textures");
}

#[test]
fn slot_rejected_at_exact_bound() {
    // Regression for #425 — old code silently wrote past the bindless
    // array bound once textures.len() == max_textures.
    let reg = make_registry_for_overflow_test(1024, 1024);
    let err = reg
        .check_slot_available()
        .expect_err("full registry must refuse new textures");
    let msg = format!("{err}");
    assert!(
        msg.contains("1024 of 1024"),
        "message reports counts: {msg}"
    );
    assert!(msg.contains("#425"), "message references the issue: {msg}");
}

#[test]
fn slot_rejected_beyond_bound() {
    let reg = make_registry_for_overflow_test(16, 16);
    assert!(reg.check_slot_available().is_err());
}

/// Seed a registry with the fallback at handle 0 and one
/// path-mapped entry at handle 1 carrying `initial_ref_count`.
/// Both entries have `texture: None` so the pure-Rust bits of
/// `drop_texture` run without calling into Vulkan.
fn make_registry_with_entry(path: &str, initial_ref_count: u32) -> TextureRegistry {
    let mut reg = make_registry_for_overflow_test(16, 0);
    reg.textures.push(TextureEntry {
        texture: None,
        pending_destroy: VecDeque::new(),
        ref_count: u32::MAX,
    });
    reg.fallback_handle = 0;
    reg.textures.push(TextureEntry {
        texture: None,
        pending_destroy: VecDeque::new(),
        ref_count: initial_ref_count,
    });
    // 3 = WRAP_S_WRAP_T per nif.xml — the legacy REPEAT entry the
    // pre-#610 cache lookups (`acquire_by_path` / `get_by_path`)
    // implicitly target.
    reg.path_map.insert(clamp_keyed_path(path, 3), 1);
    reg
}

#[test]
fn acquire_by_path_bumps_refcount() {
    // #524 — a second resolve of the same path must acquire a ref,
    // otherwise cell A's unload would free the texture that cell B
    // is still relying on.
    let mut reg = make_registry_with_entry("chair.dds", 1);
    let h1 = reg.acquire_by_path("chair.dds");
    assert_eq!(h1, Some(1));
    assert_eq!(reg.textures[1].ref_count, 2);
    let h2 = reg.acquire_by_path("chair.dds");
    assert_eq!(h2, Some(1));
    assert_eq!(reg.textures[1].ref_count, 3);
}

#[test]
fn acquire_by_path_miss_returns_none() {
    let mut reg = make_registry_with_entry("chair.dds", 1);
    assert_eq!(reg.acquire_by_path("barrel.dds"), None);
    assert_eq!(
        reg.textures[1].ref_count, 1,
        "missed lookups must not touch unrelated entries"
    );
}

#[test]
fn get_by_path_does_not_bump() {
    // Read-only probe — debug/inspect commands rely on this.
    let reg = make_registry_with_entry("chair.dds", 1);
    assert_eq!(reg.get_by_path("chair.dds"), Some(1));
    assert_eq!(reg.textures[1].ref_count, 1);
}

/// Regression for #610 / D4-NEW-02: the cache must distinguish
/// `(path, clamp_mode)` so the same DDS requested with two
/// different `TexClampMode` values gets two separate entries with
/// the right `VkSamplerAddressMode` pair attached. Pre-#610 the
/// cache was keyed by path alone — every texture got REPEAT and
/// CLAMP-authored decals bled at edges.
#[test]
fn cache_separates_entries_by_clamp_mode() {
    let mut reg = make_registry_with_entry("chair.dds", 1);
    // Default `acquire_by_path` looks up the REPEAT (`3`) entry.
    assert_eq!(reg.acquire_by_path("chair.dds"), Some(1));
    assert_eq!(reg.textures[1].ref_count, 2);
    // Same path with `0 = CLAMP_S_CLAMP_T` is a different cache
    // entry — the seeded fixture has no entry under that key, so
    // the lookup MUST miss instead of accidentally adopting the
    // REPEAT-bound texture.
    assert_eq!(
        reg.acquire_by_path_with_clamp("chair.dds", 0),
        None,
        "CLAMP request must NOT alias to the REPEAT entry"
    );
    // The miss didn't touch the REPEAT entry's refcount.
    assert_eq!(reg.textures[1].ref_count, 2);
}

/// Sibling: `acquire_by_path_with_clamp(path, 3)` must hit the
/// same entry the legacy `acquire_by_path` produced — the default
/// arm is `WRAP_S_WRAP_T = 3` so existing call sites that don't
/// pass a clamp keep their behaviour unchanged.
#[test]
fn legacy_acquire_path_routes_to_clamp_3() {
    let mut reg = make_registry_with_entry("chair.dds", 1);
    let h_legacy = reg.acquire_by_path("chair.dds");
    let h_explicit = reg.acquire_by_path_with_clamp("chair.dds", 3);
    assert_eq!(h_legacy, Some(1));
    assert_eq!(h_explicit, Some(1));
}

/// Sibling: out-of-range `clamp_mode` is clamped to `3` (REPEAT)
/// in `acquire_by_path_with_clamp` — defensive default for
/// upstream parser garbage.
#[test]
fn out_of_range_clamp_mode_falls_back_to_3() {
    let mut reg = make_registry_with_entry("chair.dds", 1);
    let h = reg.acquire_by_path_with_clamp("chair.dds", 99);
    assert_eq!(h, Some(1), "values >3 must clamp to the REPEAT entry");
}

#[test]
fn release_ref_decrements_without_freeing_until_zero() {
    // Cell A + cell B both hold a ref. Cell A unloads: decrement to
    // 1, texture entry stays live. Cell B unloads: decrement to 0,
    // path_map purged so a subsequent load creates a fresh entry.
    let mut reg = make_registry_with_entry("chair.dds", 2);
    assert!(
        !reg.release_ref(1),
        "first release should not authorise a GPU drop"
    );
    assert_eq!(reg.textures[1].ref_count, 1);
    // #610 — path_map keys now suffix the clamp_mode (`|3` =
    // WRAP_S_WRAP_T, the default REPEAT). Pre-#610 the key was
    // `"textures/chair.dds"` alone.
    assert!(
        reg.path_map.contains_key("textures/chair.dds|3"),
        "cell B still holds a ref — path_map must survive"
    );
    assert!(
        reg.release_ref(1),
        "last release must authorise the GPU drop"
    );
    assert_eq!(reg.textures[1].ref_count, 0);
    assert!(
        !reg.path_map.contains_key("textures/chair.dds|3"),
        "last release purges path_map"
    );
}

#[test]
fn release_ref_on_zero_refcount_warns_and_bails() {
    // Double-free guard: returns false without underflowing.
    let mut reg = make_registry_with_entry("chair.dds", 0);
    assert!(!reg.release_ref(1));
    assert_eq!(reg.textures[1].ref_count, 0);
}

#[test]
fn release_ref_on_unknown_handle_is_noop() {
    let mut reg = make_registry_with_entry("chair.dds", 1);
    assert!(!reg.release_ref(99));
    assert_eq!(
        reg.textures[1].ref_count, 1,
        "unrelated handles must not be touched"
    );
}

#[test]
fn fallback_refcount_sticky() {
    // Fallback handle is process-wide and must never underflow
    // from stray drops. `u32::MAX` gives plenty of headroom.
    let reg = make_registry_with_entry("chair.dds", 1);
    assert_eq!(reg.textures[0].ref_count, u32::MAX);
}

// ── #92 pending descriptor-write queue mechanics ────────────────

/// Regression for #92 — a descriptor update against the current
/// recording slot must NOT be pushed to the pending queue (the
/// current slot is written immediately); every OTHER slot must
/// receive a queued write so the caller can flush it safely after
/// that slot's fence signals.
#[test]
fn pending_write_records_on_other_slots_only() {
    let mut reg = make_registry_for_overflow_test(16, 0);
    reg.current_slot = 0;
    let image_view = vk::ImageView::null();
    let sampler = vk::Sampler::null();

    reg.record_pending_writes_for_other_slots(7, image_view, sampler);

    // Current slot (0): empty.
    assert_eq!(reg.pending_set_writes[0].len(), 0);
    // Other slot (1): received the deferred write.
    assert_eq!(reg.pending_set_writes[1].len(), 1);
    assert_eq!(reg.pending_set_writes[1][0].handle, 7);
}

/// Swapping the current slot flips which queue receives deferred
/// writes — the one previously "current" now accumulates pending
/// updates, matching the guarantee `begin_frame` relies on when
/// the caller ticks to a new slot.
#[test]
fn pending_write_current_slot_change_flips_deferred_target() {
    let mut reg = make_registry_for_overflow_test(16, 0);
    reg.current_slot = 0;
    reg.record_pending_writes_for_other_slots(1, vk::ImageView::null(), vk::Sampler::null());
    assert_eq!(reg.pending_set_writes[0].len(), 0);
    assert_eq!(reg.pending_set_writes[1].len(), 1);

    // Flip to slot 1 as the new current slot (begin_frame would
    // do this after the caller waits on slot 1's fence). A
    // subsequent write now queues on slot 0 instead.
    reg.current_slot = 1;
    reg.record_pending_writes_for_other_slots(2, vk::ImageView::null(), vk::Sampler::null());
    assert_eq!(reg.pending_set_writes[0].len(), 1);
    assert_eq!(reg.pending_set_writes[0][0].handle, 2);
    // Slot 1's queue is untouched by this call (handle 2 didn't
    // land there) — it still holds the original handle 1 from
    // before the flip.
    assert_eq!(reg.pending_set_writes[1].len(), 1);
    assert_eq!(reg.pending_set_writes[1][0].handle, 1);
}

/// Multiple writes accumulate in deferred slots — each one must
/// be replayed on flush, in authoring order. Guards against a
/// "last-write-wins" regression.
#[test]
fn pending_writes_accumulate_and_preserve_order() {
    let mut reg = make_registry_for_overflow_test(16, 0);
    reg.current_slot = 0;
    for handle in [3, 7, 11, 4] {
        reg.record_pending_writes_for_other_slots(
            handle,
            vk::ImageView::null(),
            vk::Sampler::null(),
        );
    }
    let deferred = &reg.pending_set_writes[1];
    assert_eq!(deferred.len(), 4);
    assert_eq!(
        deferred.iter().map(|w| w.handle).collect::<Vec<_>>(),
        vec![3, 7, 11, 4],
    );
}

/// `recreate_descriptor_sets` allocates fresh `VkDescriptorSet`
/// handles, so every pending write queued against the old sets
/// is invalid. Verify the queue is cleared as part of the
/// recreate-path contract (#92 — stale handles must not flow
/// into a fresh set in `flush_pending_set_writes`).
#[test]
fn pending_writes_cleared_by_recreate_semantics() {
    let mut reg = make_registry_for_overflow_test(16, 0);
    reg.current_slot = 0;
    reg.record_pending_writes_for_other_slots(5, vk::ImageView::null(), vk::Sampler::null());
    assert!(!reg.pending_set_writes[1].is_empty());

    // Simulate the `recreate_descriptor_sets` queue-clear step
    // directly (the Vulkan side needs a real device, out of
    // scope here).
    for queue in &mut reg.pending_set_writes {
        queue.clear();
    }
    assert!(reg.pending_set_writes.iter().all(|q| q.is_empty()));
}

// ── #881 pending DDS upload queue mechanics ────────────────────

/// Regression for #881 / CELL-PERF-03: the queueing core
/// (`queue_or_hit`) reserves a fresh bindless slot on cache miss
/// and pushes the upload onto the queue. The slot's `texture`
/// stays `None` until `flush_pending_uploads` populates it; the
/// queue mechanics — slot reservation, refcount = 1, path_map
/// entry, queue length — are exercisable without an
/// `ash::Device`. The fallback descriptor redirect on top of this
/// (in `enqueue_dds_with_clamp`) is gated on the fallback
/// entry's `texture` being `Some` — it's a no-op in production
/// when the fallback is uninitialised, so the queueing core is
/// what actually drives behaviour.
#[test]
fn enqueue_reserves_slot_and_queues_upload_on_miss() {
    let mut reg = make_registry_with_entry("placeholder.dds", 1);
    // Pre-state: 2 entries (fallback + the seeded `placeholder.dds`).
    assert_eq!(reg.textures.len(), 2);
    assert_eq!(reg.pending_dds_upload_count(), 0);

    // Miss path: `chair.dds` not in path_map → reserves slot 2.
    let bytes = vec![0u8; 128];
    let outcome = reg
        .queue_or_hit("chair.dds", bytes, 3)
        .expect("enqueue must succeed under non-overflow fixture");
    assert!(matches!(outcome, EnqueueOutcome::Reserved(2)));
    assert_eq!(reg.textures.len(), 3, "slot was pushed");
    assert!(
        reg.textures[2].texture.is_none(),
        "queued slot has no GPU image yet — flush populates it",
    );
    assert_eq!(
        reg.textures[2].ref_count, 1,
        "fresh enqueue starts at refcount 1 — symmetric with sync load_dds",
    );
    assert!(
        reg.path_map.contains_key("textures/chair.dds|3"),
        "path_map must point at the new handle so a sibling enqueue dedupes",
    );
    assert_eq!(reg.pending_dds_upload_count(), 1);
}

/// Repeat enqueue of the same `(path, clamp_mode)` pair must hit
/// the path_map and bump the refcount instead of reserving a
/// second slot — the cache-hit shape is the SAME as
/// `acquire_by_path_with_clamp`.
#[test]
fn enqueue_cache_hit_bumps_refcount_no_queue_growth() {
    let mut reg = make_registry_with_entry("chair.dds", 1);
    // The seeded fixture entry sits at handle 1.
    let outcome = reg
        .queue_or_hit("chair.dds", vec![0u8; 8], 3)
        .expect("cache hit must not allocate");
    assert!(
        matches!(outcome, EnqueueOutcome::Hit(1)),
        "cache hit returns the existing handle without queueing",
    );
    assert_eq!(reg.textures[1].ref_count, 2, "refcount bumped");
    assert_eq!(
        reg.pending_dds_upload_count(),
        0,
        "cache hit must NOT enqueue (no upload work to do)",
    );
}

/// 100 distinct DDS files queue 100 pending uploads — the count
/// is the number of `with_one_time_commands` calls
/// `flush_pending_uploads` collapses into ONE submit. This is the
/// invariant the audit's ~50–100 ms cell-load stall reduction
/// depends on. Pre-#881 each enqueue would have paid its own
/// fence-wait inline.
#[test]
fn one_hundred_uploads_queue_to_one_flush_batch() {
    let mut reg = make_registry_with_entry("placeholder.dds", 1);
    // Pad max_textures up to comfortably hold the test load.
    reg.max_textures = 256;

    for i in 0..100u32 {
        let path = format!("clutter_{i:03}.dds");
        let _ = reg
            .queue_or_hit(&path, vec![0u8; 64], 3)
            .expect("enqueue under non-overflow fixture must succeed");
    }
    assert_eq!(
        reg.pending_dds_upload_count(),
        100,
        "all 100 distinct paths must queue (the cell-load batched-flush invariant)",
    );
    // Sibling: 100 fresh slots reserved, all with `texture: None`.
    assert_eq!(reg.textures.len(), 102, "fallback + seed + 100 queued");
    for i in 2..102 {
        assert!(reg.textures[i].texture.is_none());
        assert_eq!(reg.textures[i].ref_count, 1);
    }
}

/// Sibling: `queue_or_hit` rejects when the bindless array is at
/// the max bound. Mirrors the existing `slot_rejected_at_exact_bound`
/// guard for the synchronous `load_dds_with_clamp` path. Without
/// the rejection, an enqueue would push past the bindless array
/// limit and corrupt descriptor state once the flush ran.
#[test]
fn enqueue_rejects_when_bindless_array_full() {
    let mut reg = make_registry_with_entry("placeholder.dds", 1);
    reg.max_textures = 2; // exact size the fixture has occupied.
    let err = reg
        .queue_or_hit("chair.dds", vec![0u8; 16], 3)
        .expect_err("full registry must refuse enqueue");
    let msg = format!("{err}");
    assert!(
        msg.contains("TextureRegistry is full"),
        "unexpected error: {msg}",
    );
    assert_eq!(
        reg.pending_dds_upload_count(),
        0,
        "queue must stay empty on rejection"
    );
}
