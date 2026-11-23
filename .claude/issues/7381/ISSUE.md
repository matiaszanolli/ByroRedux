# #7381 — REN-D5-2026-11-23-04: the `SceneBuffers::destroy` coverage guard scans only `Vec<GpuBuffer>` families, so a new single `GpuBuffer` field can skip teardown unguarded (RLT-IBL-003's `env_brdf_lut_buffer` is the first)

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_RENDERER_2026-11-23.md` (finding REN-D5-04). Reported by reading: nothing here was run on a GPU.

- **Severity**: LOW (hardening; the new field IS destroyed)
- **Dimension**: Memory/Lifecycle
- **Location**: `buffer_families` / `destroy_walks_every_per_frame_buffer_family` (`scene_buffer/destroy_coverage_tests.rs`); `SceneBuffers::destroy` (`scene_buffer/descriptors.rs`).
- **Status**: NEW
- **Description**: #6521's failure mode (a buffer missed in `destroy` keeps its `Arc<Mutex<Allocator>>` clone, so the shutdown `Arc::try_unwrap` fails and the device/surface/instance leak branch runs) applies to scalar `GpuBuffer` fields too. The scan keys on the `pub(super) <name>: Vec<GpuBuffer>,` shape. Scalar fields (`bind_inverses_persistent`, `bind_inverse_upload_staging`, `terrain_tile_buffer`, `ray_budget_buffer`, and since 84104564e `env_brdf_lut_buffer`) are covered only by hand review. A struct field spelled `pub(crate)` or on a differently shaped type also escapes it.
- **Evidence**: `SceneBuffers` declares those five scalar `GpuBuffer` fields; `SceneBuffers::destroy` destroys each (`self.env_brdf_lut_buffer.destroy(device, allocator)` confirmed), so no leak today; `GpuBuffer::destroy` itself drops the stored allocator Arc (#927), so the scalar fields need no `.clear()`.
- **Impact**: none today.
- **Related**: #6521 (closed), `dd4c50340`.
- **Suggested Fix**: extend `buffer_families` to every field whose type is `GpuBuffer` and require `self.<name>.destroy(device, allocator)` in `destroy`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other passes, other shaders, other call sites)
- [ ] **DROP**: If Vulkan objects change, the Drop / teardown ordering is still correct
- [ ] **TESTS**: A regression test pins this specific fix
