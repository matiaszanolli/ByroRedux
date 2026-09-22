# UI-D5-2026-09-21-01: #4199 moved the instance-upload clamp to the slot's grown capacity, but the UI overlay's firstInstance guard still compares against MAX_INSTANCES, before the grow

**Issue**: #4722
**Severity**: LOW
**Labels**: low,ui,bug,renderer,vulkan

## Description
#4199 moved the instance/previous-model SSBOs from an eager `MAX_INSTANCES`-sized allocation to a working capacity (`INITIAL_INSTANCE_CAPACITY` = 65,536) that grows on demand via `ensure_instance_capacity`, and made `upload_instances` clamp uploads to `self.instance_capacity[frame_index]` (the slot's *current* grown capacity), not `MAX_INSTANCES` (the 262,144 hard ceiling). But the UI overlay's own overflow guard was never updated to match: `ui_instance_idx` is still fixed against `MAX_INSTANCES` only, and computed *before* `ensure_instance_capacity` runs for the frame.

`build_and_upload_instances.rs`:
```rust
let idx = gpu_instances.len();
...
gpu_instances.push(instance);
// #3601 — ... Skip the overlay for this frame instead ...
(idx < super::super::scene_buffer::MAX_INSTANCES).then_some(idx as u32)
```
runs, then later in the same function:
```rust
// #4199 — ... A failed grow leaves the slot as it was and the upload
// below clamps to it.
match self.scene_buffers.ensure_instance_capacity(..., gpu_instances.len()) { ... }
```
If `gpu_instances.len()` is between the current `instance_capacity[frame]` and `MAX_INSTANCES`, and the grow allocation fails (host-visible buffer creation error under memory pressure), `ui_instance_idx` stays `Some(idx)` even though `idx >= instance_capacity[frame]`. `upload_instances` then drops that entry (`count = instances.len().min(capacity)`), but nothing downstream re-checks `ui_instance_idx` against the actual uploaded range: `post_passes.rs`'s overlay wrapper and `draw.rs`'s `record_overlay` submit it unconditionally as `firstInstance`. With `robust_buffer_access` not enabled (`device.rs:652`), `ui.vert`'s `instances[gl_InstanceIndex]` read past the allocated SSBO is undefined, feeding a garbage `textureIndex` into `ui.frag`'s bindless `nonuniformEXT` sampling — the exact consequence #3601 closed.

The existing #3601 regression test (`ui_instance_idx_is_clamped_to_none_past_max_instances`) is source-shape and only pins the `MAX_INSTANCES` comparison, so it still passes; the #4199 pins (`instance_capacity_growth_pin`) never mention the UI clamp at all.

## Evidence
Verified at HEAD `ee6d3fb39`:
- `crates/renderer/src/vulkan/context/build_and_upload_instances.rs:597-617` (`ui_instance_idx` capture, gated only on `MAX_INSTANCES`, computed before the grow at `:683`).
- `crates/renderer/src/vulkan/scene_buffer/upload.rs` (`upload_instances`): `let capacity = self.instance_capacity[frame_index]; let count = instances.len().min(capacity);` — clamps to the slot, not `MAX_INSTANCES`.
- `crates/renderer/src/vulkan/scene_buffer/upload.rs` (`ensure_instance_capacity`) doc: "On allocation failure the slot keeps its current buffers and capacity, and the upload clamps to it."

## Impact
Needs both an instance count above the current slot capacity (any scene denser than 65,536 initial-capacity instances — plausible for dense Skyrim/FO4 city cells) AND a failed host-visible grow allocation in the same frame — a narrow, memory-pressure-dependent window, the same reasoning that rated the original #3601 LOW. When it lands, the consequence is #3601's: an OOB SSBO read feeding a garbage bindless texture index for the UI overlay quad specifically.

## Related
- #3601 (closed) — the original finding; this is a regression via the new #4199 grow path, not the original overflow path (which the `MAX_INSTANCES` check still covers correctly).
- #4199 (closed) — introduced the working-capacity/grow mechanism that reopened this.
- A short comment is being added to #3601 pointing at this regression.

## Suggested Fix
Compute `ui_instance_idx` *after* `ensure_instance_capacity` runs, checked against the slot's actual `instance_capacity[frame]` (expose it beside the existing `instance_buffer_size(frame)` accessor), not the `MAX_INSTANCES` ceiling. Move `ui_instance_idx_is_clamped_to_none_past_max_instances` (or add a sibling) to pin the capacity-based expression instead of/alongside the `MAX_INSTANCES` one.

## Completeness Checks
- [ ] **TESTS**: A regression test simulates a failed grow with `gpu_instances.len()` between `instance_capacity[frame]` and `MAX_INSTANCES`, asserting `ui_instance_idx` becomes `None`
- [ ] **SIBLING**: Confirm no other post-#4199 consumer of `gpu_instances.len()`/instance indices still compares against `MAX_INSTANCES` instead of the per-frame slot capacity (see also the renderer-domain sibling: scene draw batches have no per-draw clamp at all, filed separately)

Source: docs/audits/AUDIT_UI_2026-09-21.md (UI-D5-2026-09-21-01)
