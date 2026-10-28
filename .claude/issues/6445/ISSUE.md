# ECS-2026-10-27-03: weather_system reads SkyParamsRes under the CloudSimState write guard

Labels: high, ecs, concurrency, terrain-exterior, bug
Source: docs/audits/AUDIT_ECS_2026-10-27.md

**Source**: `docs/audits/AUDIT_ECS_2026-10-27.md` finding `ECS-2026-10-27-03` (Dimension 1, lock ordering) · Severity HIGH · Status NEW (introduced by 4f2596bf6, 2026-10-26, which added the 20 m/s cloud-drift cap that reads the tile scales). Also noted as a dup source in AUDIT_EXTERIOR_2026-10-27 (CloudSimState <-> SkyParamsRes).

**Location**: `byroredux/src/systems/weather.rs:1264-1285` (write guard on `CloudSimState` at :1264, then `try_resource::<SkyParamsRes>()` at :1271). Opposing order: `byroredux/src/render/sky.rs:128` (`SkyParamsRes` held) -> `:165` (`CloudSimState`, in `outdoor_sky_params`).

## Description
`cap_cloud_layer_drift` needs `SkyParamsRes.cloud_tile_scale*`. The read was placed inside the `if let Some(mut clouds) = world.try_resource_mut::<CloudSimState>()` block. `build_render_data`'s sky pass has always taken `SkyParamsRes` first and `CloudSimState` second.

## Evidence
```rust
if let Some(mut clouds) = world.try_resource_mut::<CloudSimState>() {
    let mut scroll_vectors = cloud_scroll_vectors(...);
    if let Some(sky) = world.try_resource::<SkyParamsRes>() {      // read under the write guard
        cap_cloud_layer_drift(&mut scroll_vectors, [sky.cloud_tile_scale, ...]);
```
```rust
// render/sky.rs
let Some(sky_res) = world.try_resource::<SkyParamsRes>() else { ... };
outdoor_sky_params(world, &sky_res, cell_directional)   // -> try_resource::<CloudSimState>()
```
The detector panics in `render::sky::tests::{the_portal_sun_is_the_sun_above_the_clouds, interior_only_session_bakes_procedural_outdoor_sky, interior_only_boot_uses_live_clock_for_portal_sun}` with "attempted acquisition of `CloudSimState` while holding `SkyParamsRes` … `CloudSimState` -> `SkyParamsRes` -> `CloudSimState`".

## Impact
`weather_system` (Early exclusive) and the render collection both run on the main thread today, so the risk is latent. Any move of render collection off-thread, or a parallel sky consumer, turns it into a real write/read deadlock. It also keeps the CI ABBA lane red.

## Related
ECS-2026-10-27-01/-02, EXT-D4 cloud-drift work (#6416 doc).

## Suggested Fix
Read the four tile scales into a local array before taking the `CloudSimState` write guard. Then compute and cap the scroll vectors and advance under the write guard only.

## Completeness Checks
- [ ] **SIBLING**: Other `weather_system` blocks that take a resource while holding a write guard checked
- [ ] **LOCK_ORDER**: `SkyParamsRes` before `CloudSimState` order is consistent across `weather.rs` and `render/sky.rs`
- [ ] **TESTS**: The three `render::sky::tests` pass under `BYRO_LOCK_ORDER_CHECK=1`

