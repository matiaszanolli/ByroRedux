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

/// #3015 — `placed_ref.primitive` (the outer REFR's own `XPRM`) is
/// REFR-level data, authored exactly once, so the trigger-volume branch
/// must spawn for the first synthetic child only, regardless of whether
/// a later child's own base record independently has a mesh or a
/// script. Pre-fix, only `stamp_quest_reference` inside the branch was
/// gated on `is_primary_synth` — the branch itself, and therefore the
/// entity + `TriggerVolume` + `accum.trigger_volumes` count, was not,
/// so a scripted mesh-less SCOL/PKIN expansion spawned one
/// differently-placed volume per child from a single authored XPRM.
#[test]
fn trigger_volume_spawns_for_the_first_synth_child_only() {
    use super::synth_child::trigger_volume_should_spawn_for_synth_child as should_spawn;

    // The exact reachable shape the branch guards against: a later
    // child with no mesh of its own and a script from its OWN base
    // record (or the outer REFR's pre-gated VMAD) must still decline —
    // there is no per-child XPRM to build a volume from.
    assert!(
        !should_spawn(false, false, true),
        "a non-primary, mesh-less, scripted child must NOT spawn a \
         second trigger volume from the outer REFR's one XPRM",
    );
    // The first child, mesh-less and scripted: this is the one and only
    // case that should ever build a volume from `placed_ref.primitive`.
    assert!(
        should_spawn(true, false, true),
        "the first synthetic child of a scripted, mesh-less REFR must \
         spawn the authored trigger volume",
    );
    // Orthogonal gates unaffected by #3015: a visible scripted activator
    // (has a mesh) never takes this branch even as the first child —
    // #1737's whole point is that only genuinely invisible triggers do.
    assert!(!should_spawn(true, true, true));
    // No script at all: nothing to trigger on, regardless of primacy.
    assert!(!should_spawn(true, false, false));
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

/// #2541 / SCR-D7-NEW10-01 — `spawn_synth_child` has no test pinning the
/// `is_primary_synth` gate that keeps a SCOL/PKIN expansion's
/// `SceneAliasCandidate` registration to exactly one entity per REFR
/// (otherwise `SceneActorBindings`'s alias-fill resolution sees N
/// candidates for one authored alias-fillable reference). A live spawn
/// fixture would need a real `VulkanContext` (out of unit-test scope, the
/// same constraint `source_pin_tests.rs`'s sibling pins document), so this
/// pins the invariant by construction:
///
/// 1. `stamp_quest_reference`/`spawn_logical_quest_reference` themselves
///    correctly register exactly one `SceneAliasCandidate` when called
///    exactly once (the shape every gated branch below produces).
/// 2. A source-scan over `spawn_synth_child`'s body confirms every one of
///    its `stamp_quest_reference(`/`spawn_logical_quest_reference(` call
///    sites is actually gated — 8 directly by `if`/`else if
///    is_primary_synth`, plus the trigger-volume branch's `stamp_quest_
///    reference` call, gated one level up by `trigger_volume_should_spawn_
///    for_synth_child(is_primary_synth, ..)`. If a future 9th branch adds
///    an ungated call site, or an existing gate is dropped in a
///    refactor, the call-site count and the gate count diverge and this
///    assertion fails — closing the gap at zero Vulkan-fixture cost.
#[test]
fn is_primary_synth_gates_every_identity_stamp_call_site() {
    // Part 1: the stamping primitives themselves produce exactly one
    // SceneAliasCandidate when called once — the shape every gated branch
    // in `spawn_synth_child` produces for the first (and only the first)
    // synthetic child of a SCOL/PKIN expansion.
    let mut world = World::new();
    world.insert_resource(byroredux_core::form_id::FormIdPool::new());
    let placed_ref = esm::cell::PlacedRef {
        form_id: 0x00AA_0001,
        base_form_id: 0x00AA_0002,
        position: [0.0; 3],
        rotation: [0.0; 3],
        scale: 1.0,
        enable_parent: None,
        teleport: None,
        reputation_ref: None,
        primitive: None,
        linked_refs: Vec::new(),
        location_ref_types: Vec::new(),
        rooms: Vec::new(),
        portals: Vec::new(),
        radius_override: None,
        alt_texture_ref: None,
        land_texture_ref: None,
        texture_slot_swaps: Vec::new(),
        emissive_light_ref: None,
        material_swap_ref: None,
        ownership: None,
        script_instance: None,
        lock: None,
        water_velocity: None,
    };
    let load_order = ["Test.esm".to_string()];

    // Simulate a 3-child expansion: only child 0 is primary.
    for idx in 0..3u32 {
        let is_primary_synth = idx == 0;
        if is_primary_synth {
            spawn_logical_quest_reference(
                &mut world,
                &placed_ref,
                &load_order,
                Vec3::ZERO,
                Quat::IDENTITY,
                1.0,
            );
        }
    }
    let count = world
        .query::<byroredux_scripting::SceneAliasCandidate>()
        .expect("SceneAliasCandidate storage exists after one insert")
        .iter()
        .count();
    assert_eq!(
        count, 1,
        "a multi-child SCOL/PKIN expansion must register exactly one \
         SceneAliasCandidate for the whole REFR, not one per child",
    );

    // Part 2: source-scan `spawn_synth_child`'s own body — every call site
    // must actually be gated, not just the stamping primitives above.
    let src = include_str!("synth_child.rs");
    let start = src
        .find("pub(super) fn spawn_synth_child(")
        .expect("spawn_synth_child must still exist");
    let end = src[start..]
        .find("pub(super) fn trigger_volume_should_spawn_for_synth_child(")
        .expect("trigger_volume_should_spawn_for_synth_child must still follow spawn_synth_child");
    let body = &src[start..start + end];

    let stamp_calls = body.matches("stamp_quest_reference(").count();
    let spawn_logical_calls = body.matches("spawn_logical_quest_reference(").count();
    let total_call_sites = stamp_calls + spawn_logical_calls;

    let direct_gates = body.matches("if is_primary_synth {").count();
    // The trigger-volume branch's `stamp_quest_reference` isn't behind a
    // literal `if is_primary_synth {` — it's gated one level up, by
    // `trigger_volume_should_spawn_for_synth_child(is_primary_synth, ..)`.
    let composed_gates = body
        .matches("trigger_volume_should_spawn_for_synth_child(is_primary_synth,")
        .count();
    let total_gates = direct_gates + composed_gates;

    assert_eq!(
        (stamp_calls, spawn_logical_calls),
        (4, 5),
        "expected 4 stamp_quest_reference + 5 spawn_logical_quest_reference \
         call sites in spawn_synth_child; a changed count means a branch \
         was added/removed — re-verify its is_primary_synth gate before \
         updating this baseline",
    );
    assert_eq!(
        total_gates, total_call_sites,
        "every stamp_quest_reference/spawn_logical_quest_reference call site \
         in spawn_synth_child must be gated by is_primary_synth (directly or, \
         for the trigger-volume branch, via trigger_volume_should_spawn_for_\
         synth_child) — a mismatch means some call site lost its gate",
    );
}
