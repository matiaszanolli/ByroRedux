# #3601 — REN-2026-08-30-D11-01: the UI overlay's `firstInstance` is submitted un-clamped while `upload_instances` drops exactly that instance on overflow

**Labels**: `low,renderer,vulkan,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3601 --json state`.

---

- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` (`draw_frame`, the `ui_instance_idx` append at :3241-3253 and the RP-1 guard at :3298), `crates/renderer/src/vulkan/scene_buffer/upload.rs` (`upload_instances`, :548-556), `crates/renderer/src/vulkan/presentation.rs` (`record_overlay`)
- **Status**: OPEN (pre-existing; not introduced by #3426)
- **Description**: The UI quad's `GpuInstance` is pushed onto `gpu_instances` as the **last**
  element, and `ui_instance_idx = gpu_instances.len() as u32` is captured before the push.
  `upload_instances` then clamps with `let count = instances.len().min(MAX_INSTANCES);` and
  warns that "excess draws silently dropped". Because the UI instance is last, it is the
  *first* thing the clamp discards — yet `ui_instance_idx` is still handed to
  `UiOverlayDraw.instance_index` and issued as `firstInstance` in
  `device.cmd_draw_indexed(cmd, overlay.index_count, 1, 0, 0, overlay.instance_index)`.
  `ui.vert` then reads `instances[gl_InstanceIndex]` past the end of an SSBO allocated at
  exactly `size_of::<GpuInstance>() * MAX_INSTANCES` (`scene_buffer/buffers.rs:468`).
- **Evidence**:
  - `draw.rs:3241` `let ui_instance_idx = if let (Some(ui_tex), Some(_)) = …  { let idx = gpu_instances.len() as u32; … gpu_instances.push(instance); Some(idx) }`
  - `upload.rs:548` `let count = instances.len().min(MAX_INSTANCES);`
  - `ui.vert` `GpuInstance inst = instances[gl_InstanceIndex]; fragTexIndex = inst.textureIndex;`
  - `ui.frag` `outColor = texture(textures[nonuniformEXT(fragTexIndex)], fragUV);`
  - `crates/renderer/src/vulkan/device.rs:652` — the enabled `vk::PhysicalDeviceFeatures`
    chain does **not** request `robust_buffer_access`, so the OOB SSBO load is undefined
    rather than a guaranteed zero; the garbage `textureIndex` then feeds a `nonuniformEXT`
    index into the unbounded bindless `textures[]` array.
  - The existing RP-1 comment at `draw.rs:3287-3297` reasons carefully about the clamp but
    only about *dropped draws* — it does not consider that one of the dropped entries still
    has its index submitted as `firstInstance`.
- **Impact**: Requires `gpu_instances.len() > MAX_INSTANCES` (262,144), a condition that
  already emits a one-shot `log::error!`, so this is a second-order consequence of an
  already-flagged overflow, not an independently reachable bug. But the consequence is
  worse than the documented one ("draws silently dropped"): an out-of-range descriptor-array
  index rather than a missing quad.
- **Needs RenderDoc**: no
- **Suggested Fix**: Clamp at capture — `let idx = gpu_instances.len(); if idx < MAX_INSTANCES { … Some(idx as u32) } else { None }` — so an overflowing frame skips the overlay instead of drawing it from an out-of-range instance slot. One line, and it makes the `None` arm that `record_presentation_pass` already handles do the work.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D11-01

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
