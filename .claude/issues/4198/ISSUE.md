# PERF-D3-2026-09-11-03: The static-BLAS batch log line says "Cell BLAS batch" for a per-REFR batch

Labels: low,performance,renderer,doc-rot,bug

**Description**: `log::info!("Cell BLAS batch: {built}/{} meshes", ...)` runs once per placed reference, not once per cell; a cell load emits hundreds of these. An operator diagnosing BLAS residency reads them as per-cell totals — this mislabeling is what made PERF-D3-2026-09-11-01's reachability gap non-obvious.

**Evidence**:
`byroredux/src/cell_loader/spawn.rs:748-751`.

**Impact**: Telemetry-only; no runtime hazard, but actively misleads BLAS-residency triage.

**Related**: PERF-D3-2026-09-11-01 (the finding this mislabeling obscured).

**Suggested Fix**: Re-label to the REFR scope; accumulate a real per-cell figure in the caller if wanted.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md`.*
