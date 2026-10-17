# REN-VOL-L-01: #5772 left the identical gap one loop below — `integration_param_buffers` local is outside `partial` when `write_mapped` fails

Labels: low,renderer,vulkan,memory,bug
Source: docs/audits/AUDIT_RENDERER_2026-10-17.md

- **Severity**: LOW
- **Dimension**: Memory/Lifecycle (Volumetrics)
- **Location**: `volumetrics/init.rs`, `VolumetricsPipeline::new_inner` §7 "Per-FIF integration parameters" (the `for _ in 0..MAX_FRAMES_IN_FLIGHT` loop)
- **Status**: NEW (sibling of closed #5772 / baseline REN-L-06, which fixed only the moment and occupancy buffers)
- **Description**: `let mut buffer = try_or_cleanup!(… create_host_visible …)`, then `try_or_cleanup!(… buffer.write_mapped(…))`, and only afterwards `partial.integration_param_buffers.push(buffer)`. If `write_mapped` fails, `partial.destroy` does not know the buffer. Same effect class as baseline REN-L-06: not a leak, since `GpuBuffer::Drop` (#656) frees the buffer, but a debug build hits `debug_assert!(false, "GpuBuffer leaked into Drop")`, which masks the `Err` with a panic. Effectively unreachable (a mapped host buffer of exactly `size_of::<IntegrationParams>()`).
- **Evidence**: moment/occupancy sites now `push` first and fill via `.last_mut()`; this loop still fills first.
- **Suggested Fix**: push, then `write_mapped` through `.last_mut()` (the #5772 shape). #5772 shipped without a pin (`init.rs` has no tests); add a source-shape test that no `GpuBuffer` local in `new_inner` is filled before being pushed.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other init/teardown sites)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_RENDERER_2026-10-17.md` (area-scoped `volumetrics-deep` suite run, HEAD e16a624e9). Fix with `/fix-issue <number>`._
