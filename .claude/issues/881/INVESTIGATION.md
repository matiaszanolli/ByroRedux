# Investigation — #881 (CELL-PERF-03)

## Domain
renderer (Vulkan texture upload sync)

## Hot path

`cell_loader::spawn_placed_instances`
  → `asset_provider::resolve_texture_with_clamp`
  → `texture_registry::load_dds_with_clamp`
  → `Texture::from_dds_with_mip_chain` (texture.rs:303-570)
  → `with_one_time_commands` (texture.rs:455-520) — **submits + fence-waits per texture**

Each fresh DDS pays a `vkQueueSubmit` + `vkWaitForFences(.., u64::MAX)`. For 100 fresh DDS files in a worldspace edge crossing → ~100 sync stalls on the main thread.

## Existing batching primitives (already in tree)

- `with_one_time_commands_reuse_fence` (texture.rs:706) — reuses the persistent `transfer_fence`. **Texture::from_dds_with_mip_chain does NOT use it** — small ~5µs/call delta, not the main concern.
- `StagingPool` (vulkan/buffer.rs) — pooled staging buffers reused across uploads.
- `texture_registry::pending_writes` (texture_registry.rs:107) — descriptor-set write deferred-flush; the issue cites this as the architectural model to mirror for image uploads.

## Existing `texture_registry` slot lifecycle (key for the design)

- `load_dds_with_clamp` allocates Texture + writes descriptor immediately.
- `drop_texture` redirects descriptor to the fallback when refcount hits 0 (already handles the "slot present but image gone" case via `apply_descriptor_write`).
- Bindless slots are stable indices into `textures: Vec<TextureEntry>`.

## MVP design (this fix)

**Scope: batch fence-waits per cell load, not per-frame budget cap.** The
issue's full design (per-frame budget + streaming) is the gold standard
but a substantially bigger lift; the MVP captures the bulk of the wins
(N×~500µs → 1×~500µs for the data-transfer wait, plus N→1 submit
overhead). The per-frame streaming variant can layer on top later.

1. **`Texture::record_dds_upload`** — extract the record-only block from
   `from_dds_with_mip_chain` (the contents of the `with_one_time_commands`
   closure at lines 455-520, plus the staging+image alloc above). Returns
   `(Texture, StagingGuard)` so the caller manages staging lifetime.
2. **`from_dds_with_mip_chain`** — keep as a thin self-contained wrapper
   around `record_dds_upload` + `with_one_time_commands`. All existing
   callers (scene.rs single-NIF, UI) continue to work.
3. **`TextureRegistry::pending_dds_uploads`** — `Vec<PendingDdsUpload>`
   queue.
4. **`TextureRegistry::enqueue_dds_with_clamp`** — eagerly reserves a
   bindless slot (with `texture: None`, descriptor redirected to
   fallback so any pre-flush draw degrades gracefully), pushes the
   pending upload, returns the handle.
5. **`TextureRegistry::flush_pending_uploads`** — drains the queue with
   ONE `with_one_time_commands_reuse_fence` covering all queued
   uploads, then writes each handle's real descriptor.
6. **`asset_provider::resolve_texture_with_clamp`** — switches the
   miss path from `load_dds_with_clamp` to `enqueue_dds_with_clamp`.
7. **`cell_loader::load_references`** — after the placement loop,
   calls `ctx.texture_registry.flush_pending_uploads(...)` so the
   cell-load completion gate guarantees all textures are GPU-resident
   before the renderer's first draw.

## Files affected

1. `crates/renderer/src/vulkan/texture.rs`
2. `crates/renderer/src/texture_registry.rs`
3. `byroredux/src/asset_provider.rs`
4. `byroredux/src/cell_loader.rs`
5. (optional) `crates/renderer/src/vulkan/context/resources.rs` if a
   convenience wrapper is needed; otherwise none.

## Out of scope (follow-ups)

- Per-frame upload budget cap with `tick_texture_uploads` system.
- RGBA dynamic-texture batched path (UI; not on cell-load hot path).
- Mesh upload batching (sibling — couples with #879 follow-up).

## Test plan

- Unit: `enqueue_dds_with_clamp` reserves a stable handle that
  resolves to fallback descriptor pre-flush.
- Unit: `flush_pending_uploads` on an empty queue is a no-op.
- Pure-Rust pin: bookkeeping invariants
  (path_map / refcount / queue length transitions).
- Integration: full GPU upload requires a Vulkan device — covered by
  the existing cell-load integration tests, no new GPU-bound test.
