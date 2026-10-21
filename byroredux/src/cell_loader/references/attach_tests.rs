//! VMAD script-attach path tests.
//!
//! Extracted from `references/mod.rs`'s inline `mod tests`
//! (#2409 / TD1-006). Contents unchanged.

use super::*;
// Test-only symbols not referenced by production code in this module
// (they'd warn as unused at file scope). #1877 split.
use super::attach::attach_vmad_scripts;

/// The Skyrim+ `.pex` attach arm fast-outs when no script archive is
/// supplied: no `--scripts-bsa` (no `ScriptProvider` resource, or an
/// empty one) → returns `false` and attaches nothing, without touching
/// the index. Pins the "no archive → clean miss" contract.
#[test]
fn attach_vmad_scripts_no_ops_without_a_script_archive() {
    use byroredux_core::ecs::world::World;
    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    let index = esm::records::EsmIndex::default();
    let entity = world.spawn();

    // No ScriptProvider resource at all.
    assert!(!attach_vmad_scripts(
        &mut world,
        entity,
        0x0000_1234,
        &index,
        None
    ));

    // An empty ScriptProvider (flag absent) → same clean miss.
    world.insert_resource(crate::asset_provider::build_script_provider(&[]));
    assert!(!attach_vmad_scripts(
        &mut world,
        entity,
        0x0000_1234,
        &index,
        None
    ));
}

/// `attach_script_for_refr` on a base form with no SCPT and no VMAD
/// emits no `OnCellLoadEvent` — the marker fires only when canonical
/// behavior actually attaches (both per-game arms decline cleanly on
/// an empty index).
#[test]
fn attach_script_for_refr_emits_no_event_when_nothing_attaches() {
    use byroredux_core::ecs::world::World;
    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    let index = esm::records::EsmIndex::default();
    let entity = world.spawn();

    attach_script_for_refr(&mut world, entity, 0x0000_1234, &index, None, true);
    attach_script_for_refr(&mut world, entity, 0x0000_1234, &index, None, false);
    assert!(
        !world.has::<byroredux_scripting::OnCellLoadEvent>(entity),
        "no script attached → no OnCellLoadEvent",
    );
    assert!(
        !world.has::<byroredux_scripting::OnInitEvent>(entity),
        "no script attached → no OnInitEvent either",
    );
}

/// #5997 — an attached script's load marker follows whether the entity has 3D:
/// `OnCellLoadEvent` (OnInit + OnCellAttach + OnLoad + OnCellLoad) when it
/// does, `OnInitEvent` alone when it is identity-only.
#[test]
fn attach_script_for_refr_sends_the_load_events_only_to_entities_with_3d() {
    use byroredux_core::ecs::world::World;
    use esm::records::{ActiRecord, ScriptRecord};

    const BASE: u32 = 0x00BB_0001;
    const SCRI: u32 = 0x00BB_0002;

    fn spawn_marker(world: &mut World, entity: byroredux_core::ecs::EntityId) {
        use byroredux_scripting::papyrus_demo::RumbleOnActivate;
        if let Some(mut q) = world.query_mut::<RumbleOnActivate>() {
            q.insert(entity, RumbleOnActivate::default());
        }
    }

    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    world.insert_resource(byroredux_core::form_id::FormIdPool::new());
    let mut registry = byroredux_scripting::ScriptRegistry::new();
    registry.register("Has3dScript", spawn_marker);
    world.insert_resource(registry);
    let mut index = esm::records::EsmIndex::default();
    index.activators.insert(
        BASE,
        ActiRecord { form_id: BASE, editor_id: "Has3dBase".to_string(), script_form_id: SCRI, ..Default::default() },
    );
    index.scripts.insert(
        SCRI,
        ScriptRecord { form_id: SCRI, editor_id: "Has3dScript".to_string(), ..Default::default() },
    );

    let with_3d = world.spawn();
    assert!(attach_script_for_refr(&mut world, with_3d, BASE, &index, None, true));
    assert!(world.has::<byroredux_scripting::OnCellLoadEvent>(with_3d));
    assert!(!world.has::<byroredux_scripting::OnInitEvent>(with_3d));

    let identity_only = world.spawn();
    assert!(attach_script_for_refr(&mut world, identity_only, BASE, &index, None, false));
    assert!(world.has::<byroredux_scripting::OnInitEvent>(identity_only));
    assert!(
        !world.has::<byroredux_scripting::OnCellLoadEvent>(identity_only),
        "no 3D → no OnLoad / OnCellLoad",
    );
}
