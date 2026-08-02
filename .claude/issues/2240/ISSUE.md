# REN-D15-02: Authored WATR wave amplitude/frequency are parsed and translated but never reach the GPU

Severity: medium
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2240

**Dimension**: 15 (Water)
**Location**: `byroredux/src/env_translate.rs` (`mat.wave_amplitude`/`mat.wave_frequency`, lines 111-112, round-tripped from WATR per the test at lines 643-667); no consumer in `crates/renderer/src/vulkan/scene_buffer/*.rs`, `crates/renderer/shaders/water.frag`, or `crates/renderer/shaders/water.vert`
**Status**: NEW

**Description**: Authored WATR `wave_amplitude`/`wave_frequency` are correctly parsed and round-tripped into the canonical `Material` (verified by `env_translate.rs`'s own regression test), but no GPU upload path or shader consumes them — every water surface in the renderer uses hardcoded/derived wave parameters regardless of what the source WATR record authored. Pre-existing gap (documented in the 2026-07-28 audit's prose); now tracked as its own finding.

**Impact**: Per-worldspace authored wave character (amplitude/frequency variation between different WATR records) is invisible in the renderer — all water looks the same regardless of authoring.

**Suggested Fix**: thread `wave_amplitude`/`wave_frequency` through the water material's GPU upload path and consume them in `water.vert`'s displacement and `water.frag`'s normal perturbation in place of the current hardcoded values.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
