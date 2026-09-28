# #4959: REN-D4-2026-09-27-02: The second instance-SSBO grow's safety comment says "before anything is recorded for `frame`"; since #4833 that is false, and the real invariant is unpinned

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4959
- **Labels**: low,renderer,sync,test-gap,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D4-2026-09-27-02**._

- **Severity**: LOW (latent; safe today)
- **Dimension**: Sync/Barriers
- **Location**:
  - `crates/renderer/src/vulkan/context/build_and_upload_instances.rs`: the `#4199` comment above `self.grow_instance_ssbos(frame, gpu_instances.len() + model_tail)`.
  - `crates/renderer/src/vulkan/scene_buffer/upload.rs`: `SceneBuffers::ensure_instance_capacity`, which calls `update_descriptor_sets` on scene set bindings 4/18 under the SAFETY claim "the only command buffer that bound `set` has completed".
- **Status**: NEW
- **Description**: `1e6485313` (#4833) put a first grow in `begin_frame_recording`. The frame then records the RGBA copies, the reservoir clear, and all of `dispatch_skin_and_cluster` (skin, refit, AS barrier, TLAS build, cluster cull, ground-cover interaction and scatter) before `build_and_upload_instances` reaches the second grow. That grow can replace `instance_buffers[frame]` and rewrite `scene set[frame]` bindings 4/18 in mid-recording.

  This is legal only because nothing recorded before it binds `scene_buffers.descriptor_set(frame)` or names `instance_buffers()[frame]`. I verified that:
  - skin, cluster cull, scatter and interaction bind their own sets;
  - the sky bake binds the UPDATE_AFTER_BIND bindless set, and after the grow in any case;
  - the model tier reads the buffer after the grow.

  The call-site comment still claims the window is "before anything is recorded for `frame`", and no test pins the real invariant. The existing guards pin only that grow precedes upload and that the caustic set is rebound.
- **Evidence**:
  - Current comment: "This is after `sync_and_acquire_frame`'s fence wait and before anything is recorded for `frame`, which is the window the grow needs".
  - Recording order: `draw_frame` → `begin_frame_recording` (records) → `dispatch_skin_and_cluster` (records) → `build_and_upload_instances` (grow at the `model_tail` line).
- **Impact**: Suppose a future pre-upload pass binds scene set 1 (any compute pass that wants `instances[]` or the light SSBO through it). On the frames where the grow fires, that pass is invalidated by the descriptor update, since the set has no UPDATE_AFTER_BIND and the command buffer becomes invalid. Those frames are the rare exterior cases: the model-tier tail, or instance-count growth during streaming. The failure would be intermittent, exterior-only, and outside every current validation route.
- **Related**: #4199, #4833, #4413.
- **Suggested Fix**: Correct the comment to state the real invariant. Add a source-order pin that no `descriptor_set(frame)` or `instance_buffers()` use in `begin_frame_recording.rs` / `dispatch_skin_and_cluster.rs` precedes the second grow. Alternatively, fold the tail request into the first grow so the second one is a no-op in steady state.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
