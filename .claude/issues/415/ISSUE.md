# REN-SYNC-C1: TLAS build barrier missing COMPUTE_SHADER dst stage — caustic ray query unsynchronized

**Issue**: #415 — https://github.com/matiaszanolli/ByroRedux/issues/415
**Labels**: bug, renderer, vulkan, sync, critical

---

## Finding

Per-frame TLAS build at `crates/renderer/src/vulkan/context/draw.rs:214-226` emits exactly one post-build memory barrier:

```rust
src_access = ACCELERATION_STRUCTURE_WRITE_KHR  →  dst_access = ACCELERATION_STRUCTURE_READ_KHR
src_stage  = ACCELERATION_STRUCTURE_BUILD_KHR   →  dst_stage  = FRAGMENT_SHADER
```

Main render pass fragment shader is covered. But the caustic compute pass in `crates/renderer/src/vulkan/caustic.rs:682+` issues `rayQueryEXT` against the same TLAS (`DescriptorType::ACCELERATION_STRUCTURE_KHR` at `caustic.rs:276`, confirmed in `caustic_splat.comp`). Caustic runs **after** the main render pass in `COMPUTE_SHADER` stage. Nothing in `caustic::dispatch` emits an AS-build → compute barrier, and the main render pass's outgoing subpass dependency (`helpers.rs:142-158`) only forwards `COLOR_ATTACHMENT_WRITE → SHADER_READ` — not `AS_WRITE → AS_READ`.

## Impact

- AS writes are not made available to the compute ray query.
- On strict drivers this is undefined behavior — ray queries may observe a partially-built TLAS on the caustic dispatch of the same frame.
- Validation layers (synchronization2) will flag it.
- Real hardware has masked it due to tight TLAS-build/dispatch sequencing and cache hierarchy.

## Fix

One-line stage-mask widening at `draw.rs:221`:

```rust
vk::PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::COMPUTE_SHADER,
```

Or emit a dedicated `AS_BUILD → COMPUTE_SHADER` barrier at the top of `caustic::dispatch`.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Verify any other compute passes that sample TLAS (currently only caustic_splat.comp). If SVGF/TAA ever take a ray-query dependency, revisit this barrier.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: With synchronization2 validation layer enabled, run a scene with RT+caustic — assert zero validation errors.

## Source

Audit: `docs/audits/AUDIT_RENDERER_2026-04-18.md`, Dim 1 C1.
