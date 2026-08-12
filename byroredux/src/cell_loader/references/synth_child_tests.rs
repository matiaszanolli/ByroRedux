//! Synthetic-child spawn + REFR script-instance selection tests.
//!
//! Extracted from `references/mod.rs`'s inline `mod tests`
//! (#2409 / TD1-006). Contents unchanged.

use super::synth_child::stamp_visible_when_distant;
use super::*;
// Test-only symbols not referenced by production code in this module
// (they'd warn as unused at file scope). #1877 split.

/// #1890 / DELTA-01 — the spawn-path half of the VWD chain: a base record
/// whose `visible_when_distant` flag is set ends with a `VisibleWhenDistant`
/// marker on its placement root, and an unflagged one does not. Complements
/// the record→flag pin in `esm/cell/tests/addn_stat.rs` (#1889), closing the
/// parse→spawn plumbing the audit flagged as untested.
#[test]
fn stamp_visible_when_distant_marks_only_flagged_roots() {
    let mut world = World::new();

    let flagged = world.spawn();
    stamp_visible_when_distant(&mut world, flagged, true);
    let unflagged = world.spawn();
    stamp_visible_when_distant(&mut world, unflagged, false);

    let q = world
        .query::<VisibleWhenDistant>()
        .expect("VisibleWhenDistant storage exists after one insert");
    assert!(
        q.get(flagged).is_some(),
        "a VWD-flagged base record must stamp the marker on its placement root",
    );
    assert!(
        q.get(unflagged).is_none(),
        "an unflagged base record must NOT carry the marker",
    );
}

/// #2026 / SCR-D7-NEW2-01 — a VMAD-carrying SCOL/PKIN outer REFR must
/// attach its own script instance to the first synthetic child only;
/// every later child gets `None`, not a copy. Pre-fix, every
/// synthetic child received the same `Some(&script_instance)`, so a
/// SCOL/PKIN expansion would instantiate the outer REFR's behavior
/// (including `OnCellLoadEvent`) once per decorative piece instead
/// of once per REFR.
#[test]
fn refr_script_instance_attaches_to_first_synth_child_only() {
    let script_instance = esm::records::script_instance::ScriptInstanceData {
        version: 5,
        object_format: 2,
        scripts: vec![esm::records::script_instance::ScriptInstance {
            name: "MyTriggerScript".to_string(),
            status: 0,
            properties: Vec::new(),
        }],
    };

    assert_eq!(
        refr_script_instance_for_synth_child(0, Some(&script_instance)),
        Some(&script_instance),
        "the first synthetic child (idx 0) must receive the outer REFR's VMAD",
    );
    for idx in 1..5 {
        assert_eq!(
            refr_script_instance_for_synth_child(idx, Some(&script_instance)),
            None,
            "synthetic child {idx} must NOT receive a copy of the outer REFR's VMAD",
        );
    }
    // A REFR with no VMAD at all: every child (including the first)
    // correctly gets `None` — nothing to propagate in the first place.
    assert_eq!(refr_script_instance_for_synth_child(0, None), None);
}
