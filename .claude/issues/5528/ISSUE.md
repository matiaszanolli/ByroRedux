# #5528: PERF-D3-2026-10-09-01: #5275 fixed the warning but not the allocation; staging-arena growth is still retried every frame under persistent host-visible pressure

**Labels**: bug, low, memory, performance, renderer

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-10-09.md` — finding `PERF-D3-2026-10-09-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: LOW
- **Dimension**: GPU Memory Pressure
- **Location**:
  - `crates/renderer/src/texture_registry/dynamic_rgba.rs:146-173`: the growth attempt. It returns `Ok` on failure and leaves every update dirty.
  - `:296-301`: #5275's re-arm site.
- **Status**: NEW. This is the residual half of the closed #5275.
- **Description**: #5275's Impact listed two per-frame costs under a persistent BAR or host-visible failure: "one `log::warn!` per frame … **plus one host-visible allocation attempt of about 8.3 MB (1080p) or 33 MB (4K) per frame**". `7e43a1550` moved the warning's re-arm. Its commit message says the warning flag is what caused the per-frame allocation attempt. The code says otherwise:
  - `staging_skip_logged` gates only `note_staging_skip`'s log line.
  - Whenever dirty bytes exceed the slot's arena, `GpuBuffer::create_host_visible` is called again on every frame. When no existing block fits, that means gpu-allocator tries a fresh `vkAllocateMemory`, which then fails.
- **Evidence**: `dynamic_rgba.rs:146-149` (the size check), `:164-172` (`Err(e) => { note_staging_skip(..); return Ok(()); }`). No backoff state exists anywhere in `DynamicRgbaUploads`.
- **Impact**: one failing driver-side allocation per frame for the whole memory-pressure episode, which is exactly when VRAM and BAR headroom matter most. Failure path only; the frame still renders and the overlay keeps its last uploaded image. Unmeasured.
- **Related**: #5275 (closed), #4889 (the degrade-not-die contract this keeps).
- **Suggested Fix**: back off growth retries during an episode. For example, retry every N frames, or only after the dirty byte total shrinks or a free event fires. Reuse the episode flag as the backoff key. A unit test can pin "skip → skip → skip calls the allocator once per backoff window".

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
