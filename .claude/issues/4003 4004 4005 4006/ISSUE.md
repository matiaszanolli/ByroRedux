===== ISSUE 4003 =====
REN-2026-09-06-D10-03: on a non-`D32_SFLOAT` device `depth.stats` becomes an unbreakable "armed — come back in a frame or two" loop with no console-visible reason
STATE: OPEN
LABELS: bug renderer low memory 

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D10-03), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: Camera-Relative Precision
- **Location**: `byroredux/src/commands/depth.rs` (`DepthStatsCommand::execute`), `crates/renderer/src/vulkan/context/depth_capture.rs` (`VulkanContext::depth_capture_record_copy`)
- **Status**: NEW (the console-facing consequence of the just-landed #3570; the refusal
  itself is correct — see Coverage)
- **Description**: `#3570` correctly made `depth_capture_record_copy` refuse rather than
  misdecode when `find_depth_format` selected `D16_UNORM`, and it consumes the request flag
  before returning, so the refusal is a clean no-op with no layout or leak consequence.
  What it has no path back to is the caller. `DepthCaptureBridge::take_result()` will
  return `None` forever, so `depth.stats` takes its "nothing landed yet" branch on every
  invocation: it re-arms and prints "depth capture armed — run `depth.stats` again in a
  frame or two to read it". The only signal that the capture will *never* land is a
  `log::warn!` in the renderer's log stream, which a `byro-dbg` console session does not
  see.
- **Evidence**: `execute` has exactly two exits for a present bridge — the `take_result()`
  `None` arm (arm + "come back") and the report arm — and no third arm for "capture
  refused". The refusal in `depth_capture_record_copy` writes nothing to
  `depth_capture_result` and never arms `depth_capture_pending_readback`.
- **Impact**: `depth.stats` is the *live* half of #3308's before/after comparison gate
  (the analytic half being `Camera::depth_resolution_at` / `_reversed`). On any device
  whose `find_depth_format` falls through to `D16_UNORM` — Vulkan mandates `D16_UNORM`
  depth-attachment support but not `D32_SFLOAT`, which is the whole premise of #3570 — the
  gate silently has no live half, and the operator's evidence is an instruction to keep
  waiting. Zero impact on the dev RTX 4070 Ti. Note this is *distinct* from the already-filed
  #3571 (`REN-2026-08-30-D10-02`), which is about the gate being runnable only
  pre-conversion; this is about the gate being unrunnable at all on some hardware, with no
  way to tell.
- **Related**: #3570 (`26f9ddf4`), #3308, #3571 (distinct — do not merge), #3630 (the
  degenerate-camera rejection line, which is the precedent for surfacing a refusal to the
  console instead of leaving it ambiguous).
- **Suggested Fix**: Give the refusal a return channel — e.g. have `depth_capture_record_copy`
  store a `DepthCapture`-shaped `Err`/unsupported marker on the bridge (or expose the
  selected `depth_format` on the bridge at init), and add a third arm to
  `DepthStatsCommand::execute` printing "depth capture unsupported: device selected
  {format}, not D32_SFLOAT (#3570)". Mirrors the shape #3630 already established for the
  degenerate-camera case, and stops the command from arming a request that can never
  complete.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **TESTS**: A regression test pins this specific fix

===== ISSUE 4004 =====
REN-2026-09-06-D11-02: the pipeline-cache header gate accepts a `headerSize` larger than the file it validated
STATE: OPEN
LABELS: bug renderer low pipeline 

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D11-02), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/context/helpers.rs` (`validate_pipeline_cache_header`)
- **Status**: NEW
- **Description**: The SAFE-11 / #91 gate exists so "a bad header never
  reaches the driver" — an explicit defence-in-depth argument about a cache
  file "dropped next to the binary by a process with filesystem write
  access". It rejects `len < 32`, `headerSize < 32`, `headerVersion != 1`,
  and vendor/device/UUID mismatch, but deliberately does not upper-bound
  `headerSize` ("a future version might legitimately grow the prefix"). The
  cheap and version-agnostic bound is missing: `headerSize` must not exceed
  the length of the file it describes. A 32-byte file claiming
  `headerSize = 0xFFFF_FFFF` passes every check and is handed to
  `vkCreatePipelineCache` with the correct vendor/device/UUID, which is the
  one shape the gate's own threat model names.
- **Evidence**: the `if header_size < 32 { return false; }` check with no
  companion `header_size as usize > initial_data.len()` arm; the six
  `pipeline_cache_header_tests` cases cover short files, bad version, and the
  three identity fields, not an over-large `headerSize`.
- **Impact**: Defence-in-depth only — the driver re-validates independently
  and a well-behaved one rejects it. The pre-condition (write access to the
  executable's directory) is already a strong position for an attacker.
  Filed because the gate's stated purpose is exactly to catch this, and the
  fix is one comparison plus one test.
- **Related**: SAFE-11 / #91.
- **Suggested Fix**: Add `if header_size as usize > initial_data.len() { return false; }`
  alongside the `< 32` check, and a `pipeline_cache_header_tests` case for it.
  This is compatible with the "future version might grow the prefix" comment
  — a grown prefix still fits inside its own file.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix

===== ISSUE 4005 =====
REN-2026-09-06-D11-03: three stale prose statements on the pipeline / render-pass surface
STATE: OPEN
LABELS: documentation renderer low pipeline shaders doc-rot 

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D11-03), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/context/helpers.rs` (`create_render_pass`'s attachment-3 comment), `crates/renderer/src/vulkan/pipeline.rs` (`create_ui_pipeline`'s doc comment), `docs/engine/shader-pipeline.md` (§"G-Buffer Layout", the sentence after the table)
- **Status**: NEW
- **Description**: Three independent prose claims on this surface no longer
  match the code they describe. Each is small; they are grouped because they
  are one edit's worth of work and all three would mislead the next reader of
  this exact dimension.
  1. `create_render_pass`'s mesh-ID attachment comment says overflow "is
     handled by the warn-once `log::error!` + clamp in `draw.rs::draw_frame` +
     `upload.rs`". The clamp is still in `scene_buffer/upload.rs`, but the
     warn-once "RP-1" `log::error!` moved out of `draw_frame` into
     `context/build_and_upload_instances.rs` under the #3282 phase split.
     `draw.rs`'s only surviving "RP-1" mention is the *indirect-draw* ceiling
     policy in `should_use_indirect_draws`'s doc — a different overflow, on a
     different buffer; the instance-overflow `log::error!` this comment sends
     the reader to `draw_frame` for is not there. Same class of stale pointer
     #3881 just fixed one
     comment above it (the "search `0x80000000u` in `triangle.frag`"
     instruction).
  2. `create_ui_pipeline`'s doc says "water uses its own 128-byte
     push-constant layout on a separate pipeline layout". The separate layout
     is still true; the size is not — `WaterPush` is **16 bytes**, held there
     by `const _: () = assert!(size_of::<WaterPush>() == 16)`, since the
     per-draw payload moved into the `GpuWaterParams[]` SSBO and the push
     block became a compact `{ uint waterIndex; uvec3 _reserved; }` index.
  3. `docs/engine/shader-pipeline.md` §"G-Buffer Layout" ends with "After
     `vkCmdEndRenderPass` all attachments transition to
     `SHADER_READ_ONLY_OPTIMAL`." The depth attachment's `final_layout` is
     `DEPTH_STENCIL_READ_ONLY_OPTIMAL`, not `SHADER_READ_ONLY_OPTIMAL` — and
     that distinction is load-bearing three paragraphs later, where the same
     doc correctly names `DEPTH_STENCIL_READ_ONLY_OPTIMAL` as
     `copy_depth_to_history`'s precondition, and again in
     `depth_capture_record_copy`'s contract (#3628). The doc contradicts
     itself; the code is right.
- **Evidence**: as cited per item above.
- **Impact**: Documentation only. Item 3 is the one worth prioritising — it
  sits in the file the audit skill designates authoritative for G-buffer
  layout, and it disagrees with a layout precondition two other subsystems
  now depend on by name.
- **Related**: #3881 (`bf8ded3d`, which fixed the sibling stale pointer in the
  same comment block), #3282 (the split that moved RP-1), #3628 (the depth
  layout contract), #2757 (the "line numbers rot, name the symbol" rule this
  keeps re-proving).
- **Suggested Fix**: Point item 1 at `build_and_upload_instances`; change item
  2's "128-byte" to "16-byte" (or drop the size and name `WaterPush`); qualify
  item 3 to "all eight colour attachments transition to
  `SHADER_READ_ONLY_OPTIMAL`; depth transitions to
  `DEPTH_STENCIL_READ_ONLY_OPTIMAL`".

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix

===== ISSUE 4006 =====
REN-2026-09-06-D12-03: the TAA-failure recovery would rewrite a possibly-pending descriptor set (latent behind D12-02)
STATE: OPEN
LABELS: bug renderer low sync 

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D12-03), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: Sync/Barriers
- **Location**: `crates/renderer/src/vulkan/context/post_passes.rs` (`record_taa_pass`'s error arm), `crates/renderer/src/vulkan/composite.rs` (`CompositePipeline::fall_back_to_raw_hdr` → `rebind_hdr_views`)
- **Status**: NEW
- **Description**: `record_taa_pass`'s failure arm calls
  `composite.fall_back_to_raw_hdr(&self.device)` from **inside** the frame's
  command-buffer recording. That delegates to `rebind_hdr_views`, which loops
  `for (i, &hdr_view) in hdr_views.iter().enumerate()` over all
  `MAX_FRAMES_IN_FLIGHT` slots and issues
  `device.update_descriptor_sets(&[write_combined_image_sampler(self.descriptor_sets[i], 0, …)], &[])`
  for each. At that point `draw_frame` has waited only `in_flight[frame]`;
  the other slot's command buffer — which bound
  `composite.descriptor_sets[1 - frame]` in `CompositePipeline::dispatch`
  (`cmd_bind_descriptor_sets(… &[self.descriptor_sets[frame], bindless_set] …)`)
  — may still be in the pending state. Updating it violates
  VUID-vkUpdateDescriptorSets-None-03047, and the composite descriptor set
  layout is created with a plain
  `vk::DescriptorSetLayoutCreateInfo::default().bindings(&ds_bindings)` — no
  `VK_DESCRIPTOR_BINDING_UPDATE_AFTER_BIND_BIT` or
  `UPDATE_UNUSED_WHILE_PENDING_BIT` — so neither exemption applies.
  This is **not currently reachable**: per **D12-02** the arm is dead. It is
  filed separately because fixing D12-02 (making a TAA failure latch) makes
  this live, and the two must land together.
- **Evidence**: `rebind_hdr_views`'s `debug_assert_eq!(hdr_views.len(),
  MAX_FRAMES_IN_FLIGHT)` and its per-`i` `update_descriptor_sets` call;
  `composite.rs`'s `create_descriptor_set_layout` call takes no binding-flags
  `p_next`; the only other `rebind_hdr_views` caller is `context/init.rs`
  (construction time, safe).
- **Impact**: Undefined behaviour on a descriptor set read by executing GPU
  work — validation error at minimum, potential device-lost. Zero impact
  today (unreachable); the finding exists so a D12-02 fix does not silently
  open it.
- **Related**: #3605 (`c43cb269`), #2519 (the FSR sibling recovery), D12-02.
- **Suggested Fix**: Defer the rebind rather than doing it mid-recording:
  latch a `composite_needs_raw_hdr_rebind` flag and perform the
  `rebind_hdr_views` at the top of the *next* `draw_frame`, after
  `sync_and_acquire_frame`'s fence wait proves both slots retired — or wrap
  it in a `device_wait_idle()` on this once-per-session path, which is
  acceptable given it fires at most once and already implies a degraded
  session. Prefer the former; it needs no idle stall and matches the
  deferred-destroy convention the rest of the context uses.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix

