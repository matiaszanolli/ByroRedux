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

/// #3016 — a synthetic child's own base-record script must attach
/// regardless of whether it's the first child (`synth_idx == 0`) or a
/// later one, because `attach_quest_reference_script` is keyed by
/// `child_form_id`, not by the outer REFR. Two of `spawn_synth_child`'s
/// five branches (LIGH light-only, fxlight) used to wrap this call in
/// `if is_primary_synth { .. }`, which dropped it — along with the
/// unrelated `stamp_quest_reference` call it was bundled with — for
/// every child past the first.
///
/// Pins the fix by simulating exactly the two children's call shape:
/// `refr_script_instance: None` for both (correct for a REFR with no
/// VMAD of its own, and the pre-gated value `refr_script_instance_for_
/// synth_child` always produces past child 0), but each child's own
/// `child_form_id` carrying a distinct base-record `SCRI` → SCPT
/// (Obscript, the M47.0 registry path — lighter to construct than the
/// VMAD/`.pex` path, which needs a real script archive). Both must
/// attach — a regression that re-gates the call would silently drop the
/// second.
#[test]
fn base_record_script_attaches_for_every_synth_child_not_just_the_first() {
    use byroredux_core::ecs::world::World;
    use esm::records::{ActiRecord, ScriptRecord};

    const CHILD_0: u32 = 0x00AA_0001;
    const CHILD_1: u32 = 0x00AA_0002;
    const SCRI_0: u32 = 0x00BB_0001;
    const SCRI_1: u32 = 0x00BB_0002;

    fn spawn_marker(world: &mut World, entity: byroredux_core::ecs::EntityId) {
        use byroredux_scripting::papyrus_demo::RumbleOnActivate;
        let Some(mut q) = world.query_mut::<RumbleOnActivate>() else {
            return;
        };
        q.insert(entity, RumbleOnActivate::default());
    }

    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    let mut registry = byroredux_scripting::ScriptRegistry::new();
    registry.register("FirstChildScript", spawn_marker);
    registry.register("SecondChildScript", spawn_marker);
    world.insert_resource(registry);

    let mut index = esm::records::EsmIndex::default();
    index.activators.insert(
        CHILD_0,
        ActiRecord {
            form_id: CHILD_0,
            editor_id: "Light0".to_string(),
            script_form_id: SCRI_0,
            ..Default::default()
        },
    );
    index.activators.insert(
        CHILD_1,
        ActiRecord {
            form_id: CHILD_1,
            editor_id: "Light1".to_string(),
            script_form_id: SCRI_1,
            ..Default::default()
        },
    );
    index.scripts.insert(
        SCRI_0,
        ScriptRecord {
            form_id: SCRI_0,
            editor_id: "FirstChildScript".to_string(),
            ..Default::default()
        },
    );
    index.scripts.insert(
        SCRI_1,
        ScriptRecord {
            form_id: SCRI_1,
            editor_id: "SecondChildScript".to_string(),
            ..Default::default()
        },
    );

    let mut accum = RefLoadAccum::new();
    let entity0 = world.spawn();
    let entity1 = world.spawn();

    // Both calls use `refr_script_instance: None` — the value every
    // caller already passes for a REFR with no VMAD of its own, and
    // exactly what `refr_script_instance_for_synth_child` returns for
    // every non-first child regardless. Only `child_form_id` differs.
    attach_quest_reference_script(&mut world, entity0, CHILD_0, &index, None, &mut accum);
    attach_quest_reference_script(&mut world, entity1, CHILD_1, &index, None, &mut accum);

    assert!(
        world.has::<byroredux_scripting::papyrus_demo::RumbleOnActivate>(entity0),
        "the first child's own base-record SCRI/SCPT must attach",
    );
    assert!(
        world.has::<byroredux_scripting::papyrus_demo::RumbleOnActivate>(entity1),
        "a LATER child's own base-record SCRI/SCPT must attach too — it \
         is not the outer REFR's VMAD and must not share its \
         child-0-only gate",
    );
    assert_eq!(
        accum.scripts_recognized, 2,
        "both children's base-record scripts count toward the summary",
    );
}
