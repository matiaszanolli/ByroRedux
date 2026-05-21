# #1191 — SAFE-D7-NEW-01: `bind_inverses_persistent` slot 0 never initialized

**Severity**: HIGH
**Dimension**: D7 — new compute pipeline safety
**Source audit**: `docs/audits/AUDIT_SAFETY_2026-05-19.md`
**Introduced**: `5be66790` (M29.6, this session)

## One-line

Pool-overflowed skinned entities read `palette[0..MBPM] = identity × UNDEFINED = UB` because `bind_inverses_persistent[0..MBPM]` is never written.

## Sites

- Allocation that leaves persistent SSBO uninitialised: `crates/renderer/src/vulkan/scene_buffer/buffers.rs:501`
- Slot 0 reservation contract: `crates/core/src/ecs/resources.rs:557-575`
- Vertex shader consumer: `crates/renderer/shaders/triangle.vert:135-158`

## Fix recipe

Pick option 2 (cheapest, no staging-buffer reintroduction):
- In `SceneBuffers::new` after `allocate_scene_render_buffers`, issue `cmd_update_buffer` with `MAX_BONES_PER_MESH × 64 B` of identity matrices targeting `bind_inverses_persistent.buffer` at offset 0.
- The inline update fits the 65536 B Vulkan limit (9216 B for MBPM=144).
- Needs a one-time queue submit OR ride along the existing init command-buffer if one exists at `VulkanContext::new`.

## Test recipe

CPU-side numeric test that doesn't need a live GPU:
- Assert: given `bone_world[0..MBPM] = identity` and `bind_inverses[0..MBPM] = identity` (the post-init state), the per-slot multiply produces identity. (Trivial — but documents the contract.)
- The deeper test (live GPU readback) requires harness infra that doesn't exist yet.

## Next step

```
/fix-issue 1191
```
