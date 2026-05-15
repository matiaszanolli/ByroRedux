# #1056 — Investigation: MemoryBarrier consolidation in draw.rs

## Audit premise vs reality

Issue body claimed **12 sites across 4 reusable templates**. Reality on
HEAD:

- **6 sites** (not 12)
- **6 distinct stage/access combinations**, not 4 templates

Counted via `grep -c "vk::MemoryBarrier::default" crates/renderer/src/vulkan/context/draw.rs`.

## Site-by-site analysis

| Line | Name | src_access | dst_access | src_stage | dst_stage |
|------|------|-----------|-----------|-----------|-----------|
| 744 | `compute_to_blas` | SHADER_WRITE | ACCELERATION_STRUCTURE_READ_KHR | COMPUTE_SHADER | ACCELERATION_STRUCTURE_BUILD_KHR |
| 855 | `blas_to_tlas` | AS_WRITE_KHR | AS_READ_KHR | AS_BUILD_KHR | AS_BUILD_KHR |
| 959 | (generic) | AS_WRITE_KHR | AS_READ_KHR | AS_BUILD_KHR | FRAGMENT_SHADER &#124; COMPUTE_SHADER |
| 994 | `host_barrier` | HOST_WRITE | SHADER_READ &#124; UNIFORM_READ | HOST | COMPUTE_SHADER |
| 1009 | (cluster) | SHADER_WRITE | SHADER_READ | COMPUTE_SHADER | FRAGMENT_SHADER |
| 1493 | `instance_barrier` | HOST_WRITE | SHADER_READ &#124; UNIFORM_READ &#124; INDIRECT_COMMAND_READ | HOST | (various — line 1502+) |

Sites 2 and 3 differ only in `dst_stage` (one feeds TLAS-build, the
other feeds fragment+compute reads). Sites 4 and 6 differ only in
`dst_access` (one has INDIRECT_COMMAND_READ, the other doesn't). A
templated helper would need 6 distinct entry points — not 4 — to
preserve site-by-site precision.

## Why this fix isn't worth shipping

### 1. The LOC win is approximately zero (possibly negative)

Each in-place barrier is **8–12 lines**, but every one carries a 2–4
line explanatory comment block describing the producer→consumer
boundary it guards. A helper-based version would either:

- Keep the comments + add the helper call (zero LOC win, more
  indirection)
- Drop the comments (loss of context — `grep "BLAS refit"` in
  `draw.rs` is the fastest way to find the relevant barrier)

The "4 templates" framing from the audit body doesn't survive the
per-site detail.

### 2. The `feedback_speculative_vulkan_fixes` doctrine forbids it

The relevant memory entry says:

> Don't ship Vulkan render-pass / pipeline / barrier changes when the
> failure mode is invisible to cargo test. RenderDoc capture or revert
> is the right move, not iterating on speculative hypotheses.

The doctrine cites the 2026-05-01 depth-pre-pass incident: three
rounds of broken graphics on the user's machine because a refactor
that "should be byte-identical" turned out to have subtle FP-drift
issues that only visual inspection caught.

A barrier consolidation has the same risk profile. A subtle
mis-coalescing (e.g., dropping one bit of a dst_access mask in a
helper) produces:

- No `cargo test` failure
- No `cargo check` warning
- A driver-specific hang or visual artifact that surfaces only on the
  next interactive run

### 3. The issue's own Completeness Check requires RenderDoc

The issue body itself flagged this:

> **TESTS**: post-merge `cargo run --release --bench-frames 300` and
> a RenderDoc capture to confirm the per-frame command-buffer shape
> is byte-identical (`feedback_speculative_vulkan_fixes.md` doctrine —
> visual-only refactor of Vulkan recording still needs the RenderDoc
> baseline)

I don't have RenderDoc available in this session, and the failure
mode (invisible barrier misalignment) is exactly what the doctrine
warns about.

### 4. The amplification trigger doesn't fire

Per the audit's own severity scale (LOW unless promotion trigger
fires), this is a LOW finding because:

- No divergent-fix history yet
- No shipped CLI reaches a wrong barrier
- The 6 barriers are individually correct

LOW findings should only be fixed when the cost is genuinely small.
Here, the cost includes a RenderDoc baseline validation that requires
a separate interactive session.

## Recommendation

**Close as `wontfix` (or `no-action-recommended`).** The audit
correctly identified a stylistic concern but the cost of validation
exceeds the LOC win. The companion finding (#1046 — WriteDescriptorSet
helpers in `descriptors.rs`) was a legitimate win because those
helpers operate on host-side struct construction with no GPU-visible
side effects. MemoryBarrier consolidation operates inside the per-frame
command buffer, where the failure mode is precisely the class the
doctrine forbids speculation about.

If a future Vulkan change touches the same call sites (e.g., when
M29.5 GPU palette dispatch lands and adds a new compute→graphics
transition), revisit then — at that point the helper has a real
incremental consumer beyond cleanup-for-its-own-sake.
