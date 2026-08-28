# #3406 — RT-2026-08-27-02: MeshRegistry::upload leaks the vertex GpuBuffer when the index buffer fails, has no empty-input guard, and swallows the real error

Labels: medium, renderer, memory, game:skyrim, bug
Filed: 2026-08-27 by `/audit-publish docs/audits/AUDIT_RUNTIME_2026-08-27.md`
Source report: `docs/audits/AUDIT_RUNTIME_2026-08-27.md`

---

Source: `docs/audits/AUDIT_RUNTIME_2026-08-27.md` — RT-2026-08-27-02 (live headless runs at `969d81c8`).

- **Severity**: MEDIUM
- **Dimension**: renderer resource lifetime / diagnosability
- **Location**: `crates/renderer/src/mesh.rs` (`MeshRegistry::upload`, ~:499-517); `byroredux/src/scene/nif_loader.rs:830-835`
- Related to the CLOSED #656 safety net and CLOSED #87.

## Description

Three defects on one error path, all exercised 23× per `WhiterunDragonsreach` load today (see #3402):

1. `upload()` binds `vertex_buffer` to a local, then `?`-propagates `create_index_buffer`. On failure the vertex `GpuBuffer` is dropped without `destroy()`, so the #656 `Drop` safety net reclaims it and logs `GpuBuffer dropped without destroy() — running cleanup from Drop` (`crates/renderer/src/vulkan/buffer.rs:1625`). The buffer is reclaimed, so this is not a leak in release — but the same arm carries `debug_assert!(false, "GpuBuffer leaked into Drop: call destroy() first")` (`buffer.rs:1631-1633`), which would **abort a debug build 23 times on a single cell load**.
2. There is no empty-input guard. `vertices.is_empty()` or `indices.is_empty()` produces a `VkBufferCreateInfo.size == 0`, which is also a spec violation (`VUID-VkBufferCreateInfo-size-00912`) that only escapes notice because the allocator rejects the allocation a moment later.
3. `nif_loader.rs:832` formats the `anyhow::Error` with `{}`, printing only the outermost context (`Failed to allocate buffer_staging staging memory`) and discarding the `InvalidAllocationCreateDesc` source that names the real cause. #3402 needed an instrumented build only because of this.

## Evidence

`MeshRegistry::upload` at HEAD:

```rust
let vertex_buffer = GpuBuffer::create_vertex_buffer(…)?;
let index_buffer  = GpuBuffer::create_index_buffer(…)?;   // `?` drops vertex_buffer
```

No `is_empty()` check on either slice anywhere in `upload` or `upload_scene_mesh`. The consumer:

```rust
log::warn!("Failed to upload NIF mesh '{}': {}", mesh.name.as_deref().unwrap_or("?"), e);
```

`{}` on an `anyhow::Error` prints only the top context; `{:#}` prints the source chain.

## Impact

A whole class of degenerate geometry is diagnosed as "out of staging memory", which is what a reader would reasonably chase first; and no debug build can load Dragonsreach.

## Suggested Fix

Early-return an explicit error from `upload` when either slice is empty; wrap `vertex_buffer` in a guard (or `destroy()` it) on the index-buffer error arm, mirroring the `StagingGuard` pattern already used inside `create_device_local_buffer`; switch the `nif_loader` log to `{:#}`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
