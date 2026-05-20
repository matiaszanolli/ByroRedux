//! Tests for `sky_params_cleanup_tests` extracted from ../cell_loader.rs (refactor stage A).
//!
//! Same qualified path preserved (`sky_params_cleanup_tests::FOO`).

//! Regression coverage for #626 — `SkyParamsRes` texture handles
//! must reach `unload_cell`'s drop list. The Vulkan-dependent half
//! of `unload_cell` can't be unit-tested in isolation, so we cover
//! the upstream contract: `SkyParamsRes::texture_indices` enumerates
//! every bindless slot the struct owns. A future contributor who
//! adds a 6th slot but forgets to extend `texture_indices` will see
//! the count assertion fail; the comment on `texture_indices` then
//! redirects them to update both sites.
use super::*;

fn mk_sky(indices: [u32; 5]) -> SkyParamsRes {
    SkyParamsRes {
        zenith_color: [0.0; 3],
        horizon_color: [0.0; 3],
        lower_color: [0.0; 3],
        sun_direction: [0.0, 1.0, 0.0],
        sun_color: [1.0; 3],
        sun_size: 1.0,
        sun_intensity: 1.0,
        sun_angular_radius: 0.020,
        is_exterior: true,
        cloud_tile_scale: 1.0,
        cloud_texture_index: indices[0],
        sun_texture_index: indices[4],
        cloud_tile_scale_1: 1.0,
        cloud_texture_index_1: indices[1],
        cloud_tile_scale_2: 1.0,
        cloud_texture_index_2: indices[2],
        cloud_tile_scale_3: 1.0,
        cloud_texture_index_3: indices[3],
        current_dalc_cube: None,
    }
}

/// Every distinct texture-index field on `SkyParamsRes` must be
/// surfaced by `texture_indices`. Adding a new bindless slot
/// without extending the helper would regress #626 (texture
/// refcounts leaked across cell unloads).
#[test]
fn texture_indices_enumerates_all_five_slots() {
    let sky = mk_sky([10, 20, 30, 40, 50]);
    let mut indices: Vec<u32> = sky.texture_indices().into_iter().collect();
    indices.sort();
    assert_eq!(indices, vec![10, 20, 30, 40, 50]);
}

/// The `SkyParamsRes` insert path must round-trip through the
/// resource API so `unload_cell`'s `try_resource` + `remove_resource`
/// sequence sees the values written by `scene.rs`. Guards against a
/// future World API change that would break the resource type
/// dispatch for this resource specifically.
#[test]
fn sky_params_resource_round_trip() {
    let mut world = World::new();
    world.insert_resource(mk_sky([1, 2, 3, 4, 5]));
    {
        let sky = world.try_resource::<SkyParamsRes>().expect("present");
        let mut got: Vec<u32> = sky.texture_indices().into_iter().collect();
        got.sort();
        assert_eq!(got, vec![1, 2, 3, 4, 5]);
    }
    let removed = world
        .remove_resource::<SkyParamsRes>()
        .expect("remove returns Some");
    let mut got: Vec<u32> = removed.texture_indices().into_iter().collect();
    got.sort();
    assert_eq!(got, vec![1, 2, 3, 4, 5]);
    assert!(world.try_resource::<SkyParamsRes>().is_none());
}

/// #803 / STRM-N2 — `CloudSimState` is a survives-transitions
/// resource, independent of `SkyParamsRes`'s lifecycle. Pre-#803 the
/// four cloud_scroll accumulators lived on `SkyParamsRes` and got
/// reset to `[0, 0]` on every interior↔exterior transition,
/// producing a visible cloud snap-back (~0.5 UV per 30 s indoors).
///
/// Lifting the accumulators onto a separate `CloudSimState`
/// resource keeps the drift alive across any cycle that touches
/// `SkyParamsRes`, the same pattern `GameTimeRes` uses for game-time
/// persistence.
///
/// The test inserts a `CloudSimState` carrying non-zero scroll
/// (simulating "the player has been outside for a while"), then
/// performs a manual remove/reinsert cycle on `SkyParamsRes` —
/// asserting `CloudSimState` survives untouched on both sides of
/// the cycle. Note: post-#1199 production `unload_cell` no longer
/// performs the remove half of this cycle (the resources are
/// worldspace-scoped), but the test's invariant about
/// `CloudSimState`'s independence still holds and is worth pinning.
#[test]
fn cloud_sim_state_survives_sky_params_unload_reload() {
    use crate::components::CloudSimState;
    let mut world = World::new();
    world.insert_resource(mk_sky([1, 2, 3, 4, 5]));
    world.insert_resource(CloudSimState {
        cloud_scroll: [0.42, 0.13],
        cloud_scroll_1: [0.71, 0.05],
        cloud_scroll_2: [0.18, 0.91],
        cloud_scroll_3: [0.50, 0.50],
    });
    // Manual remove/reinsert cycle on `SkyParamsRes` — pins the
    // independence invariant. Production `unload_cell` no longer
    // performs the remove half (worldspace-scoped per #1199), but if
    // a future contributor reintroduces it, `CloudSimState` must
    // still survive.
    world
        .remove_resource::<SkyParamsRes>()
        .expect("SkyParamsRes was inserted");
    world.insert_resource(mk_sky([1, 2, 3, 4, 5]));
    let clouds = world
        .try_resource::<CloudSimState>()
        .expect("CloudSimState must NOT be removed by unload_cell");
    assert_eq!(clouds.cloud_scroll, [0.42, 0.13]);
    assert_eq!(clouds.cloud_scroll_1, [0.71, 0.05]);
    assert_eq!(clouds.cloud_scroll_2, [0.18, 0.91]);
    assert_eq!(clouds.cloud_scroll_3, [0.50, 0.50]);
}

/// #1199 — `unload_cell` historically released worldspace-scoped
/// resources (SkyParamsRes / CellLightingRes / WeatherDataRes /
/// WeatherTransitionRes) and their bindless texture handles on every
/// cell unload, expecting `apply_worldspace_weather` to re-acquire on
/// the next cell load. The M40 streaming refactor moved acquisition
/// to a single bootstrap call (scene.rs:226) and never re-instated
/// per-cell re-acquire. The first cell-out-of-range event over-
/// released the texture refcount (bindless slot redirected to the
/// fallback checkerboard) and wiped WeatherDataRes — `weather_system`
/// early-returned for the rest of the session, silently freezing
/// exterior lighting after the first cell-boundary crossing.
///
/// `unload_cell` is Vulkan-dependent and can't run in a unit test;
/// pin the regression at the source level by asserting the per-cell
/// release patterns are not present in `unload.rs`. Re-adding any of
/// them without a matching per-cell re-acquire in
/// `load_one_exterior_cell` regresses #1199.
#[test]
fn unload_cell_does_not_release_worldspace_resources() {
    let src = include_str!("unload.rs");
    let banned_patterns: &[&str] = &[
        "world.remove_resource::<SkyParamsRes>",
        "world.remove_resource::<CellLightingRes>",
        "world.remove_resource::<WeatherDataRes>",
        "world.remove_resource::<WeatherTransitionRes>",
    ];
    for pat in banned_patterns {
        assert!(
            !src.contains(pat),
            "unload.rs must not contain `{pat}` — worldspace-scoped per #1199. \
             If a per-cell release is intentional, also add the matching \
             per-cell re-acquire in load_one_exterior_cell.",
        );
    }
}
