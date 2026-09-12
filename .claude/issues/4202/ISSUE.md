# PERF-D5-2026-09-11-01: `CLAUDE.md` still attributes ACES tone mapping to the composite pass

Labels: low,performance,doc-rot,documentation

**Description**: Composite now emits render-resolution linear HDR; exposure + ACES moved to `presentation.frag` downstream of the upscale boundary (confirmed: `composite.frag`/`composite.rs` contain no ACES/tonemap symbol; `presentation.frag:43,162` defines and applies `aces()`; `docs/engine/fsr3-upscaler-integration-plan.md:133` records the move). `CLAUDE.md`'s lines still describe the old shape.

**Evidence**:
`CLAUDE.md:146,159`.

**Impact**: Actively misleading for the frame's most safety-critical ordering question — bloom runs *after* composite on the (correctly) linear-HDR image; a reader trusting the stale `CLAUDE.md` line would file that ordering as a MEDIUM defect and be wrong (a plausible false positive for every future pass over this area).

**Related**: None named.

**Suggested Fix**: Reword both lines to describe composite as linear-HDR reassembly and attribute exposure/ACES to `presentation.vert/frag` (which currently has no `CLAUDE.md` entry at all).



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md`.*
