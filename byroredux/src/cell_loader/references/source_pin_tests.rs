//! Source-position pins for paths that need a Vulkan device + game data.
//!
//! Extracted from `references/mod.rs`'s inline `mod tests`
//! (#2409 / TD1-006). Contents unchanged.

// Test-only symbols not referenced by production code in this module
// (they'd warn as unused at file scope). #1877 split.

/// #1798 / D7-NEW-01 + EXAL sub-REFR regression. A live call-through
/// needs Vulkan + archive fixtures, so pin the source-level wiring:
/// both game families create the resumable continuation and its active
/// work time (which excludes parked inter-frame time) still reaches the
/// end-of-cell NPC telemetry.
#[test]
fn npc_spawn_jobs_are_resumable_and_wall_clock_timed() {
    // Whole files: since the #2409 split moved these pins into this sibling,
    // the scanned sources are production-only and the searched literals can
    // no longer self-match the scanning test's own source. Both halves of
    // the loader are concatenated — the job/continuation wiring stayed in
    // `mod.rs`, the end-of-cell summary log moved to `complete.rs`.
    let src = format!("{}{}", include_str!("mod.rs"), include_str!("complete.rs"));
    let src = src.as_str();

    assert!(src.contains("NpcSpawnJob::runtime("));
    assert!(src.contains("NpcSpawnJob::prebaked("));
    assert!(src.contains("NpcSpawnProgress::Pending"));
    assert!(src.contains("active_npc: Option<NpcSpawnJob>"));
    assert!(
        src.contains("npc_spawn_wall += result.work_wall"),
        "active NPC work time must accumulate into npc_spawn_wall"
    );
    assert!(
        src.contains("{:.1}ms wall in spawn calls"),
        "the accumulated NPC-spawn wall time must be surfaced in the \
         end-of-cell summary log, or the cost stays invisible (the exact \
         gap #1798 reports)"
    );
}

/// Regression for #2277 (PERF-D7-03): a budget yield partway through a
/// SCOL/PKIN's `synth_refs` must resume from `job.current_ref_synth`
/// instead of re-walking `expand_pkin_placements`/`expand_scol_placements`
/// (and rebuilding the shared texture overlay) from scratch. Exercising
/// this end-to-end needs a real `VulkanContext`, out of unit-test scope
/// here (same constraint as `npc_spawn_jobs_are_resumable_and_wall_clock_timed`
/// above), so this pins the structural invariant via source inspection.
/// SCR-D7-NEW11-01 (#2662) — actor jobs build their own placement root
/// instead of routing through `spawn_synth_child`, so they are the one
/// spawn path that can silently skip the VMAD attach. It did, for
/// 805 `NPC_` + 822 `ACHR` VMAD-bearing records on `Skyrim.esm` alone,
/// which also made the `npcs` arm of `base_record_script_instance`
/// unreachable from the live attach path.
///
/// Source-scan rather than a live spawn: the actor path wants a Vulkan
/// device and on-disk game data, out of `cargo test` scope. Mirrors
/// `scol_expansion_is_cached_across_a_budget_yield` below.
#[test]
fn actor_spawn_branch_attaches_vmad_scripts() {
    // Production-only since the #2409 split — see the note above.
    let src = include_str!("mod.rs");

    // The actor branch specifically: its `stamp_quest_reference` must be
    // followed by an attach before the branch's `continue`.
    let actor_branch = src
        .find("NpcSpawnProgress::Complete(result)")
        .expect("the actor spawn-complete branch must still exist");
    // Bound the window to the actor arm itself — it ends at the
    // `continue` that hands off to the static `spawn_synth_child` path.
    // Without this bound the search would happily match a *later*
    // branch's attach and pass while the actor arm has none.
    let branch_len = src[actor_branch..]
        .find("let refr_script_instance = refr_script_instance_for_synth_child(")
        .expect("the static-path handoff must follow the actor branch");
    let branch_tail = &src[actor_branch..actor_branch + branch_len];
    let stamp = branch_tail
        .find("stamp_quest_reference(")
        .expect("the actor branch must stamp the canonical ACHR identity");
    let attach = branch_tail.find("attach_quest_reference_script(").expect(
        "the actor branch must attach VMAD scripts — without it every scripted \
         NPC_/ACHR loads with no canonical behavior (#2662)",
    );
    assert!(
        stamp < attach,
        "attach must follow the identity stamp in the actor branch"
    );
}

#[test]
fn scol_expansion_is_cached_across_a_budget_yield() {
    // Production-only since the #2409 split — see the note above.
    let src = include_str!("mod.rs");

    assert!(
        src.contains("current_ref_synth: Option<SynthChildPlan>"),
        "ReferenceLoadJob must cache the expanded synth_refs + overlay across yields"
    );
    assert_eq!(
        src.matches("job.current_ref_synth = Some((synth_refs, refr_overlay));")
            .count(),
        2,
        "both yield points (a plain budget yield and NpcSpawnProgress::Pending) \
         must stash the in-progress expansion, or resuming after either one \
         recomputes it"
    );
    assert!(
        src.contains("match job.current_ref_synth.take() {"),
        "the cache must be consumed at REFR entry instead of unconditionally \
         calling expand_pkin_placements/expand_scol_placements"
    );
    assert!(
        src.contains("job.current_ref_synth = None;"),
        "the cache must be cleared once a REFR's synth list is fully \
         processed, or a later REFR could read stale data"
    );
}
