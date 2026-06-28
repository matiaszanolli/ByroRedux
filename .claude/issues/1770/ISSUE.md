# FNV-D1-NEW-01: sky-texture handles leak on transition into a climateless worldspace

**Severity**: MEDIUM · **Source**: `docs/audits/AUDIT_FNV_2026-06-27.md` (FNV-D1-NEW-01)
**Location**: `byroredux/src/scene/world_setup.rs` (`apply_worldspace_weather`, the `else` branch around the `insert_procedural_fallback_resources` call); triggered per-transition from `byroredux/src/main.rs` (`apply_worldspace_weather` call site)
**Status**: NEW (adjacent to closed #1339 / #1199; not a regression of either)

## Description
`apply_worldspace_weather` releases the prior worldspace's ≤5 sky-texture handles (4 WTHR cloud layers + 1 CLMT sun sprite) **only inside the `if let Some(ref wthr) = wctx.default_weather` branch** — the `prev_sky_textures` capture (`world_setup.rs:251`) and the `sky_textures_to_release` drop loop (`:322`). When a runtime worldspace transition lands on a worldspace whose climate/weather does not resolve (`default_weather == None`), control takes the `else` branch (`:349`), which calls `insert_procedural_fallback_resources` (`:361`) and overwrites `SkyParamsRes` **without first capturing or releasing the prior worldspace's handles**.

## Evidence
The release is WTHR-branch-local:
```rust
if let Some(ref wthr) = wctx.default_weather {
    let prev_sky_textures = world.try_resource::<SkyParamsRes>().map(|s| s.texture_indices()); // :251
    ...
    world.insert_resource(sky);
    for handle in sky_textures_to_release(prev_sky_textures, sky_fallback) {                    // :322
        ctx.texture_registry.drop_texture(&ctx.device, handle);
    }
} else {
    insert_procedural_fallback_resources(world, sun_dir);  // :361 — installs a fresh SkyParamsRes, NO prior-handle release
}
```
`apply_worldspace_weather` is the *only* sky-texture release point (the #1199 design scopes them to the worldspace; neither `drain_streaming_state` nor `unload_current_interior` releases them — confirmed by grep: no other `drop_texture` of `texture_indices()` slots). It runs on each real exterior transition.

## Impact
Up to 5 leaked textures (bindless slot + VkImage) **per transition into a worldspace that fails to resolve a WTHR** (corrupt/partial ESM, mod worldspace with no CLMT, bespoke synthetic cell). Bounded per-event (not per-cell, not unbounded) → MEDIUM. Does **not** fire on the vanilla FNV WastelandNV path (which resolves a real climate→weather). Note: closed #1339's INVESTIGATION premise ("the procedural fallback runs only for the no-plugin synthetic scene, not on a real transition") is incorrect — the `else` branch is the live path for any transition into a climateless worldspace.

## Related
Closed #1339 (WTHR→WTHR sky-texture leak), #1199 (worldspace-scoped texture lifetime), #476/#463 (climate/weather degradation to procedural fallback).

## Suggested Fix
Hoist the `prev_sky_textures = world.try_resource::<SkyParamsRes>().map(|s| s.texture_indices())` capture above the `if/else` and run the `sky_textures_to_release` drop loop on **both** branches after the new `SkyParamsRes` is installed. The `else` branch installs an all-zero-index `SkyParamsRes`, so `sky_textures_to_release(prev, fallback)` already filters `0`/fallback correctly (unit-tested at `world_setup.rs:674/681`).

## Completeness Checks
- [ ] **SIBLING**: Confirm no other `insert_procedural_fallback_resources` / `SkyParamsRes` overwrite path skips the release (initial worldspace load, save-restore transition)
- [ ] **DROP**: The added `ctx.texture_registry.drop_texture` calls free the bindless slot + VkImage in the correct order (mirror the existing WTHR-branch drop)
- [ ] **TESTS**: A regression test drives a climateless-worldspace transition and asserts the prior sky-texture slots are released (extend the `sky_textures_to_release` test family)
