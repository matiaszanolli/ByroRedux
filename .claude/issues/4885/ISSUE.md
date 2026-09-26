# REN-D5-2026-09-26-07: `recreate_descriptor_sets` rewrites only live textures — reserved-but-unflushed and dead handles sample an unwritten PARTIALLY_BOUND descriptor after any resize

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/4885

**Labels**: medium,renderer,vulkan,memory,bug

- **Severity**: MEDIUM (undefined shader read of an unwritten descriptor; needs `BYRO_VALIDATION=1` / GPU-assisted validation to observe)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/texture_registry/mod.rs` — `TextureRegistry::recreate_descriptor_sets` (the `rewrites` collection uses `entry.texture.as_ref()`; the per-slot queue is discarded by `pending_set_writes[..].clear()`); redirect contract in `crates/renderer/src/texture_registry/upload.rs` (`enqueue_dds_for_view`) and `release.rs` (`drop_released_texture`)
- **Status**: NEW
- **Description / Evidence**:
  - The pool and sets are recreated, so every element starts unwritten. Only entries with `texture: Some` are rewritten (confirmed in code). The loop comment says dropped slots "will be redirected to the fallback on their next update", but the redirect in `drop_released_texture` is a one-shot at drop time and never re-runs. The queued redirect writes are discarded by the same `clear()`.
  - `texture: None` entries are: (i) reserved and unflushed slots, for which `enqueue_dds_for_view` explicitly writes the fallback "so a draw before the flush samples the checkerboard"; (ii) dead handles from a failed flush (#1922), which stay cache-hit targets and keep being handed to materials; (iii) dropped slots.
  - After a resize, (i) and (ii) are sampled by live draws while unwritten. (i) needs a resize during a multi-frame streaming apply. (ii) needs only a truncated or corrupt archive texture plus any later resize.
- **Impact**: An undefined read of a never-written descriptor: garbage or black sampling, and on some drivers a fault. This is the exact hazard the fallback redirect exists to prevent.
- **Suggested Fix**: In the rewrite pass, write the D2 or cube fallback descriptor for every `texture: None` entry. `TextureEntry` needs to keep its `view_kind`, or derive it from the queue. The `rewrites` plan is pure and can be pinned with a device-free test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (sibling constructors / upload paths / docs)
- [ ] **TESTS**: A regression test pins this specific fix
