# TD8-002: Three stale `#[allow(dead_code)]` in `groundcover_translate.rs` — #4054 already gave them a production consumer

Labels: low,tech-debt,terrain-exterior,bug

**Description**: `SUPPRESSION_KEYWORDS`, `AFFINITY_KEYWORDS`, and `layer_affinity` all carry a dead-code annotation claiming no consumer exists; #4054 wired `layer_affinity` into `cell_loader/terrain.rs`'s real (non-test) `CellSplatLayer` builder, which flows into `render/groundcover.rs`'s GPU upload. The sibling `layer_affinities` (plural) is still genuinely uncalled and should NOT be touched.

**Evidence**:
`byroredux/src/groundcover_translate.rs:82, 91, 172` (doc block at 61-72 also needs updating); confirmed live via direct read (`#[allow(dead_code)] // see DEFAULT_AFFINITY — Phase 1 scatter is the consumer`).

**Impact**: No runtime impact — the stale attribute suppresses a warning that would otherwise correctly indicate these are live, and the doc block overstates what's still pending (only the GPU-side `groundcover_scatter.comp` `affinity(splat)` dispatch remains pending, not the CPU-side consumer).

**Related**: #4054 (gave `layer_affinity` its production consumer).

**Suggested Fix**: Remove the three attributes; tighten the doc comment to say only the GPU `groundcover_scatter.comp` `affinity(splat)` dispatch is still pending, since the CPU-side consumer now exists. Do NOT touch the sibling `layer_affinities` (plural), which is still genuinely uncalled.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_TECH_DEBT_2026-09-11.md`.*
