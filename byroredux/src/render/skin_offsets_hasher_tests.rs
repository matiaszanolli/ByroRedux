//! #2985 / TD9-2026-08-16-03 — pin the hasher on `skin_offsets`, the fourth
//! collection named by the hot-path hashing rule in
//! `.claude/commands/_audit-common.md`.
//!
//! #2923 shipped guards for the other three — every `SkinSlotPool` collection
//! and the `pose_dirty` set it hands the renderer
//! (`skin_slot_pool_maps_are_not_siphash`,
//! `pose_dirty_accessor_does_not_pin_siphash_across_the_crate_boundary`) and
//! `FrameInputs.pose_dirty` plus `record_skinned_blas_refit`'s parameter
//! (`pose_dirty_crosses_the_crate_boundary_without_siphash`). `skin_offsets`
//! had none, and it is the one that lives in the binary crate — which is where
//! both prior rounds of this defect class were reintroduced (#1368 → #2174 →
//! #2923, each sweep missing the next cluster). It is `FxHashMap` at every
//! site today, so this closes a coverage gap rather than a live regression.
//!
//! A source assertion, like its siblings: `HashMap<K, V, S>`'s hasher is not
//! observable from a value at runtime, and the declaration text is exactly
//! what regresses. Swapping a *single* site to `std::collections::HashMap`
//! fails to compile (the other three sites still pass `FxHashMap`), so what
//! this guard is actually for is the consistent whole-path swap that does
//! compile — which is exactly the shape #1368 and #2174 both were.
//! `the_extraction_accepts_both_spellings_and_rejects_a_std_swap` pins the
//! detection itself against a hand-written regressed line, so a filter that
//! silently stopped matching cannot read as a clean pass.
//!
//! Unlike its siblings this one matches per-declaration rather
//! than on a fixed fully-qualified spelling, because the four sites genuinely
//! differ — `main.rs` writes the type out in full (the `App` struct
//! declaration has no local imports), the `render/` sites use the house
//! import-then-bare style — and a guard that pinned one spelling would fire on
//! an unrelated import cleanup rather than on the regression it is for.

/// Every production site that names the type of `skin_offsets`. `app_frame.rs`
/// is deliberately absent: it passes `&mut self.skin_offsets` through without
/// spelling the type, so there is nothing there to pin.
fn declaration_sites() -> [(&'static str, &'static str); 4] {
    [
        ("byroredux/src/main.rs", include_str!("../main.rs")),
        ("byroredux/src/render/mod.rs", include_str!("mod.rs")),
        (
            "byroredux/src/render/skinned.rs",
            include_str!("skinned.rs"),
        ),
        (
            "byroredux/src/render/static_meshes.rs",
            include_str!("static_meshes.rs"),
        ),
    ]
}

/// Lines that name `skin_offsets` in binding position — its declaration, its
/// parameters, and its construction in the `App` struct literal. Test modules
/// build their own local `skin_offsets` with `let mut skin_offsets = ...`,
/// which is not binding-position syntax and so never matches.
fn skin_offsets_declarations(src: &str) -> Vec<&str> {
    src.lines()
        .map(str::trim)
        .filter(|line| line.starts_with("skin_offsets:"))
        .collect()
}

#[test]
fn skin_offsets_is_fx_hashed_at_every_site() {
    for (path, src) in declaration_sites() {
        let declarations = skin_offsets_declarations(src);
        assert!(
            !declarations.is_empty(),
            "{path} no longer declares `skin_offsets` — if the map moved, move \
             this guard with it; the hot-path hashing rule names it explicitly \
             (#2985)"
        );
        for declaration in declarations {
            assert!(
                declaration.contains("FxHashMap"),
                "{path} declares `{declaration}` — `skin_offsets` is probed \
                 once per static-mesh draw per frame \
                 (`static_meshes.rs`'s `skin_offsets.get(&entity)`), which is \
                 what makes SipHash-1-3 the wrong default here (#2985, \
                 following #1368 / #2174 / #2923)"
            );
        }
    }
}

/// The negative half: a std collection must not appear on any of these lines
/// even in a spelling that happens to keep the `FxHashMap` needle satisfied
/// elsewhere in the same file.
#[test]
fn no_site_reaches_for_a_std_hasher() {
    for (path, src) in declaration_sites() {
        for declaration in skin_offsets_declarations(src) {
            assert!(
                !declaration.contains("std::collections::HashMap"),
                "{path} reverted `skin_offsets` to std's SipHash-1-3: \
                 `{declaration}` (#2985)"
            );
        }
    }
}

/// The guard has to be able to see the regression it exists for. Both
/// assertions above key on the same extraction, so pin that extraction against
/// hand-written samples of the shapes it must accept and reject — otherwise a
/// filter that quietly stopped matching anything would read as a clean pass.
#[test]
fn the_extraction_accepts_both_spellings_and_rejects_a_std_swap() {
    let bare = "    skin_offsets: &FxHashMap<EntityId, u32>,";
    let qualified = "    skin_offsets: rustc_hash::FxHashMap<byroredux_core::ecs::EntityId, u32>,";
    let swapped = "    skin_offsets: &std::collections::HashMap<EntityId, u32>,";
    let local = "    let mut skin_offsets = FxHashMap::default();";

    for accepted in [bare, qualified] {
        let found = skin_offsets_declarations(accepted);
        assert_eq!(found.len(), 1, "missed a declaration spelling: {accepted}");
        assert!(found[0].contains("FxHashMap"));
    }
    let found = skin_offsets_declarations(swapped);
    assert_eq!(
        found.len(),
        1,
        "a std swap must still be extracted, then fail"
    );
    assert!(!found[0].contains("FxHashMap"));
    assert!(found[0].contains("std::collections::HashMap"));

    assert!(
        skin_offsets_declarations(local).is_empty(),
        "a test module's local binding is not a declaration site"
    );
}
