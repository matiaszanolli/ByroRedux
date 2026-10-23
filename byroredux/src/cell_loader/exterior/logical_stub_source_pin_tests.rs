//! Regression for #2664 (SCR-D7-NEW11-03): the persistent-cell logical-actor
//! stub must route through the one shared spawner.
//!
//! Driving `PersistentCellApplyJob::apply` needs a `VulkanContext` and on-disk
//! game data, out of `cargo test` scope — so this pins the wiring the way the
//! reference loader's own untestable paths are pinned (see
//! `references/source_pin_tests.rs`). What it guards is the *duplication*: the
//! loop used to carry a verbatim copy of `stamp_quest_reference`'s body, which
//! both drifts as `SceneAliasCandidate` grows (#2541's invariant) and — the
//! live consequence — inserted no transform, excluding the entity from every
//! distance-ranked alias fill. The behavioural half is pinned in
//! `byroredux-scripting`'s
//! `quest_alias_closest_fill_needs_a_transform_on_its_only_candidate`.

/// #5650 — this file's own source with every `#[cfg(test)] mod` block
/// stripped. This file interleaves seven test modules with production
/// code (the stub loop lives between two of them), so the first-cut
/// `production_text` would drop it; `strip_test_modules` keeps it. A
/// whole-file scan is half-vacuous: the positive `contains` below used
/// to be satisfiable by the assertion's own literal even with the
/// production call deleted.
fn production_src() -> String {
    super::production_source()
}

#[test]
fn logical_actor_stubs_spawn_through_the_shared_transform_bearing_helper() {
    let src = production_src();
    // #4112 — the spawner this pin named grew a paired variant that also
    // attaches the reference's scripts, and the stub loop moved onto it.
    // The plain identity-only spawner must NOT come back here: that is
    // precisely the call this issue was about. (Naming it in prose rather
    // than in backticked call syntax — the count below would see it.)
    assert!(
        src.contains("references::spawn_logical_quest_reference_with_scripts("),
        "the persistent-cell logical stub must call the shared spawner that \
         also attaches scripts (#4112), not open-code the identity components"
    );
    // The identity-only spawner's call syntax must be absent from
    // production text entirely — a call site here drops the base
    // record's SCRI/VMAD and the ACHR's own VMAD (#4112).
    assert_eq!(
        src.matches("spawn_logical_quest_reference(").count(),
        0,
        "the identity-only spawner must not appear on this path — it drops \
         the base record's SCRI/VMAD and the ACHR's own VMAD (#4112)"
    );
    assert!(
        src.contains("transition::position_zup_to_yup(placed.position)"),
        "the stub must be given the REFR's own placement, converted the same \
         way the reference loader converts it"
    );
    // The hand-rolled block this replaced constructed the candidate
    // inline; `stamp_quest_reference` (references.rs) must stay the only
    // construction site, so none may appear in this file's production.
    assert_eq!(
        src.matches("SceneAliasCandidate {").count(),
        0,
        "the alias-candidate component must be built only by \
         `stamp_quest_reference`; a construction site here is the \
         duplication #2664 removed"
    );
}
