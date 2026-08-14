# CON-D1-01: shrink_tlas_to_fit destroys the AS while set-1 binding 2 still names it, re-opening #2673's window

- **Issue**: [#2929](https://github.com/matiaszanolli/ByroRedux/issues/2929)
- **Finding ID**: `CON-D1-01`
- **Labels**: `medium,sync,vulkan,renderer,bug`
- **Source report**: [`docs/audits/AUDIT_CONCURRENCY_2026-08-14.md`](../../../docs/audits/AUDIT_CONCURRENCY_2026-08-14.md)
- **Run**: `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2929 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: Vulkan Queue & AS Sync
- **Location**: `crates/renderer/src/vulkan/acceleration/memory.rs` (`AccelerationManager::shrink_tlas_to_fit`); call site `crates/renderer/src/vulkan/context/draw.rs` (`draw_frame`, the end-of-frame shrink block); consumers `crates/renderer/src/vulkan/acceleration/tlas.rs` (`AccelerationManager::tlas_handle`), `crates/renderer/src/vulkan/scene_buffer/descriptors.rs` (`SceneBuffers::write_tlas`), `crates/renderer/src/vulkan/scene_buffer/buffers.rs` (`build_scene_descriptor_bindings` + the binding-flags map)
- **Status**: NEW (nearest neighbours: #2673 CLOSED — the `ensure_tlas_state` half of the same class, fixed; #2774 OPEN — the *scratch* shrink, a different symbol)
- **Description**: `shrink_tlas_to_fit` unconditionally takes the slot and calls
  `destroy_acceleration_structure(old.accel, None)` plus three `GpuBuffer::destroy`s. It does **not**
  clear `SceneBuffers::tlas_written[slot]` and does not re-point scene descriptor set-1 binding 2,
  which keeps naming the just-destroyed `VkAccelerationStructureKHR` until the next successful
  `write_tlas` on that slot.

  In the happy path this is harmless: the next frame on that slot re-enters `ensure_tlas_state`
  (which sees `tlas[slot].is_none()`), creates a replacement, and `write_tlas` re-points binding 2
  *before* the render pass begins. The problem is the failure path. The #2673 fix's fallback arm in
  `draw_frame` is written against the premise that a failed `build_tlas` still leaves an AS alive:

  > "Re-point the binding at whatever AS the manager still owns (post-#2673 a failed resize keeps
  > the previous one alive)"

  `shrink_tlas_to_fit` is the one path that falsifies that premise. After a shrink, the slot owns
  nothing, so on the next frame `accel.tlas_handle(frame)` returns `None`, the
  `if let Some(stale_handle)` guard does not fire, and **binding 2 is left naming a destroyed
  acceleration structure for the whole geometry pass.**

  Binding 2 is **not** `PARTIALLY_BOUND` — `build_scene_descriptor_bindings`'s flag map applies
  `DescriptorBindingFlags::PARTIALLY_BOUND` only to `b.binding >= 5`. `triangle.frag` declares and
  statically uses `topLevelAS`, so the "descriptors must be valid if statically used" rule applies
  in full; the `rtEnabled`/`sceneFlags.x` runtime gate does not downgrade static use to dynamic use.
- **Evidence**:
  ```rust
  // memory.rs — shrink_tlas_to_fit: destroys, does not touch tlas_written or binding 2
  if let Some(mut old) = self.tlas[slot_index].take() {
      self.accel_loader.destroy_acceleration_structure(old.accel, None);
      old.buffer.destroy(device, allocator);
      old.instance_buffer.destroy(device, allocator);
      old.instance_buffer_device.destroy(device, allocator);
  }
  ```
  ```rust
  // draw.rs — the #2673 fallback, whose Some(..) arm cannot fire after a shrink
  if let Some(stale_handle) = accel.tlas_handle(frame) {
      self.scene_buffers.write_tlas(&self.device, frame, stale_handle);
  }
  self.scene_buffers.tlas_written[frame] = false;
  ```
  ```rust
  // scene_buffer/buffers.rs — binding 2 is NOT partially bound
  if b.binding >= 5 { vk::DescriptorBindingFlags::PARTIALLY_BOUND } else { vk::DescriptorBindingFlags::empty() }
  ```
  Reachability of the shrink itself, from `tlas_instance_should_shrink` +
  `WORKING_SET_FLOOR == MIN_TLAS_INSTANCE_RESERVE == 8192` + `TLAS_REBUILD_SLACK_BYTES == 1 MiB`:
  it needs `max_instances > 16384`, which `ensure_tlas_state`'s `max(2 × instance_count, 8192)`
  padding reaches at >8192 live TLAS instances. `MAX_INSTANCES == 0x40000`, so this is an ordinary
  large-exterior→interior walk, not a synthetic case.
- **Impact**: Not a use-after-traverse — the #2673 defence in depth still runs unconditionally
  (`tlas_written[frame] = false` + `patch_camera_rt_flag(.., 0.0)`), and I verified every
  `rayQueryInitializeEXT` in `triangle.frag` is behind `rtEnabled` / `directShadowRayEnabled` /
  `giRayEnabled` / `reflectionGlassRayEnabled`, with the water pass host-gated on the same
  `tlas_written[frame]` signal in `geometry_pass.rs`. So no ray actually traverses the dead handle.
  What remains is a bound-and-statically-used-but-invalid descriptor for one or more frames —
  `VUID-vkCmdDraw*-None-08114` class. Blast radius widens to a real traversal only if the
  `patch_camera_rt_flag(0.0)` call *also* fails (it is `log::warn!`-only), in which case
  `rt_flag` stays 1.0 over a destroyed AS.
- **Trigger Conditions**: (a) a frame whose TLAS instance count leaves the other frame-in-flight
  slot's `max_instances` more than 2× the working floor with >1 MiB of slack — i.e. walking from a
  >8192-instance exterior into a small interior, which fires `shrink_tlas_to_fit(other_slot)` at the
  end of `draw_frame`; **and** (b) the *next* frame on that slot failing `build_tlas` — in practice a
  host-visible or device-local allocation failure inside `ensure_tlas_state` (VRAM/BAR exhaustion),
  which is the same VRAM-pressure regime the BLAS budget + LRU machinery exists to survive.
  Deterministically reproducible by fault-injecting a `GpuBuffer::create_*` failure inside
  `ensure_tlas_state` on the frame after a shrink, in the style of `BYRO_FSR_FORCE_DISPATCH_FAIL`.
- **Verification Path**: Not observable in `cargo test` (no headless device). The **state** is
  provable from source order and needs no device. The **runtime consequence** needs the
  validation layer: a `BYRO_VALIDATION=1` release run with the fault injected should report the
  destroyed-object-in-descriptor message at draw time
  (`VUID-vkCmdDrawIndexedIndirect-None-08114` family, "Descriptor in binding #2 index 0 is using
  acceleration structure … that is invalid or has been destroyed"). RenderDoc would show set 1
  binding 2 as an unresolvable AS handle on the affected frame.
- **Related**: #2673 (CLOSED — the `ensure_tlas_state` half; this is the residual its fallback
  cannot cover), #2774 (OPEN — `shrink_tlas_scratch_to_fit`, the sibling shrink), #2141 (CLOSED —
  the identical "recreate failure leaves scene binding N pointing at a destroyed view" shape on the
  SSAO binding), `REN-D1-03` from the renderer half of this suite.
- **Suggested Fix**: Make the shrink symmetric with the fix it undermines. Cheapest correct option:
  have `shrink_tlas_to_fit` clear `tlas_written[slot]` for the slot it retires (it already knows the
  slot index; the flag lives on `SceneBuffers`, so either return the retired-slot index to the
  `draw_frame` call site or thread the flag through). Structurally better: add
  `PARTIALLY_BOUND` to binding 2's flags so an unwritten/retired TLAS binding is a spec-legal
  "descriptor not dynamically used" — that also covers the pre-first-build case symmetrically with
  bindings 5+. Either way, **do not ship without a `BYRO_VALIDATION` run** confirming the descriptor
  message before and after.

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, the sibling BLAS/TLAS path)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CONCURRENCY_2026-08-14.md`](docs/audits/AUDIT_CONCURRENCY_2026-08-14.md) — `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`. Verified CONFIRMED against current code at publish time.*
