# REN-D11-2026-09-20-02: exposure-failure warn names 'default exposure constant' — the actual fallback is NO_EXPOSURE_RESOURCE_FALLBACK (1.0), not DEFAULT_EXPOSURE (0.85); #2833's conflation resurrected

- **ID**: REN-D11-2026-09-20-02
- **Labels**: low,renderer,bug
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: LOW · **Dimension**: FSR/Presentation
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-D11-2026-09-20-02)

**Location**: `crates/renderer/src/vulkan/context/init.rs:1000`

**Description**
The skill pins 'both fall back to the SAME value (NO_EXPOSURE_RESOURCE_FALLBACK, not DEFAULT_EXPOSURE)'; the warn message reintroduces exactly the ambiguity #2833 fixed.

**Evidence**
Audit D11, 2026-09-20.

**Impact**
An operator debugging exposure reads the wrong constant out of the log.

**Suggested Fix**
Fix the message to name NO_EXPOSURE_RESOURCE_FALLBACK.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
