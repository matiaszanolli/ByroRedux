# SAFE-D4-03: six unsafe fn carry no # Safety doc section

**Issue**: #2684
**Filed**: 2026-08-12 via `/audit-publish` from `/audit-suite renderer-deep`

- **Severity**: MEDIUM
- **Dimension**: 4 — batched (per the skill's batching rule)
- **Location**:
  - [frame_upscaler.rs](crates/renderer/src/vulkan/frame_upscaler.rs) — `FrameUpscaler::record_native_blit` (:592), `::record_fsr_barriers_before` (:705), `::record_fsr_depth_restore` (:764), `::record_fsr_barriers_after` (:822)
  - [gbuffer.rs](crates/renderer/src/vulkan/gbuffer.rs) — `GBufferAttachment::destroy` (:180)
  - [screenshot.rs](crates/renderer/src/vulkan/context/screenshot.rs) — `screenshot_record_copy` (:101)
- **Status**: NEW (same class as CLOSED #2544 / #2349 / #2131, different sites)
- **Description**: Of 77 `unsafe fn` in the workspace, 71 carry a `# Safety`
  doc section stating the caller contract; these six do not. All are
  private or `pub(super)`, so blast radius is crate-internal, but the four
  `frame_upscaler` ones are the FSR3 boundary barriers — the contract they
  depend on (`cmd` in the recording state, and each image in the specific
  layout the FSR boundary assumes) is *discussed at length* in their prose docs
  yet never written as a caller obligation. `record_fsr_barriers_after`'s doc
  even records a 900-frame validation run against that contract without ever
  stating the contract. `GBufferAttachment::destroy` relies on the standard
  "no in-flight command buffer references these views" obligation — the inner
  blocks say *"caller of `destroy` (an `unsafe fn`) guarantees …"*, forwarding
  to a contract that does not exist at the signature.
- **Evidence**: `record_fsr_barriers_before` (:705) has **no** doc comment at
  all; the preceding lines are the tail of the previous function's body.
  `screenshot_record_copy` (:96-100) documents *when* it is called and the
  expected swapchain layout in prose, but has no `# Safety` heading.
- **Impact**: MEDIUM per `_audit-severity` Special Rules (`unsafe` without a
  safety comment). Practical risk is a caller added later that does not know
  the layout precondition, producing a `VUID-VkImageMemoryBarrier-oldLayout-01197`
  class error that only shows up under validation layers — precisely the
  failure mode `record_fsr_barriers_after`'s own doc warns will reappear on an
  SDK upgrade.
- **Related**: #2544 (CLOSED — fsr3-sys smoke example; verified fixed, 20/23 →
  0 uncommented), #2349 (CLOSED — `post_passes.rs` split regression), #2131.
  **Cross-audit**: overlaps **REN-D23-05** in the renderer audit, which adds the
  mechanism — clippy misses these because they are **PRIVATE** fns.
- **Suggested Fix**: Add a `# Safety` section to each of the six stating the
  caller obligation (recording-state `cmd`, live device-owned images, and for
  the four FSR fns the specific entry layouts); consider enabling
  `#![warn(clippy::missing_safety_doc)]` on `crates/renderer` so the class stops
  recurring per-refactor.

---


---
*Filed from [`docs/audits/AUDIT_SAFETY_2026-08-12.md`](docs/audits/AUDIT_SAFETY_2026-08-12.md) — `/audit-suite renderer-deep`, 2026-08-12. Finding ID `SAFE-D4-03`.*

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
