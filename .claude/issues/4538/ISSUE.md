# REN-D11-2026-09-20-01: nothing enforces 'FSR dispatch ⇒ scene_color is SHADER_READ_ONLY_OPTIMAL' — record_fsr_barriers_before hard-codes it while scene_color_layout makes GENERAL representable

- **ID**: REN-D11-2026-09-20-01
- **Labels**: low,renderer,bug
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: LOW · **Dimension**: FSR/Presentation
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-D11-2026-09-20-01)

**Location**: `crates/renderer/src/vulkan/frame_upscaler.rs` / `context/post_passes.rs` — `record_fsr_barriers_before` + `scene_color_layout` (added by #3572)

**Description**
#3572 introduced a representable GENERAL alternative for the scene-color layout; the FSR barrier path still hard-codes SHADER_READ_ONLY. Unreachable today only via TAA/FSR construction exclusivity — the CRITICAL-floor default path relies on an invariant no assert or test holds.

**Evidence**
Audit D11, 2026-09-20.

**Impact**
A future layout refactor can silently hand FSR a GENERAL image with a SHADER_READ_ONLY barrier.

**Suggested Fix**
debug_assert! (or a match arm) tying the barrier's layout to the field; one source-shape test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
