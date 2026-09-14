//! Regression coverage for #3278 (SCR-D5-2026-08-24-01) — the runtime
//! consumer for a Papyrus `Disable()`.
//!
//! Before this, `ReferenceEnableState` recorded intent nothing ever read: a
//! script could disable a reference and it stayed fully visible, collidable
//! and interactive. `spawn_placed_instances` now consults it, and gates
//! *before* any mesh, collider or light spawns, so one check covers all three.
//!
//! That function needs a live `VulkanContext`, so — following the
//! `nif_light_spawn_gate_tests` precedent for exactly this problem — the
//! decision lives in the Vulkan-free `placement_is_disabled` predicate and the
//! contract is pinned here.
use super::spawn::{placement_is_disabled, reference_is_disabled};
use byroredux_core::ecs::World;
use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

const DISABLED_REF: u32 = 0x0001_2345;
const LIVE_REF: u32 = 0x0001_9999;

/// Registers a `FormIdPool` and returns the interned ids for the two REFRs.
fn world_with_two_refs() -> (
    World,
    byroredux_core::form_id::FormId,
    byroredux_core::form_id::FormId,
) {
    let mut world = World::new();
    let mut pool = FormIdPool::new();
    let plugin = PluginId::from_filename("Skyrim.esm");
    let disabled = pool.intern(FormIdPair {
        plugin,
        local: LocalFormId(DISABLED_REF),
    });
    let live = pool.intern(FormIdPair {
        plugin,
        local: LocalFormId(LIVE_REF),
    });
    world.insert_resource(pool);
    (world, disabled, live)
}

#[test]
fn a_disabled_reference_is_gated_and_its_neighbours_are_not() {
    let (mut world, disabled, live) = world_with_two_refs();
    let mut state = byroredux_scripting::ReferenceEnableState::default();
    state.set_enabled(DISABLED_REF, false);
    world.insert_resource(state);

    assert!(
        placement_is_disabled(&world, Some(disabled)),
        "#3278: a REFR with a recorded Disable() must not spawn renderable or \
         collidable content"
    );
    assert!(
        !placement_is_disabled(&world, Some(live)),
        "#3278: the gate must be per-REFR, not a global kill switch"
    );
}

/// `Enable()` after a `Disable()` must clear the gate — `set_enabled(.., true)`
/// removes the entry rather than storing a second state.
#[test]
fn re_enabling_clears_the_gate() {
    let (mut world, disabled, _live) = world_with_two_refs();
    let mut state = byroredux_scripting::ReferenceEnableState::default();
    state.set_enabled(DISABLED_REF, false);
    state.set_enabled(DISABLED_REF, true);
    world.insert_resource(state);

    assert!(!placement_is_disabled(&world, Some(disabled)));
}

/// The precombined and loose-NIF spawn paths pass `None` — bake artifacts have
/// no placement-level identity, so there is nothing to disable and the gate
/// must never swallow them.
#[test]
fn a_placement_without_a_form_id_is_never_gated() {
    let (mut world, _disabled, _live) = world_with_two_refs();
    let mut state = byroredux_scripting::ReferenceEnableState::default();
    state.set_enabled(DISABLED_REF, false);
    world.insert_resource(state);

    assert!(
        !placement_is_disabled(&world, None),
        "#3278: precombined / loose-NIF placements carry no form id and must \
         spawn normally"
    );
}

/// A world with no scripting runtime (every cell-loader test fixture, and the
/// loose-NIF CLI route) must behave exactly as it did pre-#3278.
#[test]
fn a_world_without_the_scripting_resource_gates_nothing() {
    let (world, disabled, live) = world_with_two_refs();
    assert!(!placement_is_disabled(&world, Some(disabled)));
    assert!(!placement_is_disabled(&world, Some(live)));
    assert!(!reference_is_disabled(&world, DISABLED_REF));
}

/// #4327 — the per-REFR predicate answers from the REFR's own form id, with
/// no `FormIdPool` round trip, and agrees with the interned-id form.
#[test]
fn the_per_reference_predicate_agrees_with_the_placement_predicate() {
    let (mut world, disabled, live) = world_with_two_refs();
    let mut state = byroredux_scripting::ReferenceEnableState::default();
    state.set_enabled(DISABLED_REF, false);
    world.insert_resource(state);

    assert!(reference_is_disabled(&world, DISABLED_REF));
    assert!(!reference_is_disabled(&world, LIVE_REF));
    assert_eq!(
        reference_is_disabled(&world, DISABLED_REF),
        placement_is_disabled(&world, Some(disabled))
    );
    assert_eq!(
        reference_is_disabled(&world, LIVE_REF),
        placement_is_disabled(&world, Some(live))
    );
}

/// #4326 / #4327 — the gate must sit ahead of every spawn path that never
/// reaches `spawn_placed_instances`: the actor job, the invisible trigger
/// volume, and the LIGH-only / fxlight branches. Those need Vulkan and game
/// data to drive, so pin the wiring in source.
#[test]
fn every_path_that_bypasses_spawn_placed_instances_consults_the_gate() {
    const MOD_RS: &str = include_str!("references/mod.rs");
    const SYNTH_RS: &str = include_str!("references/synth_child.rs");

    let gate = MOD_RS
        .find("reference_is_disabled(")
        .expect("the per-REFR gate in load_references_budgeted");
    let actor_skip = MOD_RS
        .find("if placement_disabled {")
        .expect("the actor arm's disabled branch");
    let actor_job = MOD_RS
        .find("NpcSpawnJob::runtime(")
        .expect("the actor job construction");
    assert!(
        gate < actor_skip && actor_skip < actor_job,
        "a disabled actor must be skipped before its NPC job is built (#4327)"
    );

    let volume = SYNTH_RS
        .find("world.insert(entity, volume);")
        .expect("the trigger volume insert");
    assert!(
        SYNTH_RS[..volume]
            .rfind("if !placement_disabled {")
            .is_some(),
        "a disabled trigger must not receive its volume (#4326)"
    );
    assert_eq!(
        SYNTH_RS.matches(".filter(|_| !placement_disabled)").count(),
        2,
        "both the LIGH-only and the fxlight branch must gate their LightSource (#4327)"
    );
}
