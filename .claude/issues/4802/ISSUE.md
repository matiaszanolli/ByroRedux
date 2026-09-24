# #4802: PERF-D3-2026-09-23b-02: Census-only mesh provenance is recorded on every upload and never pruned on free

**Severity**: LOW
**Labels**: low, performance, renderer, memory, bug
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23b.md (PERF-D3-2026-09-23b-02)

- **Severity**: LOW
- **Dimension**: GPU Memory Pressure (host-side retention). Merges PERF-D7-2026-09-23b-04.
- **Location**:
  - `crates/renderer/src/mesh.rs:372-382` (field), `:986-1006` (`note_mesh_provenance`), `:1071-1110` (`release_mesh_ref`, no `remove`), `:1178` (cleared only in `destroy_all`);
  - consumer `mesh/geometry_residency.rs:116,225-228` (gated on `BYROREDUX_GEOMETRY_CENSUS=1`).
- **Status**: NEW (`ff1b48d7c`)
- **Description**:
  - Seven production sites write an owned label into a std `HashMap<u32, MeshProvenance>` on every upload; the cell path uses `format!` + `to_owned`.
  - Handles are never reused, and `release_mesh_ref` frees the GPU buffers but leaves the entry.
  - The census reads only live handles.
  - The sibling `geometry_cache` *is* pruned on free, which shows the intended pattern.
- **Impact**: *est.* ~150 B of host RAM per mesh ever uploaded; tens of MB over a long exterior soak. Nothing per frame, nothing on the GPU.
- **Suggested Fix**: Remove the entry in `release_mesh_ref`'s last-holder arm, or record provenance only when the census env var is set.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
