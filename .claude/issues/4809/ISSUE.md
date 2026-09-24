# #4809: PERF-D7-2026-09-23b-03: Beam-card matchers run per placement × sub-mesh on the main thread, allocating ~6 Strings each to recompute a per-model result

**Severity**: LOW
**Labels**: low, performance, bug
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23b.md (PERF-D7-2026-09-23b-03)

- **Severity**: LOW
- **Dimension**: Streaming & Cells (placement spawn)
- **Location**: `byroredux/src/cell_loader/spawn/mesh_instance.rs:702-730` (called per REFR from `cell_loader/spawn.rs:773`); `byroredux/src/fog.rs:919,1245,1309,1380`; pre-existing eager `fog_semantics` at `mesh_instance.rs:563-565` (3 lowercases feeding only a `log::debug!`)
- **Status**: NEW (`0572bfd5a`)
- **Description**: `prepare_mesh_uploads` runs inside the budgeted per-REFR loop on the main thread. Three of the seven matchers allocate a `replace` and a `to_ascii_lowercase` before they can reject. The verdict depends only on the model path.
- **Impact**: *est.* 2–3 ms per 2–3k-placement interior, load-time only.
- **Suggested Fix**: Classify once per unique model when the `CachedNifImport` is filled; gate `fog_semantics` behind `log_enabled!(Debug)`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
