# REN-D1-2026-09-20-02: census.actor_diverted_alpha_blend is structurally dead while rt.masks still publishes it and its display test hand-builds = 6

- **ID**: REN-D1-2026-09-20-02
- **Labels**: low,renderer,tech-debt,bug
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: LOW · **Dimension**: AS Correctness
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-D1-2026-09-20-02)

**Location**: `crates/renderer/src/vulkan/context/telemetry.rs:429` (publisher); increment site is Actor-guarded, and the divert returns AlphaBlend only for non-Actor

**Description**
After 84bbc44ed, no input can increment actor_diverted_alpha_blend, yet rt.masks still publishes the counter and the display test constructs a synthetic =6 row. The #3305 console oracle lost its alpha-divert signal without anyone noticing.

**Evidence**
Static trace during the 2026-09-20 audit (D1).

**Impact**
A dead telemetry lane reads as data in rt.masks; #3305 A/B work cannot see alpha diverts.

**Suggested Fix**
Either drop the counter and its display row or make it live again alongside the policy decision from #3305's A/B.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
