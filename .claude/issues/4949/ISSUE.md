# #4949: REN-D1-2026-09-27-02: `refit_skinned_blas` says its index buffer is "the `MeshRegistry`'s global SSBO"; it is the per-mesh index buffer

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4949
- **Labels**: low,renderer,documentation,doc-rot

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D1-2026-09-27-02**._

- **Severity**: LOW (comment built on a false premise)
- **Dimension**: AS Correctness
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_skinned.rs` (`refit_skinned_blas`, the #3469 note above the `index_address` query); `crates/renderer/src/vulkan/context/skinned_blas_refit.rs` (dispatch collection: `mesh.index_buffer.as_ref().expect("skinned mesh requires a per-mesh index buffer")`, gated on `mesh.rt_capable`)
- **Status**: NEW (the comment was introduced by `dd7986793`, #3469)
- **Description**: The comment explains why the index device address is re-queried every refit instead of cached: "the index buffer is the `MeshRegistry`'s global SSBO … a stale-address GPU fault if any realloc site forgets to refresh". Every skinned BUILD/refit source is the mesh's dedicated index buffer. `rt_capable` meshes always own one, and global-only meshes are never `rt_capable` (`global_only_meshes_are_never_rt_capable`). That buffer is fixed for the mesh's lifetime and is never reallocated by the geometry-SSBO rebuild or compaction. The stated hazard therefore does not exist.
- **Evidence**: Call path `record_skinned_blas_refit` → `dispatches.push((…, mesh.index_buffer…buffer, …))` → `SkinnedBlasGeometry { index_buffer: idx_buffer, … }` → `refit_skinned_blas`.
- **Impact**: None at runtime. It misleads auditors, for example into believing compaction (`7e9da5dcc`) or the chunked rebuild can invalidate skinned BLAS inputs. The caching decision it justifies is harmless either way.
- **Related**: #3469; Dim 9 (skinning).
- **Suggested Fix**: Reword it to say the source is the mesh's dedicated index buffer, stable for the mesh's lifetime, and that the query is kept only because the scratch buffer is reallocated from three sites. Optionally cache the index address on the entry.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
