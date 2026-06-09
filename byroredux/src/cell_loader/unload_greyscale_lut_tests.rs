//! Regression tests for #1341 / D3-05 — the cell-unload victim walk
//! (`collect_victim_gpu_handles`) must sweep `GreyscaleLutHandle` so the
//! BSEffectShaderProperty greyscale-LUT texture refcount + bindless slot
//! is released on cell unload. Pre-fix the component was attached at
//! spawn (via `resolve_texture`, which bumps the refcount) but omitted
//! from the walk, leaking one texture per distinct LUT per unloaded cell.
//!
//! These exercise the GPU-free collection fn directly (the Vulkan half of
//! `unload_cell` can't run without a `VulkanContext`), so a future change
//! that drops the `GreyscaleLutHandle` arm from the walk fails here.

use super::collect_victim_gpu_handles;
use crate::components::{DarkMapHandle, GreyscaleLutHandle, NormalMapHandle};
use byroredux_core::ecs::{MeshHandle, TextureHandle, World};

/// A victim entity carrying a real greyscale LUT must have that handle
/// collected for drop. This is the exact #1341 leak case.
#[test]
fn unload_walk_collects_greyscale_lut_handle() {
    let mut world = World::new();
    let fallback_tex: u32 = 999;

    let fx = world.spawn();
    world.insert(fx, MeshHandle(1));
    world.insert(fx, GreyscaleLutHandle(42));

    let (_mesh, texture_drops, _terrain) = collect_victim_gpu_handles(&world, &[fx], fallback_tex);

    assert!(
        texture_drops.contains(&42),
        "greyscale LUT handle (42) must be collected for drop_texture on \
         cell unload — its resolve_texture acquire is otherwise leaked (#1341)"
    );
}

/// A LUT that resolved to the registry fallback (handle == fallback_tex)
/// or to 0 must NOT be dropped — those are shared placeholder slots that
/// were never per-cell refcounted. Mirrors the `push_tex_drop` skip rule.
#[test]
fn unload_walk_skips_fallback_and_zero_greyscale_lut() {
    let mut world = World::new();
    let fallback_tex: u32 = 999;

    let fb = world.spawn();
    world.insert(fb, GreyscaleLutHandle(fallback_tex));
    let zero = world.spawn();
    world.insert(zero, GreyscaleLutHandle(0));

    let (_mesh, texture_drops, _terrain) =
        collect_victim_gpu_handles(&world, &[fb, zero], fallback_tex);

    assert!(
        !texture_drops.contains(&fallback_tex),
        "fallback-resolved LUT must be skipped (no per-cell refcount)"
    );
    assert!(
        !texture_drops.contains(&0),
        "handle 0 (placeholder) must never be dropped"
    );
}

/// Sanity: the walk still sweeps the other texture-handle components in
/// the same pass, so this fn fully replaces the previous inline loop and
/// the greyscale add didn't regress the existing coverage.
#[test]
fn unload_walk_collects_all_texture_handle_components() {
    let mut world = World::new();
    let fallback_tex: u32 = 999;

    let e = world.spawn();
    world.insert(e, MeshHandle(7));
    world.insert(e, TextureHandle(10));
    world.insert(e, NormalMapHandle(11, false));
    world.insert(e, DarkMapHandle(12));
    world.insert(e, GreyscaleLutHandle(13));

    let (mesh_drops, texture_drops, _terrain) =
        collect_victim_gpu_handles(&world, &[e], fallback_tex);

    assert!(mesh_drops.contains(&7), "mesh handle must be collected");
    for tex in [10, 11, 12, 13] {
        assert!(
            texture_drops.contains(&tex),
            "texture handle {tex} must be collected by the unload walk"
        );
    }
}
