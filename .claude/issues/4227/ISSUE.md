# TD9-001: Golden-frame pixel regression guard has zero effective coverage pending a human GPU regen

Labels: low,tech-debt,test-gap,renderer,bug

**Description**: The project's only pixel-level render regression guard cannot currently pass — the baseline PNG predates `--bench-mode renderer-static` (2026-08-11) and FSR3 becoming non-default (2026-07-22); 550+ renderer commits (272 touching shaders) have landed since the last real capture (`4376f7a6`, 2026-06-04). Closed #3849 (closed today via `f5127c1c`) converted the false-positive failure mode into an explicit staleness assertion, which is the correct mitigation for *that* problem but does not restore actual pixel-regression coverage — this finding tracks the still-needed manual regeneration step, not a regression of #3849's fix.

**Evidence**:
`byroredux/tests/golden_frames.rs:66-120`, `byroredux/tests/golden/cube_demo_60f.capture`; confirmed `BYROREDUX_REGEN_GOLDEN` env var and staleness-check plumbing present in current code.

**Impact**: Zero effective pixel-regression coverage across 550+ renderer commits until a human with a Vulkan device runs the one-time regen command. No code change needed to fix — this is a data/process gap, not a bug.

**Related**: Closed #3849 (added the staleness assertion; this finding is the residual actionable step it deliberately did not attempt).

**Suggested Fix**: No source change needed — a one-time manual regeneration (`BYROREDUX_REGEN_GOLDEN=1 cargo test --release -p byroredux -- --ignored cube_demo_golden_frame`) on a Vulkan-capable machine, committing the refreshed baseline + `.capture`.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_TECH_DEBT_2026-09-11.md`.*
