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
