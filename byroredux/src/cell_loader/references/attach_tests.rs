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

    attach_script_for_refr(&mut world, entity, 0x0000_1234, &index, None);
    assert!(
        !world.has::<byroredux_scripting::OnCellLoadEvent>(entity),
        "no script attached → no OnCellLoadEvent",
    );
}
