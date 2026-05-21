# #1192 — SAFE-D7-NEW-02: `upload_pending_bind_inverses` silently drops excess past cap

**Severity**: MEDIUM
**Dimension**: D7 — new compute pipeline safety
**Source audit**: `docs/audits/AUDIT_SAFETY_2026-05-19.md`
**Introduced**: `5be66790` (M29.6, this session)

## One-line

`MAX_PENDING_BIND_INVERSE_UPLOADS_PER_FRAME = 16` cap drops entries 17+ from the upload but they stay in `pool.entity_to_slot` → render with UB transforms until pool eviction. FO4 MedTek at 23 SkinnedMesh entities is one NPC away from tripping.

## Sites

- Silent drop: `crates/renderer/src/vulkan/scene_buffer/upload.rs:207-231`
- Caller drains pool then passes the full list: `byroredux/src/main.rs:1233-1263`

## Fix recipe

Option 1 (preferred): re-queue the dropped tail.

1. `upload_pending_bind_inverses` returns the dropped tail as well as `capped`.
2. Add `SkinSlotPool::requeue_pending(entries: impl IntoIterator<Item = (u32, EntityId)>)` that extends `pending_uploads`.
3. In `main.rs::RedrawRequested`, after `draw_frame`, push the renderer's reported tail back onto the pool.

Alternative (option 3): just bump the cap to MAX_SKINNED = 226 and grow the staging buffer to ~2 MB. Engine has 6 GB VRAM minimum; cost negligible.

## Test recipe

Unit test on `SkinSlotPool` that:
- Allocates 17+ entities in one frame
- Drains pending → asserts all 17+ are in the drain
- After "renderer cap" simulation (take first 16), call `requeue_pending(tail)`
- Verify next `drain_pending` yields the tail

## Next step

```
/fix-issue 1192
```
