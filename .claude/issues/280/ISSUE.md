# #280: C3-08 — rebuild_geometry_ssbo destroys old SSBO without device_wait_idle

**Severity**: HIGH | **Domain**: renderer | **Type**: bug

## Finding
`rebuild_geometry_ssbo` (`crates/renderer/src/mesh.rs:208-233`) immediately destroys
the old `global_vertex_buffer` and `global_index_buffer` without synchronization.
In-flight command buffers may still reference these via scene descriptor set bindings
8 and 9, creating a use-after-free on the GPU.

## Fix
Call `device_wait_idle()` before destroying, or defer destruction for
`MAX_FRAMES_IN_FLIGHT` frames using a deferred-destroy queue.
