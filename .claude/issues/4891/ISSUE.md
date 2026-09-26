# REN-D5-2026-09-26-13: Failure-path policy is inconsistent across the upload orchestrators (forget vs destroy vs no rollback)

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/4891

**Labels**: low,renderer,memory,bug

- **Severity**: LOW (defence in depth)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/buffer.rs` (`GpuBuffer::create_device_local_buffers_batched`: `std::mem::forget(staging); std::mem::forget(buffers)`), `crates/renderer/src/texture_registry/upload.rs` (`flush_upload_batch` error arm, the `flush_pending_uploads` doc comment), `crates/renderer/src/mesh.rs` (`upload_scene_mesh`, `register_scene_mesh_keyed`, `upload_scene_mesh_global_only`)
- **Status**: NEW
- **Description / Evidence**: (a) The batched buffer path forgets the whole staging arena and every destination buffer on *any* `with_one_time_commands_reuse_fence` error, including the pre-submit failures where nothing was submitted. That strands their `Arc<Mutex<Allocator>>` clones and defeats `Arc::try_unwrap` at shutdown. (b) `flush_upload_batch` destroys immediately on the same error class, while the `flush_pending_uploads` doc says the staging buffers "leak into the pool". (c) `upload_scene_mesh` and `register_scene_mesh_keyed` call `accumulate_global_geometry` before `self.upload(..)?` and do not roll back on failure, unlike `upload_scene_meshes_batched`.
- **Suggested Fix**: Have `with_one_time_commands_inner` return a typed error separating "nothing submitted" (safe to destroy) from "submit or wait failed" (ambiguous), and branch on it in both orchestrators. Extract the batched path's pool rollback and use it in the single-mesh paths. Fix the doc.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl and teardown ordering are still correct
- [ ] **SIBLING**: Same pattern checked in related files (sibling constructors / upload paths / docs)
- [ ] **TESTS**: A regression test pins this specific fix
