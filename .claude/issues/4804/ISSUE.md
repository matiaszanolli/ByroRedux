# #4804: PERF-D3-2026-09-23b-04: The static working set rebuilds a hash set twice per frame over the TLAS draw set, where a dense handle-indexed stamp would do

**Severity**: LOW
**Labels**: low, performance, renderer, bug
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23b.md (PERF-D3-2026-09-23b-04)

- **Severity**: LOW
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs:468,548`; `blas_static.rs:105-112`; `static_working_set.rs:19-27`
- **Status**: NEW (`b9e961eeb`)
- **Description**:
  - Two rebuilds run each frame: `mark_static_blas_used` → `replace(handles)`, and `build_tlas_instances` → `clear()` plus one `insert` per eligible rigid draw.
  - The set is Fx, so this is not a #2923 regression. But mesh handles are dense slot ids.
- **Impact**: *est.* 50–140 µs/frame at MedTek (13,038 instances).
- **Suggested Fix**: Use a `Vec<u32>` generation stamp indexed by handle, advanced once per real frame.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
