# #4006 — REN-2026-09-06-D12-03: the TAA-failure recovery would rewrite a possibly-pending descriptor set (latent behind D12-02)

**Labels**: low, renderer, sync, bug

---

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
