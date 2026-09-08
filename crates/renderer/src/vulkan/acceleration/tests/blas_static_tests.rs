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

    /// #4001 — every deferred-destroy push in the acceleration module must
    /// spell the countdown as `DEFAULT_COUNTDOWN`.
    ///
    /// `DeferredDestroyQueue::push`'s doc says production callers pass it, and
    /// the constant exists so a `MAX_FRAMES_IN_FLIGHT` bump propagates through
    /// one place. `drop_skinned_blas` passed `MAX_FRAMES_IN_FLIGHT as u32`
    /// directly — identical by construction, and therefore invisible to every
    /// behavioural test, which is exactly why it needs a structural one.
    #[test]
    fn no_deferred_destroy_push_reaches_around_the_shared_countdown() {
        for (label, src) in [
            ("blas_static.rs", BLAS_STATIC_RS),
            ("blas_skinned.rs", BLAS_SKINNED_RS),
        ] {
            let flat = src.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(
                !flat.contains(".push(entry, MAX_FRAMES_IN_FLIGHT"),
                "{label} pushes onto a deferred-destroy queue with a raw \
                 MAX_FRAMES_IN_FLIGHT cast — use DEFAULT_COUNTDOWN, the single \
                 place that bump is meant to propagate through (#4001)"
            );
        }
        // And the skinned site still pushes at all — an assertion that only
        // forbids the wrong spelling would pass on a deleted push.
        assert!(
            BLAS_SKINNED_RS
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .contains("self.pending_destroy_blas.push(entry, DEFAULT_COUNTDOWN);"),
            "drop_skinned_blas must still defer its BLAS destroy — an earlier \
             frame's command buffer may still reference it (#1782)"
        );
    }

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

    /// #3979 (REN-2026-09-06-D1-01) — the resident counter must have a real
    /// ADMISSION consumer, not just the (inert) mid-batch eviction trigger.
    /// Its docstring has asserted since #3840 that it is "the figure admission
    /// checks must use … letting a batch allocate against headroom that does
    /// not exist yet", while no admission check existed anywhere: the Phase-1
    /// loop called `create_device_local_uninit` unconditionally on every
    /// iteration and the only budget interaction was an eviction request.
    #[test]
    fn the_phase_one_loop_declines_rather_than_allocating_past_residency() {
        assert!(
            BLAS_STATIC_RS.contains("if blas_admission_exhausted("),
            "build_blas_batched's Phase-1 loop must consult the admission gate              before allocating another result buffer — without it the resident              counter cannot change any outcome (#3979)"
        );
        let gate = BLAS_STATIC_RS
            .find("if blas_admission_exhausted(")
            .expect("the admission gate call must still exist");
        let alloc = BLAS_STATIC_RS
            .find("let mut result_buffer = GpuBuffer::create_device_local_uninit(")
            .expect("the Phase-1 result-buffer allocation must still exist");
        assert!(
            gate < alloc,
            "the admission gate must sit AHEAD of the Phase-1 result-buffer              allocation — a check after the allocation admits the very byte it              was meant to decline (#3979)"
        );
        assert!(
            BLAS_STATIC_RS[gate..alloc].contains("self.resident_static_blas_bytes(),"),
            "the admission gate must be fed the RESIDENT figure, not the paper              `static_blas_bytes`: eviction credits the paper figure the instant it              queues an entry, but the allocator free is DEFAULT_COUNTDOWN frames              out inside draw_frame, which never runs during a batch (#3979 / #3840)"
        );
        assert!(
            BLAS_STATIC_RS.contains("eviction_can_still_reclaim = self.static_blas_bytes < paper_before;"),
            "the gate's 'eviction is out of candidates' input must be derived from              the PAPER figure moving across an eviction pass — the resident figure              is unchanged by eviction inside a batch, so a resident-based test would              never report progress (#3979)"
        );
        assert!(
            BLAS_STATIC_RS.contains("if prepared.is_empty() {"),
            "a batch the gate declines outright must return before Phase 2 — a              zero-length batch creates a query pool with queryCount == 0, which              VUID-VkQueryPoolCreateInfo-queryCount-02763 forbids (#3979)"
        );
        assert!(
            !BLAS_STATIC_RS.contains("self.pending_destroy_blas.tick"),
            "the admission shortfall must NOT be 'fixed' by ticking the deferred-              destroy queue at a batch boundary — that is the #1449 / #1782              use-after-free class; the countdown stands in for a fence wait              build_blas_batched does not have (#3979)"
        );
    }
}

/// #3999 — every BLAS-residency accessor must have a live consumer.
///
/// Four `pub` accessors had none, and three named one that did not exist: a
/// texture-stats console command never registered, and two unit tests never
/// written. The newest was added by #3840 *the day before* the audit,
/// specifically so an operator could read how much VRAM the deferred-destroy
/// queue holds — and then not connected. The effect was that the exact
/// overshoot `REN-2026-09-06-D1-01` describes was not diagnosable from a
/// running engine even after the number had been computed.
///
/// A docstring cannot be compiled, so the claim needs a gate. This asserts
/// the consumer end: `fill_rt_integrity_stats` must call each accessor by
/// name. Scanning the caller rather than counting greps means a future
/// refactor that reads the private fields directly — re-orphaning the public
/// surface while keeping the telemetry working — still fails here.
#[cfg(test)]
mod blas_residency_telemetry_tests {
    const CONTEXT_MOD_RS: &str = include_str!("../../context/mod.rs");

    #[test]
    fn every_blas_residency_accessor_reaches_the_rt_integrity_snapshot() {
        let fill = CONTEXT_MOD_RS
            .split("pub fn fill_rt_integrity_stats(")
            .nth(1)
            .expect(
                "fill_rt_integrity_stats must still exist — it is the one path \
                 BLAS residency takes to the console (#3999)",
            );
        // Stop at the next documented item so a call elsewhere in the file
        // cannot satisfy this.
        let body = fill
            .split("\n    /// ")
            .next()
            .expect("the fn must still be followed by another documented item");

        for accessor in [
            "accel.total_blas_bytes()",
            "accel.static_blas_bytes()",
            "accel.pending_destroy_static_bytes()",
            "accel.pending_destroy_blas_count()",
            "accel.pending_destroy_scratch_count()",
        ] {
            assert!(
                body.contains(accessor),
                "`{accessor}` is no longer read by fill_rt_integrity_stats — \
                 BLAS device residency and the deferred-destroy backlog become \
                 unreadable from a running engine again, and the accessor \
                 becomes a `pub` item whose docstring names a consumer that \
                 does not exist (#3999)"
            );
        }
    }

    /// The docstrings themselves, since naming a command that was never
    /// registered is what made the original four misleading rather than
    /// merely unused. The sweep found a fifth mention the report did not
    /// list, on the `total_blas_bytes` *field* in `acceleration/mod.rs`.
    #[test]
    fn no_blas_accessor_docstring_names_a_command_that_does_not_exist() {
        // Composed at runtime so this test's own text is not what it matches.
        let phantom = format!("{}{}", "tex", ".stats");
        for (label, src) in [
            ("blas_static.rs", include_str!("../blas_static.rs")),
            ("memory.rs", include_str!("../memory.rs")),
            ("acceleration/mod.rs", include_str!("../mod.rs")),
        ] {
            assert!(
                !src.contains(phantom.as_str()),
                "{label} names a console command that is not in the registry \
                 — the docstring asserts an observability path an operator \
                 cannot take (#3999)"
            );
        }
    }
}

/// #4000 / PERF-D6-01 residual — the acceleration module carries its own
/// hot-path-hashing pin.
///
/// `_audit-common.md`'s rule is that the per-frame render/skinning path is
/// `FxHashMap`/`FxHashSet` end-to-end **across the crate boundary**.
/// `context/mod.rs` holds that line with eight pinned fields — but its guards
/// read that file's own source text, so `skinned_blas`, the one member of
/// `PERF-D6-01`'s seven-field table living outside it, survived #3061's sweep
/// untouched and unnoticed. A guard that can only see one file is why the
/// finding recurred; this is the acceleration module's own.
#[cfg(test)]
mod hot_path_hashing_tests {
    const ACCELERATION_MOD_RS: &str = include_str!("../mod.rs");

    /// `skinned_blas` is probed once per skinned draw command in
    /// `build_tlas_instances` and three more times per dirty entity on the
    /// refit path, over a `u32` keyspace.
    #[test]
    fn skinned_blas_stays_fx_hashed() {
        assert!(
            ACCELERATION_MOD_RS
                .contains("skinned_blas: rustc_hash::FxHashMap<EntityId, BlasEntry>"),
            "skinned_blas must stay `FxHashMap` (#4000) — it is on the \
             per-frame skinned path, which the guard tests in \
             `context/mod.rs` claim is Fx-hashed end-to-end"
        );
        assert!(
            !ACCELERATION_MOD_RS.contains("skinned_blas: std::collections::HashMap"),
            "skinned_blas reverted to std's SipHash-1-3 (#4000)"
        );
    }
}
