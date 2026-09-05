//! Acceleration-structure tests — static (mesh-keyed) BLAS build, eviction and deferred destroy.
//!
//! Split out of the 2 329-LOC monolithic `tests.rs` under #2977. Every
//! test here is a pure unit test (no live Vulkan context); the split
//! mirrors the production submodule names where tests exist for them.

// #2481 / AS-D1-NEW-02 — BLAS registration must release any BLAS already
// occupying the target slot/key before overwriting it, or the previous
// `vk::AccelerationStructureKHR` leaks (no `Drop` impl) and the byte
// budget counters drift upward. Building a real BLAS needs a live Vulkan
// device, so — matching this crate's convention for logic that can only
// be exercised end-to-end with a GPU (e.g. `context/mod.rs`'s
// `rigid_history_hasher_tests`, `context/skinned_blas_refit.rs`'s
// `skin_built_this_frame_skip_tests`) — this pins the fix at the source
// level: the release call must appear, and must appear strictly before
// the registration it guards, at both surviving sites.
//
// #2914 deleted the third — the never-called single-shot
// `blas_static::build_blas` — so `build_blas_releases_before_overwriting`
// went with it. `blas_static.rs` now holds exactly ONE static
// registration site (`build_blas_batched`'s Phase 7); the test below
// asserts that count directly, so reviving a second static build path
// without its `drop_blas` guard fails here rather than silently leaking a
// `vk::AccelerationStructureKHR`.
#[cfg(test)]
mod blas_registration_releases_occupied_slot_tests {
    const BLAS_STATIC_RS: &str = include_str!("../blas_static.rs");
    const BLAS_SKINNED_RS: &str = include_str!("../blas_skinned.rs");

    #[test]
    fn build_blas_batched_releases_before_overwriting() {
        // #2914 — `build_blas_batched`'s Phase 7 is now the ONLY static
        // registration site. Pinning the count is what keeps this test
        // honest: a revived second static build path that forgot its
        // guard would otherwise sail past a bare "find the first one".
        assert_eq!(
            BLAS_STATIC_RS
                .matches("self.blas_entries[handle] = Some(BlasEntry {")
                .count(),
            1,
            "blas_static.rs gained a second static registration site — give it \
             a `drop_blas` guard and extend this test, or the entry it \
             overwrites leaks its vk::AccelerationStructureKHR (#2481/#2914)"
        );
        let guard_pos = BLAS_STATIC_RS.find("self.drop_blas(mesh_handle);").expect(
            "build_blas_batched's Phase 7 registration must release any \
             occupied handle before overwriting it (#2481)",
        );
        let assign_pos = BLAS_STATIC_RS
            .find("self.blas_entries[handle] = Some(BlasEntry {")
            .expect("build_blas_batched's registration assignment must still exist");
        assert!(
            guard_pos < assign_pos,
            "the release must run BEFORE the overwrite, or the entry being \
             replaced is still live when it's dropped as plain memory"
        );
    }

    #[test]
    fn skinned_blas_batch_releases_before_overwriting() {
        let guard_pos = BLAS_SKINNED_RS
            .find("self.drop_skinned_blas(p.entity_id);")
            .expect(
                "build_skinned_blas_batched_on_cmd's Phase 4 registration must \
                 release any existing entity entry before overwriting it (#2481)",
            );
        let assign_pos = BLAS_SKINNED_RS
            .find("self.skinned_blas.insert(")
            .expect("the skinned_blas registration insert must still exist");
        assert!(
            guard_pos < assign_pos,
            "the release must run BEFORE the insert, or the entry being \
             replaced is still live when it's dropped as plain memory"
        );
    }
}

// ── BLAS compaction rollback + peak accounting ───────────────────────
//
// Both invariants live on `build_blas_batched`'s compaction phase, whose
// only trigger is an allocator OOM part-way through a batch — a live
// device plus a genuinely exhausted pool. Same source-position pinning
// approach the file already uses for `blas_registration_releases_
// occupied_slot_tests` and `tlas_commit_ordering_tests`.
#[cfg(test)]
mod blas_compaction_rollback_tests {
    const BLAS_STATIC_RS: &str = include_str!("../blas_static.rs");

    /// #2926 / PERF-D3-02 — `alloc_compact`'s two early exits
    /// (`create_device_local_uninit`'s `?` and the
    /// `create_acceleration_structure` `bail!`) must not strand the
    /// compaction destinations earlier iterations already allocated. A
    /// `vk::AccelerationStructureKHR` has no `Drop` impl, so a
    /// closure-owned `compact_accels` leaked one handle per already-
    /// compacted mesh — on the one path (OOM) where leaking makes the
    /// next attempt fail sooner. The vec must therefore be owned by the
    /// caller and walked by the rollback arm.
    #[test]
    fn alloc_compact_failure_destroys_already_compacted_structures() {
        let decl = BLAS_STATIC_RS
            .find(
                "let mut compact_accels: Vec<CompactedBlas> = Vec::with_capacity(prepared.len());",
            )
            .expect(
                "`compact_accels` must be declared OUTSIDE `alloc_compact` so the \
                 rollback arm can see what the closure allocated before it failed (#2926)",
            );
        let closure = BLAS_STATIC_RS
            .find("let mut alloc_compact = |compact_accels: &mut Vec<CompactedBlas>|")
            .expect(
                "`alloc_compact` must take `compact_accels` by `&mut` rather than \
                 owning it (#2926)",
            );
        assert!(
            decl < closure,
            "the caller-owned vec must be declared before the closure that fills it"
        );

        let err_arm = BLAS_STATIC_RS[closure..]
            .find("match alloc_compact(&mut compact_accels)")
            .map(|p| p + closure)
            .expect("the call site must pass the caller-owned vec in");
        // The rollback arm for the compaction-allocation failure runs
        // before the `prepared` rollback that #316 already had.
        let compact_cleanup = BLAS_STATIC_RS[err_arm..]
            .find("for (_, accel, mut buf, _, _, _) in compact_accels {")
            .map(|p| p + err_arm)
            .expect(
                "the `alloc_compact` failure arm must destroy every compaction \
                 destination already allocated — each is a raw \
                 vk::AccelerationStructureKHR with no Drop impl (#2926)",
            );
        let prepared_cleanup = BLAS_STATIC_RS[err_arm..]
            .find("for mut p in prepared {")
            .map(|p| p + err_arm)
            .expect("the #316 `prepared` rollback must still run on this arm");
        assert!(
            compact_cleanup < prepared_cleanup,
            "both rollbacks must run on the compaction-failure arm"
        );
    }

    /// #2927 / PERF-D3-03 — the compaction phase is where static-BLAS
    /// residency peaks (originals + destinations both live until Phase 7),
    /// and the Phase-1 `pending_bytes` ledger never sees it. The budget
    /// must be tested against `total_before + total_after` before the
    /// first destination is allocated — the readback above it has already
    /// made the exact peak knowable.
    #[test]
    fn compaction_phase_checks_the_budget_against_the_real_peak() {
        let totals = BLAS_STATIC_RS
            .find("let total_after: u64 = compacted_sizes.iter().sum();")
            .expect("alloc_compact must still sum the compacted sizes");
        let evict = BLAS_STATIC_RS[totals..]
            .find("self.evict_unused_blas(")
            .map(|p| p + totals)
            .expect(
                "the compaction phase must run a budget check — it is the phase \
                 that pushes static-BLAS residency to its batch maximum, and \
                 pre-#2927 it had no eviction call at all",
            );
        let alloc_loop = BLAS_STATIC_RS[totals..]
            .find("for (i, p) in prepared.iter().enumerate() {")
            .map(|p| p + totals)
            .expect("the destination-allocation loop must still exist");
        assert!(
            evict < alloc_loop,
            "the check must run BEFORE the first compaction destination is \
             allocated, or it is measuring a peak it can no longer avoid (#2927)"
        );
        assert!(
            BLAS_STATIC_RS[evict..alloc_loop].contains("total_before.saturating_add(total_after)"),
            "the pending figure must be originals + destinations — both sets are \
             simultaneously resident until Phase 7 destroys the originals (#2927)"
        );
    }
}

/// #3840 — `pending_destroy_static_bytes` only tells the truth if every site
/// that moves static bytes onto the deferred queue credits it, and every site
/// that frees them releases it. A live `AccelerationManager` needs a Vulkan
/// device, so pin the balance structurally instead.
#[cfg(test)]
mod pending_destroy_static_bytes_stays_balanced_tests {
    const BLAS_STATIC_RS: &str = include_str!("../blas_static.rs");
    const BLAS_SKINNED_RS: &str = include_str!("../blas_skinned.rs");

    #[test]
    fn every_static_deferred_push_credits_the_resident_counter() {
        // Both static push sites (`drop_blas`, `evict_unused_blas`) deduct
        // `static_blas_bytes` and must hand the bytes to the resident counter
        // in the same breath, or admission checks under-count real residency.
        let pushes = BLAS_STATIC_RS
            .matches("self.pending_destroy_blas.push(entry, DEFAULT_COUNTDOWN);")
            .count();
        assert_eq!(
            pushes, 2,
            "blas_static.rs's static deferred-destroy push count changed — a new \
             site must also credit `pending_destroy_static_bytes` (#3840)"
        );
        // Collapse whitespace first: the two sites sit at different nesting
        // depths, so rustfmt wraps them differently.
        let flat = BLAS_STATIC_RS
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            flat.matches(
                "self.pending_destroy_static_bytes = self .pending_destroy_static_bytes \
                 .saturating_add(entry.size_bytes);"
            )
            .count(),
            pushes,
            "every static push onto `pending_destroy_blas` must credit \
             `pending_destroy_static_bytes` — an uncredited one lets a batch spend \
             headroom whose memory the GPU still holds (#3840)"
        );
    }

    #[test]
    fn both_destroy_paths_release_the_resident_counter() {
        // tick = countdown expiry, drain = shutdown sweep. A path that frees
        // the memory without releasing the counter strands bytes forever and
        // permanently depresses the eviction trigger.
        assert!(
            BLAS_STATIC_RS.contains("if entry.counted_in_static_bytes {")
                && BLAS_STATIC_RS.contains(".saturating_sub(released_static);"),
            "tick_deferred_destroy must release the static bytes it actually frees (#3840)"
        );
        assert!(
            BLAS_STATIC_RS.contains("self.pending_destroy_static_bytes = 0;"),
            "drain_pending_destroys empties the queue, so it must zero \
             `pending_destroy_static_bytes` (#3840)"
        );
    }

    #[test]
    fn skinned_entries_are_excluded_from_the_static_counter() {
        // Skinned BLAS never reach `static_blas_bytes`, so counting them on the
        // way out would make the static budget respond to skinned churn — the
        // exact thrash #920 split the counters to prevent.
        assert!(
            BLAS_SKINNED_RS.contains("counted_in_static_bytes: false,"),
            "skinned BlasEntry construction must opt out of the static byte \
             accounting (#920 / #3840)"
        );
        assert!(
            BLAS_STATIC_RS.contains("counted_in_static_bytes: true,"),
            "static BlasEntry construction must opt in (#3840)"
        );
    }

    #[test]
    fn mid_batch_trigger_uses_resident_bytes_but_the_evict_loop_does_not() {
        // The asymmetry is deliberate and load-bearing. The trigger asks "is
        // the GPU actually full?" (resident). The eviction loop asks "have I
        // scheduled enough?" (paper) — each iteration moves the same bytes from
        // `static_blas_bytes` into `pending_destroy_static_bytes`, so a
        // resident-based break can never be satisfied and would evict every
        // idle candidate on the first pressure event, guaranteeing a rebuild
        // storm on the next frame.
        assert!(
            BLAS_STATIC_RS.contains("self.resident_static_blas_bytes(),"),
            "the mid-batch eviction trigger must use resident bytes (#3840)"
        );
        let loop_break = BLAS_STATIC_RS
            .find("if !blas_over_budget(\n                self.static_blas_bytes,")
            .expect(
                "evict_unused_blas's loop break must keep using the paper figure \
                 `static_blas_bytes` — see the comment at its push site (#3840)",
            );
        assert!(
            BLAS_STATIC_RS[loop_break..].contains("resident figure belongs"),
            "the paper-vs-resident asymmetry must stay documented at the push site, \
             or a future reader will 'fix' the break into a thrash (#3840)"
        );
    }
}
