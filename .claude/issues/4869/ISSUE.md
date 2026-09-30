# #4869: REN-D1-2026-09-24-03: doc rot after #4779 — shader-pipeline.md steps 4b/5c, record_scatter comment, memory-budget LRU paragraph

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D1-2026-09-24-03**._

**Severity**: LOW · **Status**: NEW · every claim below was checked against the code by the auditor and re-checked against the tip of `main` at publish time.

**Dimension**: AS Correctness

#4779 left stale wording in `docs/engine/shader-pipeline.md` step 4b (says volumetrics still ray-queries on a failed build) and step 5c (`tlas_handle(frame)`, code is `ray_query_tlas`) and in `groundcover.rs` `record_scatter`'s `None` comment; `docs/engine/memory-budget.md` "### LRU eviction" omits the `StaticBlasWorkingSet::can_evict` protection; `static_working_set.rs` is missing from CLAUDE.md's acceleration tree. **Existing: #4795** (the removed "Fit projection" row, still at `memory-budget.md:600`) and **#4801** (`tlas.rs` `PipelineKey` comment) are not re-argued.

## Completeness Checks
- [ ] **SIBLING**: The same stale claim is checked in the neighbouring docs and code comments (doc moves with the pass)

