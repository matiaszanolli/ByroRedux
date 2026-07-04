//! Static source-assertion tests for scheduler `Access` declaration
//! completeness (AUDIT_CONCURRENCY_2026-07-02, #1785 / #1787).
//!
//! These can't be live-tested: the declarations live inline in `App::new`'s
//! scheduler construction, which needs a live window + Vulkan context to
//! run. Instead each test greps the actual `main.rs` source for the exact
//! `.reads::<T>()` / `.writes::<T>()` call the fix added, so a future edit
//! that drops one back out of the declaration (while the system body still
//! touches it) fails the build instead of silently regressing the "UNION
//! across all paths" invariant these declarations promise.

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
