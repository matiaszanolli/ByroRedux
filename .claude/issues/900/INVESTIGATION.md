# Investigation — #900 (FNV-D5-NEW-01)

## Audit premise vs current code (verified at HEAD `318fcaf`)

- `SKIN_MAX_SLOTS = 32` hardcoded at [context/mod.rs:1102](crates/renderer/src/vulkan/context/mod.rs#L1102) ✓
- Pool sized at `max_slots × MAX_FRAMES_IN_FLIGHT × 3` storage-buffer descriptors at [skin_compute.rs:250-260](crates/renderer/src/vulkan/skin_compute.rs#L250-L260) ✓
- Retry-without-cache pattern at [draw.rs:501-513](crates/renderer/src/vulkan/context/draw.rs#L501-L513) ✓ — every frame re-fires `create_slot` on entities whose previous attempt returned `OUT_OF_POOL_MEMORY`

## Comment-vs-reality drift

The pool-sizing comment at `skin_compute.rs:248-249` reads:

> `// max_slots == 32 (matches MAX_TOTAL_BONES / MAX_BONES_PER_MESH)`
> `// covers every realistic interior cell.`

Math is wrong: `MAX_TOTAL_BONES = 32 768` (`scene_buffer.rs:33`) and
`MAX_BONES_PER_MESH = 128` (`crates/core/src/ecs/components/skinned_mesh.rs:29`).
The actual ratio is `32 768 / 128 = 256`, not 32. The 32 was picked
independently of any architectural ceiling. The bone-palette SSBO has
room for 256 simultaneous skinned meshes; the skin-compute pool admits
32. **8× headroom in the bone palette is being wasted on a stale cap.**

## Fix scope

Two-track fix per the audit's "belt-and-braces" suggestion:

1. **Capacity bump** — `SKIN_MAX_SLOTS: 32 → 64`. Closes the immediate
   Prospector overflow with ~2× headroom over the observed ~30 skinned
   entities. Each slot costs `3 × MAX_FRAMES_IN_FLIGHT = 6` storage-buffer
   descriptors → 64 slots = 384 descriptors total, well under any
   reasonable device limit. Picking 64 over 256 (the architectural
   ceiling) keeps the cap as a pressure signal rather than saturating it.

2. **Retry suppression** — `failed_skin_slots: HashSet<EntityId>` on
   `VulkanContext`. Insert on `create_slot` Err; check membership before
   retrying. Clear on **any LRU eviction** (when a slot frees up,
   capacity opened — re-test the previously-failing entities). This
   covers the entity-respawn case naturally: `EntityId` is generational,
   so a despawned entity's id won't be reused; on a contrived 100-skinned
   scene the cache caps at `count - SKIN_MAX_SLOTS` and only entries
   for genuinely-failing entities persist.

## Lifecycle hook for cache clear

The LRU eviction at [draw.rs:705-716](crates/renderer/src/vulkan/context/draw.rs#L705-L716) is
the right hook — `skin_slots.remove(&eid)` happens there when a slot
goes idle past `min_idle`. When that fires, capacity opens up and any
entry in `failed_skin_slots` could now succeed. Clearing the entire set
on eviction is O(N) where N is the few entries cached; cheap.

## Files touched (4)

1. `crates/renderer/src/vulkan/context/mod.rs` — bump `SKIN_MAX_SLOTS`,
   add `failed_skin_slots` field + Default init.
2. `crates/renderer/src/vulkan/skin_compute.rs` — fix the false math
   comment, document the new chosen ceiling rationale.
3. `crates/renderer/src/vulkan/context/draw.rs` — gate retry on cache;
   insert on Err; clear cache on LRU eviction.

3 files, well within the `>5` scope-check threshold.

## Test strategy

`SkinComputePipeline` and `VulkanContext` are awkward to mock without a
real Vulkan device — the tests in this crate are integration-shaped, not
unit-shaped. Following the precedent set by #871 (sibling fix on the
same call site, shipped without a unit test), no regression test ships
with this fix. Verification path: live FNV `--bench-frames 300` on
Prospector — the captured baseline reported 58 WARN lines / 300 frames;
post-fix should be 0.
