//! Static source-assertion tests for scheduler construction correctness
//! (AUDIT_CONCURRENCY_2026-07-02, #1785 / #1787 / #1788).
//!
//! These can't be live-tested: the declarations and construction order live
//! inline in `App::new`'s scheduler construction, which needs a live window
//! + Vulkan context to run. Instead each test greps the actual `main.rs`
//! source for the exact call the fix added or reordered, so a future edit
//! that drops a declaration back out (while the system body still touches
//! it) or reverts a call order fails the build instead of silently
//! regressing the invariant.

const MAIN_RS: &str = include_str!("main.rs");

/// #1785 / CONC-D3-02 — `apply_color_channels` writes all five
/// `ColorTarget` color-sink storages (Diffuse, Ambient, Specular,
/// Emissive, ShaderColor); the animation system's declaration must
/// claim all five, not just the two it had before the fix.
#[test]
fn animation_declaration_writes_all_five_color_sinks() {
    for ty in [
        "AnimatedDiffuseColor",
        "AnimatedAmbientColor",
        "AnimatedSpecularColor",
        "AnimatedEmissiveColor",
        "AnimatedShaderColor",
    ] {
        let needle = format!(".writes::<byroredux_core::ecs::{ty}>()");
        assert!(
            MAIN_RS.contains(&needle),
            "animation_system declaration is missing `{needle}` — \
             apply_color_channels writes this color sink, see \
             byroredux/src/systems/animation.rs",
        );
    }
}

/// #1787 / CONC-D4-01 — `physics_sync_system`'s body reads `ContactConfig`
/// (register_newcomers) and, behind the opt-in `BYRO_PROFILE_FALLERS`
/// diagnostic, `RenderLayer` / `FormIdComponent` / `PhysicsSourceForm` /
/// `FormIdPool` (all in `dump_awake_fallers`, crates/physics/src/sync.rs).
/// The runtime gate is invisible to the declaration; all five reads must
/// be present regardless.
#[test]
fn physics_sync_declaration_reads_contact_config_and_faller_dump_types() {
    for needle in [
        ".reads::<byroredux_core::ecs::components::RenderLayer>()",
        ".reads::<byroredux_core::ecs::components::FormIdComponent>()",
        ".reads::<byroredux_core::ecs::components::PhysicsSourceForm>()",
        ".reads_resource::<byroredux_core::form_id::FormIdPool>()",
    ] {
        assert!(
            MAIN_RS.contains(needle),
            "physics_sync_system declaration is missing `{needle}` — \
             see crates/physics/src/sync.rs::dump_awake_fallers",
        );
    }
}

/// #1787 / CONC-D4-01 (consolidates CONC-D3-03) — both
/// `physics_sync_system` (register_newcomers) and
/// `player_controller_system` (systems/character.rs) snapshot
/// `ContactConfig` once per tick; both declarations must claim the read.
#[test]
fn contact_config_read_is_declared_on_both_physics_systems() {
    let needle = ".reads_resource::<byroredux_physics::ContactConfig>()";
    let count = MAIN_RS.matches(needle).count();
    assert_eq!(
        count, 2,
        "expected exactly 2 occurrences of `{needle}` (physics_sync_system \
         + player_controller_system), found {count} — see \
         crates/physics/src/sync.rs::register_newcomers and \
         byroredux/src/systems/character.rs",
    );
}

/// #1788 / CONC-D4-02 — `debug_server::start` must run before
/// `install_runtime_registries` in `App::new`: the former adds
/// `DebugDrainSystem` to the scheduler via `add_exclusive`, and the
/// latter snapshots `SystemList`/`SchedulerAccessReport` from the
/// scheduler as it stands at that point. Snapshotting first silently
/// dropped the drain system from the `systems` / `sys.accesses` console
/// output on every debug-mode launch — `debug_server::start`'s own doc
/// comment already states this precondition ("Call this after all
/// systems have been added to the scheduler").
#[test]
fn debug_server_start_runs_before_runtime_registries_snapshot() {
    let start_call = "byroredux_debug_server::start(&mut scheduler, debug_port)";
    let snapshot_call = "Self::install_runtime_registries(&mut world, &scheduler);";

    let start_pos = MAIN_RS
        .find(start_call)
        .unwrap_or_else(|| panic!("`{start_call}` not found in main.rs — App::new changed shape"));
    let snapshot_pos = MAIN_RS.find(snapshot_call).unwrap_or_else(|| {
        panic!("`{snapshot_call}` not found in main.rs — App::new changed shape")
    });

    assert!(
        start_pos < snapshot_pos,
        "debug_server::start (byte {start_pos}) must appear before \
         install_runtime_registries (byte {snapshot_pos}) in App::new, or \
         DebugDrainSystem is silently omitted from `systems`/`sys.accesses`",
    );
}
