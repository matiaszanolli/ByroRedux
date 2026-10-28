# REN-D1-01: #6143's split turned the #4199 / #4726 instance-SSBO order pins into definition-order checks

Labels: low, renderer, bug, test-gap
Source: docs/audits/AUDIT_RENDERER_2026-10-27.md

**Source**: `docs/audits/AUDIT_RENDERER_2026-10-27.md` — finding `REN-D1-01` (LOW, Dim 1 AS Correctness, test-gap)
**Location (current)**: `crates/renderer/src/vulkan/context/build_and_upload_instances.rs`, tests `instance_buffers_grow_before_upload_and_rebind_the_caustic_set` (~line 2384) and `the_clamp_runs_after_the_grow_and_before_the_indirect_upload` (~line 1898).

## Description
Both tests compare text offsets over the whole file. Since `3e6f6b66a` (#6143 split) the right-hand needles (`.upload_instances(&self.device, frame, gpu_instances)` and `upload_indirect_draws(`) live in the helper `upload_instance_and_draw_buffers` (defined at ~line 1371, after `build_and_upload_instances` at ~287). Because the helper is defined later in the file, `call < upload` and `clamp < indirect` hold because of where the helper is *defined*, regardless of the real call order inside `build_and_upload_instances`.

## Evidence
- `self.upload_instance_and_draw_buffers(` is called at ~line 738; the `.upload_instances(` needle is at ~1385 and `upload_indirect_draws(` at ~1476 (inside the helper).
- `find(".upload_instances(&self.device, frame, gpu_instances)")` / `find("upload_indirect_draws(")` therefore always land after any call-site needle placed in `build_and_upload_instances`.

## Impact
Moving `self.upload_instance_and_draw_buffers(` above the final `grow_instance_ssbos` or the batch clamp would reintroduce #4199 (upload clamps to the old capacity) or #4726 (batches reference slots past capacity), and both pins would stay green. The call order is correct today.

## Related
#4199, #4726, #4833, #6143, `3e6f6b66a`.

## Suggested Fix
Anchor on the call site `self.upload_instance_and_draw_buffers(` inside a `fn_body` cut of `build_and_upload_instances` (as `nothing_before_the_final_grow_binds_the_scene_set` already does). Pin separately that the helper body contains both the instance upload and the indirect upload.

## Completeness Checks
- [ ] **SIBLING**: Other source-pin tests in this file that needle across the #6143 split (`update_scene_history_gates`, `instance_flags`) are checked for the same definition-order weakness
- [ ] **TESTS**: Verify the strengthened pins fail when the call is moved above the grow/clamp (mutate and revert locally)

