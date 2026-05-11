//! Tests for `pkin_expansion_tests` extracted from ../cell_loader.rs (refactor stage A).
//!
//! Same qualified path preserved (`pkin_expansion_tests::FOO`).

//! Regression tests for #589 (FO4-DIM4-03) — PKIN (Pack-In) REFR
//! expansion. Every `CNAM` content ref spawns at the outer REFR's
//! transform. Pre-fix the 872 vanilla Fallout4.esm PKIN records
//! were routed through the MODL-only parser and their CNAM lists
//! were silently dropped.
use super::*;
use byroredux_plugin::esm::cell::EsmCellIndex;
use byroredux_plugin::esm::records::PkinRecord;

fn mk_pkin(form_id: u32, editor_id: &str, contents: Vec<u32>) -> PkinRecord {
    PkinRecord {
        form_id,
        editor_id: editor_id.to_string(),
        full_name: String::new(),
        contents,
        vnam_form_id: 0,
        flags: 0,
        // #815 — FLTR (workshop build-mode filter) defaults to
        // empty for the placement-expansion tests; the cell loader
        // doesn't consult `filter` today (pre-render), so the test
        // shape is unchanged.
        filter: Vec::new(),
    }
}

/// Baseline: a base form that isn't a PKIN returns `None` so the
/// caller falls through to the SCOL / default chain.
#[test]
fn expand_non_pkin_returns_none() {
    let index = EsmCellIndex::default();
    let result = expand_pkin_placements(
        0x0010_ABCD,
        Vec3::new(100.0, 50.0, -25.0),
        Quat::IDENTITY,
        2.0,
        &index,
    );
    assert!(result.is_none());
}

/// Vanilla shape: a PKIN with a single CNAM fans out to one
/// synthetic placement at the outer transform. This is how FO4
/// workbench-loot bundles render end-to-end.
#[test]
fn expand_pkin_single_cnam_fans_out_to_one_synth() {
    let mut index = EsmCellIndex::default();
    let pkin_id = 0x0055_0001;
    index.packins.insert(
        pkin_id,
        mk_pkin(pkin_id, "WorkbenchLoot", vec![0x0020_0001]),
    );

    let outer_pos = Vec3::new(500.0, 100.0, 250.0);
    let outer_rot = Quat::IDENTITY;
    let outer_scale = 1.5;
    let synths = expand_pkin_placements(pkin_id, outer_pos, outer_rot, outer_scale, &index)
        .expect("PKIN with a CNAM must fan out");
    assert_eq!(synths.len(), 1);
    assert_eq!(synths[0].0, 0x0020_0001);
    assert_eq!(synths[0].1, outer_pos);
    assert_eq!(synths[0].2, outer_rot);
    // Outer scale propagates verbatim — PKIN has no per-child scale.
    assert_eq!(synths[0].3, outer_scale);
}

/// Multi-CNAM bundle: each content ref becomes a synth. All synths
/// share the outer transform; authoring order is preserved so
/// downstream consumers iterate in the right sequence.
#[test]
fn expand_pkin_multiple_cnam_preserves_order_at_outer_transform() {
    let mut index = EsmCellIndex::default();
    let pkin_id = 0x0055_0002;
    index.packins.insert(
        pkin_id,
        mk_pkin(
            pkin_id,
            "MultiPack",
            vec![0x0020_0001, 0x0020_0002, 0x0020_0003],
        ),
    );

    let outer_pos = Vec3::new(10.0, 20.0, 30.0);
    let outer_rot = Quat::from_rotation_y(0.5);
    let outer_scale = 1.0;
    let synths =
        expand_pkin_placements(pkin_id, outer_pos, outer_rot, outer_scale, &index).unwrap();
    assert_eq!(synths.len(), 3);
    assert_eq!(synths[0].0, 0x0020_0001);
    assert_eq!(synths[1].0, 0x0020_0002);
    assert_eq!(synths[2].0, 0x0020_0003);
    // Every synth shares the outer transform exactly.
    for s in &synths {
        assert_eq!(s.1, outer_pos);
        assert_eq!(s.2, outer_rot);
        assert_eq!(s.3, outer_scale);
    }
}

/// A PKIN with an empty `contents` list (malformed / author-trimmed)
/// returns `None` so the caller falls through rather than spawning
/// zero synthetic children at the outer transform. Prevents the
/// outer REFR from being silently dropped — the single-entry
/// default path still runs its own stat lookup.
#[test]
fn expand_pkin_with_empty_contents_returns_none() {
    let mut index = EsmCellIndex::default();
    let pkin_id = 0x0055_0003;
    index
        .packins
        .insert(pkin_id, mk_pkin(pkin_id, "EmptyBundle", Vec::new()));
    let result = expand_pkin_placements(pkin_id, Vec3::ZERO, Quat::IDENTITY, 1.0, &index);
    assert!(result.is_none());
}

/// #635 / FNV-D3-06 — PKIN-of-PKIN nesting fans out one extra level.
/// Pre-fix the inner PKIN's content was silently dropped when the
/// outer caller looked up the inner PKIN's form ID in `index.statics`
/// and missed (PKINs don't live in the statics table). Verify the
/// outer expansion now flattens to the leaf STAT form IDs.
#[test]
fn expand_pkin_recurses_into_nested_pkin() {
    let mut index = EsmCellIndex::default();
    let outer_pkin_id = 0x0066_0001;
    let inner_pkin_id = 0x0066_0002;
    let leaf_a = 0x0021_0001;
    let leaf_b = 0x0021_0002;
    let leaf_c = 0x0021_0003;
    // Outer PKIN points at a leaf STAT plus a nested PKIN.
    index.packins.insert(
        outer_pkin_id,
        mk_pkin(outer_pkin_id, "OuterBundle", vec![leaf_a, inner_pkin_id]),
    );
    // Inner PKIN expands into two more leaf STATs.
    index.packins.insert(
        inner_pkin_id,
        mk_pkin(inner_pkin_id, "InnerBundle", vec![leaf_b, leaf_c]),
    );

    let outer_pos = Vec3::new(7.0, 8.0, 9.0);
    let outer_rot = Quat::from_rotation_z(0.25);
    let outer_scale = 1.25;
    let synths = expand_pkin_placements(outer_pkin_id, outer_pos, outer_rot, outer_scale, &index)
        .expect("PKIN-of-PKIN must still fan out");
    assert_eq!(synths.len(), 3, "leaf_a + (leaf_b + leaf_c) flattened");
    let leaf_ids: Vec<u32> = synths.iter().map(|s| s.0).collect();
    assert_eq!(leaf_ids, vec![leaf_a, leaf_b, leaf_c]);
    // All leaves inherit the outer REFR's transform — PKIN has no
    // per-child placement data at any nesting level.
    for s in &synths {
        assert_eq!(s.1, outer_pos);
        assert_eq!(s.2, outer_rot);
        assert_eq!(s.3, outer_scale);
    }
}

/// #635 / FNV-D3-06 — depth cap at MAX_PKIN_DEPTH (4) prevents
/// runaway recursion. Construct a self-referential PKIN (its own
/// CNAM points back at itself); the expander must terminate and at
/// most emit MAX_PKIN_DEPTH synth entries (one per level explored)
/// rather than looping until the stack overflows.
#[test]
fn expand_pkin_self_referential_terminates_at_depth_cap() {
    let mut index = EsmCellIndex::default();
    let cycle_pkin_id = 0x0066_0010;
    // PKIN whose only content is itself — pathological author error.
    index.packins.insert(
        cycle_pkin_id,
        mk_pkin(cycle_pkin_id, "Cycle", vec![cycle_pkin_id]),
    );

    let synths = expand_pkin_placements(cycle_pkin_id, Vec3::ZERO, Quat::IDENTITY, 1.0, &index)
        .expect("self-reference must still return Some(_) for the depth-cap leaf");
    // At the cap, the expander stops recursing and emits the last
    // form ID as a leaf. Verify recursion terminated cleanly with a
    // single trailing leaf entry (the caller's `stat_miss` accounting
    // handles the bogus form ID downstream).
    assert!(
        !synths.is_empty(),
        "depth-cap fallback must still produce a leaf so the bogus \
         form ID is logged via stat_miss rather than silently dropped"
    );
    assert!(
        synths.len() <= 8,
        "MAX_PKIN_DEPTH (4) caps blow-up; observed {} synths",
        synths.len()
    );
    // Every synth points at the cycle PKIN's own ID (the only content
    // entry at every level).
    for s in &synths {
        assert_eq!(s.0, cycle_pkin_id);
    }
}

/// #635 / FNV-D3-06 — children that are NOT PKINs pass through
/// unchanged. The recursive helper must not mistakenly probe
/// non-PKIN form IDs against `index.packins`.
#[test]
fn expand_pkin_non_pkin_children_pass_through_unchanged() {
    let mut index = EsmCellIndex::default();
    let pkin_id = 0x0066_0020;
    let stat_a = 0x0030_0001;
    let stat_b = 0x0030_0002;
    index.packins.insert(
        pkin_id,
        mk_pkin(pkin_id, "FlatBundle", vec![stat_a, stat_b]),
    );
    // Note: no nested PKINs registered.

    let synths = expand_pkin_placements(pkin_id, Vec3::ZERO, Quat::IDENTITY, 1.0, &index)
        .expect("flat PKIN still expands");
    assert_eq!(synths.len(), 2);
    assert_eq!(synths[0].0, stat_a);
    assert_eq!(synths[1].0, stat_b);
}
