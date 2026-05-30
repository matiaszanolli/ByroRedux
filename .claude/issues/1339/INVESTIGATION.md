# Investigation — #1339 (D3-03): worldspace sky-texture leak on transition

## Root cause (confirmed)
`apply_worldspace_weather` (`scene/world_setup.rs`) resolves 4 WTHR cloud layers (via
`resolve_cloud_layer` → `resolve_texture` → `acquire_by_path`) + 1 CLMT sun sprite (via
`load_dds`), each bumping a texture-registry refcount, and stores their handles in a freshly
inserted `SkyParamsRes`. Per #1199 these are worldspace-scoped and intentionally survive
per-cell unload (`unload_cell` skips them). #1199 left a known follow-up: "a future door-walking
worldspace transition will release them at the boundary." That boundary handler now exists
(`main.rs:1218`, the Exterior transition) but the release was never wired — so every
interior→exterior / exterior→exterior crossing re-acquired a fresh set while the prior set's
refcounts stayed pinned forever. The `SkyParamsRes::texture_indices()` accessor built for the
release was dead on the release side.

## Refcount-semantics verification (the subtle part)
The two acquire paths use different APIs, so I verified both bump exactly one ref per call
before releasing:
- Clouds: `resolve_texture` → `acquire_by_path` — bumps on cache hit (#524).
- Sun: `load_dds` → `load_dds_with_clamp` — **also bumps `entry.ref_count` on a cache hit**
  (`texture_registry.rs:519-523`), creates fresh `ref_count: 1` on miss.

So all 5 handles are acquired one ref per call. Releasing the prior set's 5 indices once each is
refcount-correct: a handle shared with the new worldspace drops back to its prior count (stays
resident); one unique to the old worldspace hits 0 and frees its slot. No over-release.

`drop_texture` is frame-safe: shared handles just `release_ref`-decrement (early return, no GPU
drop); a last-ref drop defers the VkImage to `pending_destroy` keyed by `current_frame_id`
(frames-in-flight, #92) and redirects the bindless slot to the fallback so no in-flight
`GpuInstance` reads a freed view. The new `SkyParamsRes` references the new handles, not the
freed slot.

## Fix: single acquire/release pairing inside the acquire function
Rather than wire the release only into the `main.rs` transition handler (the finding's
suggestion), put it inside `apply_worldspace_weather` itself — that covers ALL THREE callers
(startup `scene.rs:227`, `debug_load.rs:296`, the `main.rs:1218` transition) with one change and
makes acquire/release impossible to drift. Order is acquire-new → install new `SkyParamsRes` →
release-old (no transient free+reupload for textures shared between worldspaces). The release is
inside the `if let Some(ref wthr)` block, paired with the new `SkyParamsRes` insert — the
no-weather `else` branch acquires nothing and the prior `SkyParamsRes` legitimately persists, so
it must NOT release.

The which-handles-to-drop logic is extracted into the pure `sky_textures_to_release(prev,
fallback) -> Vec<u32>` (skips `0` and the fallback slot), unit-tested without a `VulkanContext`.

## Sibling completeness
Grepped every `SkyParamsRes` construction: the only production acquire site is
`apply_worldspace_weather` (world_setup.rs:337). The procedural fallback
(`insert_procedural_fallback_resources`, world_setup.rs:497) sets every cloud/sun index to `0`
(acquires nothing → nothing to release) and runs only for the no-plugin synthetic scene, not on
a real-worldspace transition. No other release pairing needed.

## Test
`apply_worldspace_weather` is GPU-bound (can't unit-test), but the release SET is pure:
`sky_textures_to_release` is tested for (a) only real non-fallback handles released, (b) `None`
prior → empty (startup no-op), (c) all-absent → empty. Mirrors the source-level / pure-helper
approach `sky_params_cleanup_tests.rs` uses for this #1199-adjacent area.

## Files (1 production)
- `scene/world_setup.rs` — capture prior indices, `sky_textures_to_release` helper, release loop,
  3 unit tests.
