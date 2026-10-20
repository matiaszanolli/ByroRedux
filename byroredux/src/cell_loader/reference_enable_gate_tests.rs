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
    assert!(!reference_is_disabled(&world, DISABLED_REF, false, None));
}

/// #4327 — the per-REFR predicate answers from the REFR's own form id, with
/// no `FormIdPool` round trip, and agrees with the interned-id form.
#[test]
fn the_per_reference_predicate_agrees_with_the_placement_predicate() {
    let (mut world, disabled, live) = world_with_two_refs();
    let mut state = byroredux_scripting::ReferenceEnableState::default();
    state.set_enabled(DISABLED_REF, false);
    world.insert_resource(state);

    assert!(reference_is_disabled(&world, DISABLED_REF, false, None));
    assert!(!reference_is_disabled(&world, LIVE_REF, false, None));
    assert_eq!(
        reference_is_disabled(&world, DISABLED_REF, false, None),
        placement_is_disabled(&world, Some(disabled))
    );
    assert_eq!(
        reference_is_disabled(&world, LIVE_REF, false, None),
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

/// #4813 — the authored "Initially Disabled" flag is the spawn default: it
/// disables an untouched reference (with or without a ledger installed), a
/// scripted `Enable()` overrides it, and a scripted `Disable()` still wins on
/// a reference the plugin authors live.
#[test]
fn initially_disabled_is_the_default_a_scripted_enable_overrides() {
    let (mut world, _, _) = world_with_two_refs();
    assert!(
        reference_is_disabled(&world, LIVE_REF, true, None),
        "the authored flag stands with no ledger installed"
    );

    world.insert_resource(byroredux_scripting::ReferenceEnableState::default());
    assert!(reference_is_disabled(&world, LIVE_REF, true, None));
    assert!(!reference_is_disabled(&world, LIVE_REF, false, None));

    world
        .resource_mut::<byroredux_scripting::ReferenceEnableState>()
        .set_enabled(LIVE_REF, true);
    assert!(
        !reference_is_disabled(&world, LIVE_REF, true, None),
        "a scripted Enable() overrides Initially Disabled"
    );

    world
        .resource_mut::<byroredux_scripting::ReferenceEnableState>()
        .set_enabled(DISABLED_REF, false);
    assert!(reference_is_disabled(&world, DISABLED_REF, false, None));
}

/// #4813 / #3278 — `spawn_placed_instances` takes the caller's per-REFR
/// answer rather than re-deriving it from `placement_form_id_pair`, which
/// only the primary SCOL/PKIN child carries. Pin that the call site hands it
/// the per-REFR flag and the function no longer looks it up itself.
#[test]
fn spawn_placed_instances_takes_the_per_reference_disabled_answer() {
    const SPAWN_RS: &str = include_str!("spawn.rs");
    const SYNTH_RS: &str = include_str!("references/synth_child.rs");
    let body = SPAWN_RS
        .split("pub(super) fn spawn_placed_instances(")
        .nth(1)
        .expect("spawn_placed_instances");
    let body = &body[..body.find("\n}\n").expect("function end")];
    assert!(body.contains("if placement_disabled {"));
    assert!(
        !body.contains("placement_is_disabled("),
        "the primary-child-only form id lookup must not gate content"
    );
    assert!(body.contains("PlacementContentWithheld"), "#4820 marker");
    let call = SYNTH_RS
        .split("spawn_placed_instances(")
        .nth(1)
        .expect("synth_child call");
    let call = &call[..call.find(");").expect("call end")];
    assert!(call.trim_end().ends_with("placement_disabled,"));
}

/// #6077 — an `XESP` child's state is its parent's. A non-inverted child of a
/// disabled marker parent (FNV's Hoover Dam aftermath set: parent
/// `0015D98C` is Persistent + Initially Disabled) spawns disabled even with
/// no flag of its own, an inverted one spawns enabled, a script enabling the
/// parent cascades to both, and a script toggling the child itself has no
/// effect (GECK *Enable*: "it will have no effect"). A chain resolves to its
/// top parent, flipping per inverted hop.
#[test]
fn an_enable_parent_child_follows_its_parents_resolved_state() {
    use byroredux_plugin::esm::cell::EnableParent;
    const MARKER: u32 = 0x0015_D98C;
    const SCORCH: u32 = 0x0015_E4AF;
    const INVERTED: u32 = 0x0015_E4B5;
    const GRANDCHILD: u32 = 0x0015_E4C0;
    let child = |inverted| Some(EnableParent { form_id: MARKER, inverted });
    let (mut world, _, _) = world_with_two_refs();
    world.insert_resource(byroredux_scripting::ReferenceEnableState::default());

    // No authored table: the parent counts as enabled (the old heuristic).
    assert!(!reference_is_disabled(&world, SCORCH, false, child(false)));
    assert!(reference_is_disabled(&world, INVERTED, false, child(true)));

    world.insert_resource(byroredux_scripting::AuthoredDisabledReferences {
        initially_disabled: [MARKER, INVERTED].into(),
        enable_parents: [
            (SCORCH, EnableParent { form_id: MARKER, inverted: false }),
            (INVERTED, EnableParent { form_id: MARKER, inverted: true }),
            (GRANDCHILD, EnableParent { form_id: INVERTED, inverted: false }),
        ]
        .into(),
    });
    assert!(reference_is_disabled(&world, SCORCH, false, child(false)), "the parent starts disabled");
    assert!(
        !reference_is_disabled(&world, INVERTED, true, child(true)),
        "opposite of a disabled parent, whatever the child's own flag says"
    );
    assert!(byroredux_scripting::reference_enabled(&world, GRANDCHILD), "follows the inverted child");

    world
        .resource_mut::<byroredux_scripting::ReferenceEnableState>()
        .set_enabled(SCORCH, true);
    assert!(
        reference_is_disabled(&world, SCORCH, false, child(false)),
        "a script cannot enable a child independently of its parent"
    );

    world
        .resource_mut::<byroredux_scripting::ReferenceEnableState>()
        .set_enabled(MARKER, true);
    assert!(!reference_is_disabled(&world, SCORCH, false, child(false)), "Enable() cascades");
    assert!(reference_is_disabled(&world, INVERTED, true, child(true)));
    assert!(!byroredux_scripting::reference_enabled(&world, GRANDCHILD));
}

/// #6077 corpus check: FalloutNV.esm's `XESP` children against the
/// pre-#6077 "every parent enabled" rule. The audit census found 1,876
/// children that spawned although their parent starts disabled and 72
/// skipped although they should be visible; the Hoover Dam aftermath set
/// under `0015D98C` is the named example.
#[test]
#[ignore = "needs FNV game data on disk; parses the whole master"]
fn installed_fnv_enable_parent_children_follow_their_parents() {
    let data = std::env::var("BYROREDUX_FNV_DATA").unwrap_or_else(|_| {
        "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data".to_string()
    });
    let esm_path = std::path::Path::new(&data).join("FalloutNV.esm");
    let Ok(esm_bytes) = std::fs::read(&esm_path) else {
        eprintln!("skipping: {esm_path:?} not available");
        return;
    };
    let index = byroredux_plugin::esm::parse_esm(&esm_bytes).expect("parse FalloutNV.esm");
    let mut world = World::new();
    byroredux_scripting::install_authored_disabled_references(&mut world, &index);
    let children: Vec<_> = index
        .cells
        .cells
        .values()
        .chain(index.cells.exterior_cells.values().flat_map(|grid| grid.values()))
        .chain(index.cells.worldspace_persistent_cells.values())
        .flat_map(|cell| cell.references.iter())
        .filter(|placed| placed.enable_parent.is_some_and(|parent| parent.form_id != 0))
        .collect();
    let (mut now_hidden, mut now_visible) = (0, 0);
    for placed in &children {
        let was_visible = !placed.enable_parent.unwrap().inverted;
        let disabled = reference_is_disabled(&world, placed.form_id, placed.initially_disabled, placed.enable_parent);
        match (was_visible, !disabled) {
            (true, false) => now_hidden += 1,
            (false, true) => now_visible += 1,
            _ => {}
        }
    }
    eprintln!(
        "FNV XESP children: {}; spawned-but-parent-disabled now hidden {now_hidden}, \
         skipped-but-visible now shown {now_visible}",
        children.len()
    );
    for child in [0x0015_E4AF, 0x0015_E4B5] {
        assert!(
            !byroredux_scripting::reference_enabled(&world, child),
            "{child:08X} (Hoover Dam aftermath) starts hidden behind its disabled marker"
        );
    }
    assert!(now_hidden > 1_000, "the disabled-parent children must hide: {now_hidden}");
}
