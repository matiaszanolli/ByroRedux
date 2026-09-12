# TD6-002: `NiStencilProperty` state captured at parse time, never consumed by the renderer

Labels: medium,tech-debt,renderer,nif,bug

**Description**: The importer decodes and stores all 7 non-`draw_mode` `NiStencilProperty` fields, but `pipeline.rs` hardcodes `stencil_test_enable(false)` unconditionally regardless of the parsed state — the comment there explicitly states the data is "dormant until per-material stencil pipeline variants land." `grep -rn stencil_state crates/renderer` confirms zero non-comment reads outside that note. Closed issue #337 ("NiStencilProperty stencil state not mapped to Vulkan") covers exactly this gap and was closed without landing the mapping — the wiring described in its title is still absent.

**Evidence**:
`crates/nif/src/import/material/mod.rs:1732` (cross-reference); `MaterialInfo.stencil_state`; consumer gap confirmed at `crates/renderer/src/vulkan/pipeline.rs:449-457` (`stencil_test_enable(false)` unconditional).

**Impact**: Any authored two-sided-stencil / stencil-masked effect (portal masking, mirror clipping, some decal techniques) that relies on `NiStencilProperty` renders identically to content with no stencil property at all — visually silent divergence from source content, not a crash.

**Related**: Regression of #337 (closed, title matches this exact gap — the fix never landed the pipeline wiring).

**Suggested Fix**: Implement the stencil pipeline variant (the #337 follow-up that never shipped), or downgrade the "dormant" framing to an explicit ROADMAP known-gap entry so it isn't mistaken for done.



## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_TECH_DEBT_2026-09-11.md`.*
