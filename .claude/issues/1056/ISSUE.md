## Description

`crates/renderer/src/vulkan/context/draw.rs` has **12** `vk::MemoryBarrier::default()` builder sites, each immediately followed by a `self.device.cmd_pipeline_barrier(…)` call. The barrier shapes are not arbitrary — every site follows one of 4 reusable templates:

1. **COMPUTE_SHADER_WRITE → ACCELERATION_STRUCTURE_BUILD_INPUT_READ** (skin compute → BLAS refit, line 744, 855)
2. **ACCELERATION_STRUCTURE_BUILD_WRITE → SHADER_READ** (BLAS build → TLAS build / fragment ray-query, line ~959)
3. **HOST_WRITE → TRANSFER_READ** (host-visible upload buffer barriers, line ~994)
4. **COLOR_ATTACHMENT_WRITE → FRAGMENT_SHADER_READ** (gbuffer → composite, scattered)

Concrete sites (line numbers from current HEAD):
\`\`\`
744  compute_to_blas barrier
855  blas_to_tlas barrier
959  generic shader barrier
994  host_barrier
[+8 more — full sweep needed]
\`\`\`

Each in-line builder is 8–12 lines of boilerplate. Total inline cost: ~120 LOC of repeated `src_access` / `dst_access` / `src_stage` / `dst_stage` plumbing.

## Severity rationale

**LOW** (default tech-debt). No correctness bug — each barrier is individually correct. The amplification trigger doesn't fire here (no divergent-fix history yet, no shipped CLI reaches a wrong barrier). Worth fixing for **future-correctness** insurance: when the renderer adds more compute → graphics transitions (M29.5 GPU palette dispatch is on deck), having a 4-template helper means new sites can't trivially miscombine stages.

## Proposed fix

Add `crates/renderer/src/vulkan/context/draw/barriers.rs` (or inline in `draw.rs` if file budget permits — file is already 2 571 LOC and on the splittable list per **#1051**):

\`\`\`rust
pub(super) fn record_compute_to_as_build_barrier(device: &ash::Device, cmd: vk::CommandBuffer) { … }
pub(super) fn record_as_build_to_shader_read_barrier(device: &ash::Device, cmd: vk::CommandBuffer) { … }
pub(super) fn record_host_write_to_transfer_read_barrier(device: &ash::Device, cmd: vk::CommandBuffer) { … }
pub(super) fn record_color_attachment_to_shader_read_barrier(device: &ash::Device, cmd: vk::CommandBuffer) { … }
\`\`\`

12 call sites collapse to 12 single-line invocations. Visual-only refactor; no behavior change.

## Completeness Checks

- [ ] **UNSAFE**: barrier sites are inside `unsafe { … }` blocks today; helpers stay `unsafe fn` to preserve the boundary
- [ ] **SIBLING**: TD3-008's WriteDescriptorSet-helper half closed via **#1046** (descriptors.rs builders). This issue is the matching MemoryBarrier half
- [ ] **DROP**: n/a
- [ ] **LOCK_ORDER**: n/a
- [ ] **FFI**: n/a
- [ ] **TESTS**: post-merge `cargo run --release --bench-frames 300` and a RenderDoc capture to confirm the per-frame command-buffer shape is byte-identical (`feedback_speculative_vulkan_fixes.md` doctrine — visual-only refactor of Vulkan recording still needs the RenderDoc baseline)

## Dedup notes

Sibling of **#1046** (CLOSED — WriteDescriptorSet half of TD3-008). Distinct from **#1051** (file split — that issue tracks the *file* coming under 2K LOC; this one tracks the *boilerplate*).
Status: Closed (wontfix)
