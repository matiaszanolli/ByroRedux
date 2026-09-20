# REN-D7-2026-09-20-01: under --upscaler taa + a raw-output debug view, Halton jitter is still applied while the TAA resolve is skipped

- **ID**: REN-D7-2026-09-20-01
- **Labels**: medium,renderer,bug
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: MEDIUM · **Dimension**: TAA
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-D7-2026-09-20-01)

**Location**: `context/post_passes.rs` (`record_taa_pass` skip), `context/assemble_camera_and_lights.rs` (`taa_jitter`); #3632 folded the raw-output predicate into the FSR arm's `is_fsr_dispatch_active` only

**Description**
Raw-output correctness views under TAA mode render jittered-but-unresolved: the resolve is skipped (correct) but the projection jitter still applies, leaving persistent shimmer on raw oracles. Reachable automatically via the #2480 FSR-startup→TAA promotion (`init.rs:1754-1776`). The FSR arm received the equivalent fix in #3632; the TAA arm was missed.

**Evidence**
Skill/guard context: `taa_resolves_the_post_bloom_scene_tap` and `record_bloom_pass_skips_raw_correctness_views_before_dispatch` pin the neighboring behavior; nothing pins the TAA arm's jitter gate.

**Impact**
Raw-output debug oracles (render.debug correctness views) are unusable under TAA mode — the exact views that exist to be jitter-free.

**Suggested Fix**
Fold the raw-output predicate into the TAA arm's jitter decision, mirroring `is_fsr_dispatch_active`; extend `taa_and_fsr_negate_jitter_y_the_same_way`-style coverage.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
